//! Evolution engine: the glue that turns primitives into a working A/B loop.
//!
//! Called from `cmd_record` (after each session is inserted) and from
//! `cmd_roll` (manual challenger generation).

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use evolve_adapters::AdapterRegistry;
use evolve_core::ids::{ConfigId, ExperimentId, ProjectId, SessionId};
use evolve_core::promotion::{
    AggregationConfig, Decision, PromotionConfig, SignalInput, SignalKind as PromSignalKind,
    aggregate, promotion_decision,
};
use evolve_llm::LlmClient;
use evolve_mutators::{MutationCtx, MutatorPicker};
use evolve_storage::Storage;
use evolve_storage::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
use evolve_storage::experiments::{Experiment, ExperimentRepo, ExperimentStatus};
use evolve_storage::projects::{Project, ProjectRepo};
use evolve_storage::sessions::SessionRepo;
use evolve_storage::signals::{SignalKind as StorageSignalKind, SignalRepo};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::path::Path;

/// Convert storage Signal kind into promotion SignalKind.
fn map_kind(k: StorageSignalKind) -> PromSignalKind {
    match k {
        StorageSignalKind::Explicit => PromSignalKind::Explicit,
        StorageSignalKind::Implicit => PromSignalKind::Implicit,
    }
}

/// Load all signals tied to sessions that ran under `config_id`, group by
/// session, and collapse each session's signals into a single 0..=1 score.
pub async fn collect_scores_for_config(storage: &Storage, config_id: ConfigId) -> Result<Vec<f64>> {
    let signals = SignalRepo::new(storage).list_for_config(config_id).await?;
    let mut by_session: std::collections::HashMap<SessionId, Vec<SignalInput>> = Default::default();
    for sig in signals {
        by_session
            .entry(sig.session_id)
            .or_default()
            .push(SignalInput {
                kind: map_kind(sig.kind),
                value: sig.value,
            });
    }
    let cfg = AggregationConfig::default();
    Ok(by_session
        .values()
        .map(|sigs| aggregate(sigs, &cfg))
        .collect())
}

/// Evaluate the running experiment (if any) against the promotion threshold.
/// Returns the experiment + decision, or `None` if no experiment is running.
pub async fn evaluate_promotion(
    storage: &Storage,
    project_id: ProjectId,
) -> Result<Option<(Experiment, Decision)>> {
    let exp = match ExperimentRepo::new(storage)
        .get_running_for_project(project_id)
        .await?
    {
        Some(e) => e,
        None => return Ok(None),
    };
    let champion_scores = collect_scores_for_config(storage, exp.champion_config_id).await?;
    let challenger_scores = collect_scores_for_config(storage, exp.challenger_config_id).await?;

    let seed = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let cfg = PromotionConfig::default();
    let decision = promotion_decision(&champion_scores, &challenger_scores, &cfg, &mut rng);
    Ok(Some((exp, decision)))
}

/// Promote the challenger: mark experiment as Promoted, swap project's champion
/// pointer, and re-apply the new champion to disk via the adapter.
pub async fn promote_challenger(
    storage: &Storage,
    registry: &AdapterRegistry,
    project: &Project,
    experiment: &Experiment,
    posterior: f64,
) -> Result<()> {
    let now = Utc::now();
    ExperimentRepo::new(storage)
        .update_status(
            experiment.id,
            ExperimentStatus::Promoted,
            Some(now),
            Some(posterior),
        )
        .await?;
    ProjectRepo::new(storage)
        .set_champion(project.id, experiment.challenger_config_id)
        .await?;

    let cfg_row = AgentConfigRepo::new(storage)
        .get_by_id(experiment.challenger_config_id)
        .await?
        .ok_or_else(|| anyhow!("challenger config row missing"))?;
    if let Some(adapter) = registry.get(project.adapter_id.as_str()) {
        // Best-effort — the root may be gone in odd cases.
        let _ = adapter
            .apply_config(Path::new(&project.root_path), &cfg_row.payload)
            .await;
    }
    Ok(())
}

/// Generate a challenger from the current champion using one mutator, persist
/// it as an AgentConfig row with role=Challenger, start a new Experiment with
/// traffic_share=1.0 (v0.2.0 deploys the challenger full-time and compares
/// against the historical champion's session population), and apply the
/// challenger config to disk via the adapter.
pub async fn generate_challenger(
    storage: &Storage,
    registry: &AdapterRegistry,
    llm: &dyn LlmClient,
    project: &Project,
    rng: &mut ChaCha8Rng,
) -> Result<(ConfigId, ExperimentId)> {
    let champion_id = project
        .champion_config_id
        .ok_or_else(|| anyhow!("project has no champion"))?;
    let champion_row = AgentConfigRepo::new(storage)
        .get_by_id(champion_id)
        .await?
        .ok_or_else(|| anyhow!("champion config row missing"))?;

    let picker = MutatorPicker::default();
    let mutator = picker.pick(rng);
    let mut ctx = MutationCtx { llm, rng };
    let challenger_payload = mutator
        .mutate(&champion_row.payload, &mut ctx)
        .await
        .context("mutator failed")?;

    let challenger_id = ConfigId::new();
    AgentConfigRepo::new(storage)
        .insert(&AgentConfigRow {
            id: challenger_id,
            project_id: project.id,
            adapter_id: champion_row.adapter_id.clone(),
            role: ConfigRole::Challenger,
            fingerprint: challenger_payload.fingerprint(),
            payload: challenger_payload.clone(),
            created_at: Utc::now(),
        })
        .await?;

    let experiment_id = ExperimentId::new();
    ExperimentRepo::new(storage)
        .insert(&Experiment {
            id: experiment_id,
            project_id: project.id,
            champion_config_id: champion_id,
            challenger_config_id: challenger_id,
            status: ExperimentStatus::Running,
            traffic_share: 1.0,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        })
        .await?;

    if let Some(adapter) = registry.get(project.adapter_id.as_str()) {
        adapter
            .apply_config(Path::new(&project.root_path), &challenger_payload)
            .await
            .context("adapter apply_config failed")?;
    }

    Ok((challenger_id, experiment_id))
}

/// Default scheduler: trigger challenger generation when enough sessions have
/// accumulated since the last champion change. Skips if an experiment is
/// already running.
pub async fn should_evolve(
    storage: &Storage,
    project_id: ProjectId,
    threshold_sessions: u32,
) -> Result<bool> {
    if ExperimentRepo::new(storage)
        .get_running_for_project(project_id)
        .await?
        .is_some()
    {
        return Ok(false);
    }
    let sessions = SessionRepo::new(storage)
        .list_recent(project_id, threshold_sessions)
        .await?;
    Ok(sessions.len() as u32 >= threshold_sessions)
}

/// Figure out which variant + config_id a new session should be tagged with.
/// If an experiment is running: challenger variant on the challenger config.
/// Otherwise: champion variant on the project's champion config.
pub async fn resolve_active_deployment(
    storage: &Storage,
    project: &Project,
) -> Result<(
    evolve_storage::sessions::SessionVariant,
    ConfigId,
    Option<ExperimentId>,
)> {
    if let Some(exp) = ExperimentRepo::new(storage)
        .get_running_for_project(project.id)
        .await?
    {
        return Ok((
            evolve_storage::sessions::SessionVariant::Challenger,
            exp.challenger_config_id,
            Some(exp.id),
        ));
    }
    let champ = project
        .champion_config_id
        .ok_or_else(|| anyhow!("project has no champion"))?;
    Ok((
        evolve_storage::sessions::SessionVariant::Champion,
        champ,
        None,
    ))
}
