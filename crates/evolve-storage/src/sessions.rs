//! Repository for the `sessions` table.

use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::ids::{ConfigId, ExperimentId, ProjectId, SessionId};
use uuid::Uuid;

/// Which variant was active when this session ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionVariant {
    /// The project's champion config was applied.
    Champion,
    /// The active experiment's challenger config was applied.
    Challenger,
}

impl SessionVariant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Champion => "champion",
            Self::Challenger => "challenger",
        }
    }

    fn from_str(s: &str) -> Result<Self, StorageError> {
        Ok(match s {
            "champion" => Self::Champion,
            "challenger" => Self::Challenger,
            other => {
                return Err(StorageError::Sqlx(sqlx::Error::Decode(
                    format!("unknown session variant {other:?}").into(),
                )));
            }
        })
    }
}

/// One recorded user session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session identity.
    pub id: SessionId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Experiment active when the session started, if any.
    pub experiment_id: Option<ExperimentId>,
    /// Which variant was deployed for this session.
    pub variant: SessionVariant,
    /// The exact config row that was active.
    pub config_id: ConfigId,
    /// Start time.
    pub started_at: DateTime<Utc>,
    /// End time.
    pub ended_at: DateTime<Utc>,
    /// Opaque adapter-specific reference (e.g., transcript filename).
    pub adapter_session_ref: Option<String>,
}

/// Raw tuple shape returned by `SELECT`s on `sessions`. Factored out to keep
/// call sites under `clippy::type_complexity`.
type SessionRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    Option<String>,
);

/// Repository for `sessions`.
#[derive(Debug, Clone)]
pub struct SessionRepo<'a> {
    storage: &'a Storage,
}

impl<'a> SessionRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new session row.
    pub async fn insert(&self, s: &Session) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO sessions
                (id, project_id, experiment_id, variant, config_id,
                 started_at, ended_at, adapter_session_ref)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.id.to_string())
        .bind(s.project_id.to_string())
        .bind(s.experiment_id.map(|e| e.to_string()))
        .bind(s.variant.as_str())
        .bind(s.config_id.to_string())
        .bind(s.started_at.to_rfc3339())
        .bind(s.ended_at.to_rfc3339())
        .bind(s.adapter_session_ref.as_deref())
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// List most recent sessions for a project (descending by `started_at`).
    pub async fn list_recent(
        &self,
        project_id: ProjectId,
        limit: u32,
    ) -> Result<Vec<Session>, StorageError> {
        let rows: Vec<SessionRow> = sqlx::query_as(
            "SELECT id, project_id, experiment_id, variant, config_id,
                    started_at, ended_at, adapter_session_ref
             FROM sessions
             WHERE project_id = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(project_id.to_string())
        .bind(limit as i64)
        .fetch_all(self.storage.pool())
        .await?;
        rows.into_iter().map(row_to_session).collect()
    }

    /// All sessions belonging to a specific experiment.
    pub async fn list_for_experiment(
        &self,
        experiment_id: ExperimentId,
    ) -> Result<Vec<Session>, StorageError> {
        let rows: Vec<SessionRow> = sqlx::query_as(
            "SELECT id, project_id, experiment_id, variant, config_id,
                    started_at, ended_at, adapter_session_ref
             FROM sessions
             WHERE experiment_id = ?
             ORDER BY started_at DESC",
        )
        .bind(experiment_id.to_string())
        .fetch_all(self.storage.pool())
        .await?;
        rows.into_iter().map(row_to_session).collect()
    }
}

fn row_to_session(
    (id, project_id, experiment_id, variant, config_id, started_at, ended_at, ref_): SessionRow,
) -> Result<Session, StorageError> {
    let parse_ts = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))
    };
    Ok(Session {
        id: SessionId::from_uuid(Uuid::parse_str(&id)?),
        project_id: ProjectId::from_uuid(Uuid::parse_str(&project_id)?),
        experiment_id: experiment_id
            .map(|s| Uuid::parse_str(&s).map(ExperimentId::from_uuid))
            .transpose()?,
        variant: SessionVariant::from_str(&variant)?,
        config_id: ConfigId::from_uuid(Uuid::parse_str(&config_id)?),
        started_at: parse_ts(&started_at)?,
        ended_at: parse_ts(&ended_at)?,
        adapter_session_ref: ref_,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
    use crate::experiments::{Experiment, ExperimentRepo, ExperimentStatus};
    use crate::projects::{Project, ProjectRepo};
    use evolve_core::agent_config::AgentConfig;
    use evolve_core::ids::AdapterId;

    async fn seeded() -> (Storage, ProjectId, ConfigId, ConfigId, ExperimentId) {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let pid = ProjectId::new();
        ProjectRepo::new(&storage)
            .insert(&Project {
                id: pid,
                adapter_id: AdapterId::new("claude-code"),
                root_path: "/tmp/sessions-test".into(),
                name: "s".into(),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await
            .unwrap();

        let champ = AgentConfigRow {
            id: ConfigId::new(),
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: 1,
            payload: AgentConfig::default_for("claude-code"),
            created_at: Utc::now(),
        };
        let chall = AgentConfigRow {
            id: ConfigId::new(),
            role: ConfigRole::Challenger,
            ..champ.clone()
        };
        let cfg = AgentConfigRepo::new(&storage);
        cfg.insert(&champ).await.unwrap();
        cfg.insert(&chall).await.unwrap();

        let eid = ExperimentId::new();
        ExperimentRepo::new(&storage)
            .insert(&Experiment {
                id: eid,
                project_id: pid,
                champion_config_id: champ.id,
                challenger_config_id: chall.id,
                status: ExperimentStatus::Running,
                traffic_share: 0.1,
                started_at: Utc::now(),
                decided_at: None,
                decision_posterior: None,
            })
            .await
            .unwrap();

        (storage, pid, champ.id, chall.id, eid)
    }

    #[tokio::test]
    async fn list_recent_orders_newest_first_and_respects_limit() {
        let (storage, pid, champ, _chall, _eid) = seeded().await;
        let repo = SessionRepo::new(&storage);

        for i in 0..5 {
            let s = Session {
                id: SessionId::new(),
                project_id: pid,
                experiment_id: None,
                variant: SessionVariant::Champion,
                config_id: champ,
                started_at: Utc::now() - chrono::Duration::minutes(i * 10),
                ended_at: Utc::now() - chrono::Duration::minutes(i * 10)
                    + chrono::Duration::minutes(5),
                adapter_session_ref: Some(format!("transcript-{i}.jsonl")),
            };
            repo.insert(&s).await.unwrap();
        }
        let rows = repo.list_recent(pid, 3).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].started_at >= rows[1].started_at);
        assert!(rows[1].started_at >= rows[2].started_at);
    }

    #[tokio::test]
    async fn list_for_experiment_returns_only_tagged_sessions() {
        let (storage, pid, champ, chall, eid) = seeded().await;
        let repo = SessionRepo::new(&storage);

        let tagged = Session {
            id: SessionId::new(),
            project_id: pid,
            experiment_id: Some(eid),
            variant: SessionVariant::Challenger,
            config_id: chall,
            started_at: Utc::now(),
            ended_at: Utc::now(),
            adapter_session_ref: None,
        };
        let untagged = Session {
            id: SessionId::new(),
            project_id: pid,
            experiment_id: None,
            variant: SessionVariant::Champion,
            config_id: champ,
            started_at: Utc::now(),
            ended_at: Utc::now(),
            adapter_session_ref: None,
        };
        repo.insert(&tagged).await.unwrap();
        repo.insert(&untagged).await.unwrap();

        let got = repo.list_for_experiment(eid).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, tagged.id);
    }
}
