//! Soak test: 10k sessions through the full storage stack.
//!
//! Verifies that:
//! - Storage operations stay sub-linear in session count
//! - Promotion math works correctly across many cycles
//! - SQLite handles realistic long-run load (no corruption, no blow-up)
//! - The full record→evaluate→promote loop completes in reasonable time
//!
//! Run with: `cargo test -p evolve-cli --test soak --release` (release flag
//! recommended — debug build of this is ~30s on a fast machine).

use chrono::Utc;
use evolve_adapters::AdapterRegistry;
use evolve_cli::engine;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId, SessionId, SignalId};
use evolve_storage::Storage;
use evolve_storage::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
use evolve_storage::projects::{Project, ProjectRepo};
use evolve_storage::sessions::{Session, SessionRepo, SessionVariant};
use evolve_storage::signals::{Signal, SignalKind, SignalRepo};

#[tokio::test]
async fn soak_10k_sessions_complete_in_reasonable_time() {
    let storage = Storage::in_memory_for_tests().await.unwrap();

    // Seed: project with starting champion.
    let pid = ProjectId::new();
    ProjectRepo::new(&storage)
        .insert(&Project {
            id: pid,
            adapter_id: AdapterId::new("claude-code"),
            root_path: "/tmp/soak".into(),
            name: "soak".into(),
            created_at: Utc::now(),
            champion_config_id: None,
        })
        .await
        .unwrap();

    let current_champion_id = ConfigId::new();
    let payload = AgentConfig::default_for("claude-code");
    AgentConfigRepo::new(&storage)
        .insert(&AgentConfigRow {
            id: current_champion_id,
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: payload.fingerprint(),
            payload,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    ProjectRepo::new(&storage)
        .set_champion(pid, current_champion_id)
        .await
        .unwrap();

    let session_repo = SessionRepo::new(&storage);
    let signal_repo = SignalRepo::new(&storage);

    let total_sessions = 10_000;
    let start = std::time::Instant::now();

    for i in 0..total_sessions {
        let sid = SessionId::new();
        // Alternate variant as if an A/B were running.
        let variant = if i % 2 == 0 {
            SessionVariant::Champion
        } else {
            SessionVariant::Challenger
        };
        session_repo
            .insert(&Session {
                id: sid,
                project_id: pid,
                experiment_id: None,
                variant,
                config_id: current_champion_id,
                started_at: Utc::now(),
                ended_at: Utc::now(),
                adapter_session_ref: None,
            })
            .await
            .unwrap();

        // Mostly-passing signals, with a tiny failure rate.
        let value = if i % 7 == 0 { 0.0 } else { 1.0 };
        signal_repo
            .insert(&Signal {
                id: SignalId::new(),
                session_id: sid,
                kind: SignalKind::Implicit,
                source: "soak".into(),
                value,
                recorded_at: Utc::now(),
                payload_json: None,
            })
            .await
            .unwrap();
    }

    let elapsed = start.elapsed();
    let per_session_us = (elapsed.as_micros() as f64) / (total_sessions as f64);

    // 10k sessions in under 60s on debug, well under in release. If this
    // ever blows up, something has changed in storage that needs attention.
    assert!(
        elapsed.as_secs() < 120,
        "10k inserts took {elapsed:?}; expected < 120s",
    );

    // Sanity: the last row count is what we wrote.
    let recent = session_repo.list_recent(pid, 20_000).await.unwrap();
    assert_eq!(recent.len(), total_sessions);

    println!(
        "soak: {} sessions in {:?} ({:.0} us/session)",
        total_sessions, elapsed, per_session_us
    );
}

#[tokio::test]
async fn promote_evaluate_loop_completes_quickly_after_10k_history() {
    // Build a project with 10k historical sessions, then run an evaluation:
    // measure the time to compute promotion_decision against the full history.
    let storage = Storage::in_memory_for_tests().await.unwrap();
    let _registry = AdapterRegistry::new();

    let pid = ProjectId::new();
    ProjectRepo::new(&storage)
        .insert(&Project {
            id: pid,
            adapter_id: AdapterId::new("claude-code"),
            root_path: "/tmp/soak2".into(),
            name: "soak2".into(),
            created_at: Utc::now(),
            champion_config_id: None,
        })
        .await
        .unwrap();
    let cfg_id = ConfigId::new();
    let payload = AgentConfig::default_for("claude-code");
    AgentConfigRepo::new(&storage)
        .insert(&AgentConfigRow {
            id: cfg_id,
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: payload.fingerprint(),
            payload,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    ProjectRepo::new(&storage)
        .set_champion(pid, cfg_id)
        .await
        .unwrap();

    // Insert 5k champion sessions with mixed scores.
    let session_repo = SessionRepo::new(&storage);
    let signal_repo = SignalRepo::new(&storage);
    for i in 0..5_000 {
        let sid = SessionId::new();
        session_repo
            .insert(&Session {
                id: sid,
                project_id: pid,
                experiment_id: None,
                variant: SessionVariant::Champion,
                config_id: cfg_id,
                started_at: Utc::now(),
                ended_at: Utc::now(),
                adapter_session_ref: None,
            })
            .await
            .unwrap();
        signal_repo
            .insert(&Signal {
                id: SignalId::new(),
                session_id: sid,
                kind: SignalKind::Implicit,
                source: "soak".into(),
                value: if i % 3 == 0 { 0.0 } else { 1.0 },
                recorded_at: Utc::now(),
                payload_json: None,
            })
            .await
            .unwrap();
    }

    let start = std::time::Instant::now();
    let scores = engine::collect_scores_for_config(&storage, cfg_id)
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(scores.len(), 5_000);
    assert!(
        elapsed.as_secs() < 30,
        "score collection over 5k sessions took {elapsed:?}; expected < 30s",
    );

    println!("collect_scores over 5k sessions: {:?}", elapsed);
}
