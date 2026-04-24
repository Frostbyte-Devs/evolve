//! Repository for the `experiments` table.

use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::ids::{ConfigId, ExperimentId, ProjectId};
use uuid::Uuid;

/// Experiment lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperimentStatus {
    /// Actively collecting signals.
    Running,
    /// Challenger won; promoted to champion.
    Promoted,
    /// Manually cancelled or superseded.
    Aborted,
    /// Decision reached but champion kept.
    Held,
}

impl ExperimentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Promoted => "promoted",
            Self::Aborted => "aborted",
            Self::Held => "held",
        }
    }

    fn from_str(s: &str) -> Result<Self, StorageError> {
        Ok(match s {
            "running" => Self::Running,
            "promoted" => Self::Promoted,
            "aborted" => Self::Aborted,
            "held" => Self::Held,
            other => {
                return Err(StorageError::Sqlx(sqlx::Error::Decode(
                    format!("unknown experiment status {other:?}").into(),
                )));
            }
        })
    }
}

/// One champion-vs-challenger experiment row.
#[derive(Debug, Clone)]
pub struct Experiment {
    /// Experiment identity.
    pub id: ExperimentId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Champion config under test.
    pub champion_config_id: ConfigId,
    /// Challenger config under test.
    pub challenger_config_id: ConfigId,
    /// Lifecycle state.
    pub status: ExperimentStatus,
    /// Share of sessions routed to challenger (0..=1).
    pub traffic_share: f64,
    /// When the experiment started.
    pub started_at: DateTime<Utc>,
    /// When the decision was reached (only set for non-Running statuses).
    pub decided_at: Option<DateTime<Utc>>,
    /// P(challenger > champion) at decision time.
    pub decision_posterior: Option<f64>,
}

/// Raw tuple shape returned by `SELECT`s on `experiments`. Factored out so
/// `fetch_optional` / `fetch_all` call sites stay under `clippy::type_complexity`.
type ExperimentRow = (
    String,
    String,
    String,
    String,
    String,
    f64,
    String,
    Option<String>,
    Option<f64>,
);

/// Repository for `experiments`.
#[derive(Debug, Clone)]
pub struct ExperimentRepo<'a> {
    storage: &'a Storage,
}

impl<'a> ExperimentRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new experiment.
    pub async fn insert(&self, exp: &Experiment) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO experiments
                (id, project_id, champion_config_id, challenger_config_id,
                 status, traffic_share, started_at, decided_at, decision_posterior)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(exp.id.to_string())
        .bind(exp.project_id.to_string())
        .bind(exp.champion_config_id.to_string())
        .bind(exp.challenger_config_id.to_string())
        .bind(exp.status.as_str())
        .bind(exp.traffic_share)
        .bind(exp.started_at.to_rfc3339())
        .bind(exp.decided_at.map(|d| d.to_rfc3339()))
        .bind(exp.decision_posterior)
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// Return the single running experiment for a project, if any.
    pub async fn get_running_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<Experiment>, StorageError> {
        let row: Option<ExperimentRow> = sqlx::query_as(
            "SELECT id, project_id, champion_config_id, challenger_config_id,
                    status, traffic_share, started_at, decided_at, decision_posterior
             FROM experiments
             WHERE project_id = ? AND status = 'running'
             LIMIT 1",
        )
        .bind(project_id.to_string())
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_experiment).transpose()
    }

    /// List all non-Running experiments for a project, most recent first.
    pub async fn list_completed(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<Experiment>, StorageError> {
        let rows: Vec<ExperimentRow> = sqlx::query_as(
            "SELECT id, project_id, champion_config_id, challenger_config_id,
                    status, traffic_share, started_at, decided_at, decision_posterior
             FROM experiments
             WHERE project_id = ? AND status != 'running'
             ORDER BY COALESCE(decided_at, started_at) DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(self.storage.pool())
        .await?;
        rows.into_iter().map(row_to_experiment).collect()
    }

    /// Update the lifecycle state (and decision timestamp + posterior on terminal transitions).
    pub async fn update_status(
        &self,
        id: ExperimentId,
        status: ExperimentStatus,
        decided_at: Option<DateTime<Utc>>,
        decision_posterior: Option<f64>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE experiments
             SET status = ?, decided_at = ?, decision_posterior = ?
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(decided_at.map(|d| d.to_rfc3339()))
        .bind(decision_posterior)
        .bind(id.to_string())
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }
}

fn row_to_experiment(
    (
        id,
        project_id,
        champion,
        challenger,
        status,
        traffic_share,
        started_at,
        decided_at,
        posterior,
    ): ExperimentRow,
) -> Result<Experiment, StorageError> {
    Ok(Experiment {
        id: ExperimentId::from_uuid(Uuid::parse_str(&id)?),
        project_id: ProjectId::from_uuid(Uuid::parse_str(&project_id)?),
        champion_config_id: ConfigId::from_uuid(Uuid::parse_str(&champion)?),
        challenger_config_id: ConfigId::from_uuid(Uuid::parse_str(&challenger)?),
        status: ExperimentStatus::from_str(&status)?,
        traffic_share,
        started_at: DateTime::parse_from_rfc3339(&started_at)
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))?
            .with_timezone(&Utc),
        decided_at: decided_at
            .map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|d| d.with_timezone(&Utc))
                    .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))
            })
            .transpose()?,
        decision_posterior: posterior,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
    use crate::projects::{Project, ProjectRepo};
    use evolve_core::agent_config::AgentConfig;
    use evolve_core::ids::AdapterId;

    async fn seeded() -> (Storage, ProjectId, ConfigId, ConfigId) {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let pid = ProjectId::new();
        ProjectRepo::new(&storage)
            .insert(&Project {
                id: pid,
                adapter_id: AdapterId::new("claude-code"),
                root_path: "/tmp/experiments-test".into(),
                name: "x".into(),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await
            .unwrap();

        let champion = AgentConfigRow {
            id: ConfigId::new(),
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: 1,
            payload: AgentConfig::default_for("claude-code"),
            created_at: Utc::now(),
        };
        let challenger = AgentConfigRow {
            id: ConfigId::new(),
            role: ConfigRole::Challenger,
            fingerprint: 2,
            ..champion.clone()
        };
        let cfg = AgentConfigRepo::new(&storage);
        cfg.insert(&champion).await.unwrap();
        cfg.insert(&challenger).await.unwrap();

        (storage, pid, champion.id, challenger.id)
    }

    #[tokio::test]
    async fn insert_and_get_running_returns_the_row() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);
        let exp = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        repo.insert(&exp).await.unwrap();
        let back = repo.get_running_for_project(pid).await.unwrap().unwrap();
        assert_eq!(back.id, exp.id);
    }

    #[tokio::test]
    async fn only_one_running_experiment_per_project_is_allowed() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);

        let first = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        let second = Experiment {
            id: ExperimentId::new(),
            ..first.clone()
        };
        repo.insert(&first).await.unwrap();
        let err = repo.insert(&second).await.unwrap_err();
        assert!(matches!(err, StorageError::Sqlx(sqlx::Error::Database(_))));
    }

    #[tokio::test]
    async fn update_status_to_promoted_sets_decided_at_and_posterior() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);
        let exp = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        repo.insert(&exp).await.unwrap();
        let decided = Utc::now();
        repo.update_status(
            exp.id,
            ExperimentStatus::Promoted,
            Some(decided),
            Some(0.97),
        )
        .await
        .unwrap();

        let completed = repo.list_completed(pid).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].status, ExperimentStatus::Promoted);
        assert_eq!(completed[0].decision_posterior, Some(0.97));
    }

    #[tokio::test]
    async fn list_completed_excludes_running() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);
        let exp = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        repo.insert(&exp).await.unwrap();
        assert!(repo.list_completed(pid).await.unwrap().is_empty());
    }
}
