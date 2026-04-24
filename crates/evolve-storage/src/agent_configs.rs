//! Repository for the `agent_configs` table.

use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId};
use uuid::Uuid;

/// The role an AgentConfig plays for its project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRole {
    /// Currently-deployed default.
    Champion,
    /// Variant being A/B-tested against the champion.
    Challenger,
    /// Retired — kept for the promotion log.
    Historical,
}

impl ConfigRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Champion => "champion",
            Self::Challenger => "challenger",
            Self::Historical => "historical",
        }
    }

    fn from_str(s: &str) -> Result<Self, StorageError> {
        Ok(match s {
            "champion" => Self::Champion,
            "challenger" => Self::Challenger,
            "historical" => Self::Historical,
            other => {
                return Err(StorageError::Sqlx(sqlx::Error::Decode(
                    format!("unknown config role {other:?}").into(),
                )));
            }
        })
    }
}

/// A stored AgentConfig row.
#[derive(Debug, Clone)]
pub struct AgentConfigRow {
    /// Row id.
    pub id: ConfigId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Adapter this config targets.
    pub adapter_id: AdapterId,
    /// Role at time of insertion (can be promoted/retired later).
    pub role: ConfigRole,
    /// Stable hash of the payload (from [`AgentConfig::fingerprint`]).
    pub fingerprint: u64,
    /// The config itself.
    pub payload: AgentConfig,
    /// When this row was inserted.
    pub created_at: DateTime<Utc>,
}

/// Repository for `agent_configs`.
#[derive(Debug, Clone)]
pub struct AgentConfigRepo<'a> {
    storage: &'a Storage,
}

impl<'a> AgentConfigRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new config row. Caller owns the id.
    pub async fn insert(&self, row: &AgentConfigRow) -> Result<(), StorageError> {
        let payload_json = serde_json::to_string(&row.payload)?;
        sqlx::query(
            "INSERT INTO agent_configs
                (id, project_id, adapter_id, role, fingerprint, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.to_string())
        .bind(row.project_id.to_string())
        .bind(row.adapter_id.as_str())
        .bind(row.role.as_str())
        .bind(row.fingerprint as i64) // bit-cast u64 -> i64; see Phase 2 design decisions
        .bind(payload_json)
        .bind(row.created_at.to_rfc3339())
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// Fetch by id.
    pub async fn get_by_id(&self, id: ConfigId) -> Result<Option<AgentConfigRow>, StorageError> {
        let row: Option<(String, String, String, String, i64, String, String)> = sqlx::query_as(
            "SELECT id, project_id, adapter_id, role, fingerprint, payload_json, created_at
             FROM agent_configs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_agent_config).transpose()
    }

    /// Return the most recently created row for `(project, role)`.
    pub async fn latest_for_project_role(
        &self,
        project_id: ProjectId,
        role: ConfigRole,
    ) -> Result<Option<AgentConfigRow>, StorageError> {
        let row: Option<(String, String, String, String, i64, String, String)> = sqlx::query_as(
            "SELECT id, project_id, adapter_id, role, fingerprint, payload_json, created_at
             FROM agent_configs
             WHERE project_id = ? AND role = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(project_id.to_string())
        .bind(role.as_str())
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_agent_config).transpose()
    }
}

fn row_to_agent_config(
    (id, project_id, adapter_id, role, fingerprint, payload_json, created_at): (
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
    ),
) -> Result<AgentConfigRow, StorageError> {
    Ok(AgentConfigRow {
        id: ConfigId::from_uuid(Uuid::parse_str(&id)?),
        project_id: ProjectId::from_uuid(Uuid::parse_str(&project_id)?),
        adapter_id: AdapterId::new(adapter_id),
        role: ConfigRole::from_str(&role)?,
        fingerprint: fingerprint as u64,
        payload: serde_json::from_str(&payload_json)?,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))?
            .with_timezone(&Utc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{Project, ProjectRepo};

    async fn seeded_storage() -> (Storage, ProjectId) {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let project = Project {
            id: ProjectId::new(),
            adapter_id: AdapterId::new("claude-code"),
            root_path: "/tmp/agent-config-repo-test".into(),
            name: "test".into(),
            created_at: Utc::now(),
            champion_config_id: None,
        };
        ProjectRepo::new(&storage).insert(&project).await.unwrap();
        let pid = project.id;
        (storage, pid)
    }

    fn sample_row(project_id: ProjectId, role: ConfigRole) -> AgentConfigRow {
        let payload = AgentConfig::default_for("claude-code");
        AgentConfigRow {
            id: ConfigId::new(),
            project_id,
            adapter_id: AdapterId::new("claude-code"),
            role,
            fingerprint: payload.fingerprint(),
            payload,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn insert_and_get_by_id_roundtrips_full_payload() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);
        let row = sample_row(pid, ConfigRole::Champion);
        repo.insert(&row).await.unwrap();
        let back = repo.get_by_id(row.id).await.unwrap().unwrap();
        assert_eq!(back.id, row.id);
        assert_eq!(back.role, ConfigRole::Champion);
        assert_eq!(back.fingerprint, row.fingerprint);
        assert_eq!(back.payload, row.payload);
    }

    #[tokio::test]
    async fn latest_for_project_role_returns_most_recent() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);

        let older = AgentConfigRow {
            created_at: Utc::now() - chrono::Duration::hours(2),
            ..sample_row(pid, ConfigRole::Champion)
        };
        let newer = sample_row(pid, ConfigRole::Champion);
        repo.insert(&older).await.unwrap();
        repo.insert(&newer).await.unwrap();

        let latest = repo
            .latest_for_project_role(pid, ConfigRole::Champion)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, newer.id);
    }

    #[tokio::test]
    async fn latest_for_project_role_returns_none_when_no_rows() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);
        assert!(
            repo.latest_for_project_role(pid, ConfigRole::Challenger)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn cascade_delete_removes_configs() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);
        let row = sample_row(pid, ConfigRole::Champion);
        repo.insert(&row).await.unwrap();

        ProjectRepo::new(&storage).delete(pid).await.unwrap();
        assert!(repo.get_by_id(row.id).await.unwrap().is_none());
    }
}
