//! End-to-end: open -> write -> drop -> reopen -> read the same data.

use chrono::Utc;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId, SessionId, SignalId};
use evolve_storage::Storage;
use evolve_storage::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
use evolve_storage::projects::{Project, ProjectRepo};
use evolve_storage::sessions::{Session, SessionRepo, SessionVariant};
use evolve_storage::signals::{Signal, SignalKind, SignalRepo};
use tempfile::TempDir;

#[tokio::test]
async fn data_survives_process_restart() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("evolve.db");

    let project_id = ProjectId::new();
    let config_id = ConfigId::new();

    {
        let storage = Storage::open(&db_path).await.unwrap();
        ProjectRepo::new(&storage)
            .insert(&Project {
                id: project_id,
                adapter_id: AdapterId::new("claude-code"),
                root_path: "/tmp/restart-test".into(),
                name: "restart".into(),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await
            .unwrap();

        AgentConfigRepo::new(&storage)
            .insert(&AgentConfigRow {
                id: config_id,
                project_id,
                adapter_id: AdapterId::new("claude-code"),
                role: ConfigRole::Champion,
                fingerprint: 42,
                payload: AgentConfig::default_for("claude-code"),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        // storage goes out of scope -> pool closes
    }

    // Reopen and verify.
    let storage = Storage::open(&db_path).await.unwrap();
    let back_project = ProjectRepo::new(&storage)
        .get_by_id(project_id)
        .await
        .unwrap()
        .expect("project should survive restart");
    assert_eq!(back_project.id, project_id);

    let back_cfg = AgentConfigRepo::new(&storage)
        .get_by_id(config_id)
        .await
        .unwrap()
        .expect("config should survive restart");
    assert_eq!(back_cfg.id, config_id);
    assert_eq!(back_cfg.fingerprint, 42);
}

#[tokio::test]
async fn cascade_delete_project_removes_everything_downstream() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("evolve.db");
    let storage = Storage::open(&db_path).await.unwrap();

    let project = Project {
        id: ProjectId::new(),
        adapter_id: AdapterId::new("claude-code"),
        root_path: "/tmp/cascade-test".into(),
        name: "cascade".into(),
        created_at: Utc::now(),
        champion_config_id: None,
    };
    ProjectRepo::new(&storage).insert(&project).await.unwrap();

    let cfg = AgentConfigRow {
        id: ConfigId::new(),
        project_id: project.id,
        adapter_id: AdapterId::new("claude-code"),
        role: ConfigRole::Champion,
        fingerprint: 1,
        payload: AgentConfig::default_for("claude-code"),
        created_at: Utc::now(),
    };
    AgentConfigRepo::new(&storage).insert(&cfg).await.unwrap();

    let session = Session {
        id: SessionId::new(),
        project_id: project.id,
        experiment_id: None,
        variant: SessionVariant::Champion,
        config_id: cfg.id,
        started_at: Utc::now(),
        ended_at: Utc::now(),
        adapter_session_ref: None,
    };
    SessionRepo::new(&storage).insert(&session).await.unwrap();

    let signal = Signal {
        id: SignalId::new(),
        session_id: session.id,
        kind: SignalKind::Implicit,
        source: "x".into(),
        value: 0.5,
        recorded_at: Utc::now(),
        payload_json: None,
    };
    SignalRepo::new(&storage).insert(&signal).await.unwrap();

    // Delete project and re-check.
    ProjectRepo::new(&storage).delete(project.id).await.unwrap();

    assert!(
        ProjectRepo::new(&storage)
            .get_by_id(project.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        AgentConfigRepo::new(&storage)
            .get_by_id(cfg.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        SignalRepo::new(&storage)
            .list_for_session(session.id)
            .await
            .unwrap()
            .is_empty()
    );
}
