//! End-to-end evolution loop test.
//!
//! Proves that the engine correctly promotes a challenger when its session
//! scores beat the champion's. This is the test that should have existed
//! before claiming the product worked.

use async_trait::async_trait;
use chrono::Utc;
use evolve_adapters::AdapterRegistry;
use evolve_cli::engine;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId, SessionId, SignalId};
use evolve_core::promotion::Decision;
use evolve_llm::{CompletionResult, LlmClient, LlmError, TokenUsage};
use evolve_storage::Storage;
use evolve_storage::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
use evolve_storage::projects::{Project, ProjectRepo};
use evolve_storage::sessions::{Session, SessionRepo, SessionVariant};
use evolve_storage::signals::{Signal, SignalKind, SignalRepo};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Debug)]
struct MockLlm;

#[async_trait]
impl LlmClient for MockLlm {
    async fn complete(&self, _: &str, _: u32) -> Result<CompletionResult, LlmError> {
        Ok(CompletionResult {
            text: "A varied prefix proposed by the mock LLM for the test.".into(),
            usage: TokenUsage::default(),
        })
    }

    fn model_id(&self) -> &str {
        "mock"
    }
}

async fn seed_project_with_champion(storage: &Storage) -> (Project, ConfigId) {
    let pid = ProjectId::new();
    let project = Project {
        id: pid,
        adapter_id: AdapterId::new("claude-code"),
        root_path: "/tmp/e2e-test".into(),
        name: "e2e".into(),
        created_at: Utc::now(),
        champion_config_id: None,
    };
    ProjectRepo::new(storage).insert(&project).await.unwrap();

    let champ_id = ConfigId::new();
    let payload = AgentConfig::default_for("claude-code");
    AgentConfigRepo::new(storage)
        .insert(&AgentConfigRow {
            id: champ_id,
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: payload.fingerprint(),
            payload,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    ProjectRepo::new(storage)
        .set_champion(pid, champ_id)
        .await
        .unwrap();
    let project = ProjectRepo::new(storage)
        .get_by_id(pid)
        .await
        .unwrap()
        .unwrap();
    (project, champ_id)
}

async fn insert_session_with_score(
    storage: &Storage,
    project_id: ProjectId,
    config_id: ConfigId,
    variant: SessionVariant,
    experiment_id: Option<evolve_core::ids::ExperimentId>,
    score: f64,
) {
    let sid = SessionId::new();
    SessionRepo::new(storage)
        .insert(&Session {
            id: sid,
            project_id,
            experiment_id,
            variant,
            config_id,
            started_at: Utc::now(),
            ended_at: Utc::now(),
            adapter_session_ref: None,
        })
        .await
        .unwrap();
    SignalRepo::new(storage)
        .insert(&Signal {
            id: SignalId::new(),
            session_id: sid,
            kind: SignalKind::Implicit,
            source: "test".into(),
            value: score,
            recorded_at: Utc::now(),
            payload_json: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn challenger_promotes_when_outperforming_champion() {
    let storage = Storage::in_memory_for_tests().await.unwrap();
    let registry = AdapterRegistry::new();
    let (project, champ_id) = seed_project_with_champion(&storage).await;

    // 25 champion sessions: 5 wins, 20 losses (success rate ~ 20%).
    for i in 0..25 {
        let score = if i < 5 { 1.0 } else { 0.0 };
        insert_session_with_score(
            &storage,
            project.id,
            champ_id,
            SessionVariant::Champion,
            None,
            score,
        )
        .await;
    }

    // Generate the challenger.
    let mock = MockLlm;
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let (chall_id, exp_id) =
        engine::generate_challenger(&storage, &registry, &mock, &project, &mut rng)
            .await
            .unwrap();

    // 25 challenger sessions: 23 wins, 2 losses (success rate ~ 92%).
    for i in 0..25 {
        let score = if i < 23 { 1.0 } else { 0.0 };
        insert_session_with_score(
            &storage,
            project.id,
            chall_id,
            SessionVariant::Challenger,
            Some(exp_id),
            score,
        )
        .await;
    }

    // The decision should be Promote.
    let (_exp, decision) = engine::evaluate_promotion(&storage, project.id)
        .await
        .unwrap()
        .unwrap();

    match decision {
        Decision::Promote { posterior } => {
            assert!(
                posterior >= 0.95,
                "expected posterior >= 0.95, got {posterior}",
            );
        }
        other => panic!("expected Promote, got {other:?}"),
    }
}

#[tokio::test]
async fn obvious_loser_holds_at_low_posterior() {
    let storage = Storage::in_memory_for_tests().await.unwrap();
    let registry = AdapterRegistry::new();
    let (project, champ_id) = seed_project_with_champion(&storage).await;

    // 25 champion sessions: 23 wins, 2 losses.
    for i in 0..25 {
        let score = if i < 23 { 1.0 } else { 0.0 };
        insert_session_with_score(
            &storage,
            project.id,
            champ_id,
            SessionVariant::Champion,
            None,
            score,
        )
        .await;
    }

    let mock = MockLlm;
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let (chall_id, exp_id) =
        engine::generate_challenger(&storage, &registry, &mock, &project, &mut rng)
            .await
            .unwrap();

    // 25 challenger sessions: 5 wins, 20 losses (much worse).
    for i in 0..25 {
        let score = if i < 5 { 1.0 } else { 0.0 };
        insert_session_with_score(
            &storage,
            project.id,
            chall_id,
            SessionVariant::Challenger,
            Some(exp_id),
            score,
        )
        .await;
    }

    let (_, decision) = engine::evaluate_promotion(&storage, project.id)
        .await
        .unwrap()
        .unwrap();

    assert!(
        matches!(decision, Decision::Hold { posterior } if posterior < 0.05),
        "expected Hold with low posterior, got {decision:?}",
    );
}

#[tokio::test]
async fn promote_challenger_swaps_project_champion() {
    let storage = Storage::in_memory_for_tests().await.unwrap();
    let registry = AdapterRegistry::new();
    let (project, _champ_id) = seed_project_with_champion(&storage).await;

    let mock = MockLlm;
    let mut rng = ChaCha8Rng::seed_from_u64(13);
    let (chall_id, _exp_id) =
        engine::generate_challenger(&storage, &registry, &mock, &project, &mut rng)
            .await
            .unwrap();

    // Stuff in enough scores that promote_challenger has work to do.
    let exp = evolve_storage::experiments::ExperimentRepo::new(&storage)
        .get_running_for_project(project.id)
        .await
        .unwrap()
        .unwrap();

    engine::promote_challenger(&storage, &registry, &project, &exp, 0.97)
        .await
        .unwrap();

    // Project's champion pointer should now be the challenger config.
    let after = ProjectRepo::new(&storage)
        .get_by_id(project.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.champion_config_id, Some(chall_id));

    // Experiment should be marked Promoted.
    let completed = evolve_storage::experiments::ExperimentRepo::new(&storage)
        .list_completed(project.id)
        .await
        .unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(
        completed[0].status,
        evolve_storage::experiments::ExperimentStatus::Promoted
    );
    assert_eq!(completed[0].decision_posterior, Some(0.97));
}

#[tokio::test]
async fn no_experiment_running_returns_none() {
    let storage = Storage::in_memory_for_tests().await.unwrap();
    let (project, _) = seed_project_with_champion(&storage).await;
    let result = engine::evaluate_promotion(&storage, project.id)
        .await
        .unwrap();
    assert!(result.is_none());
}
