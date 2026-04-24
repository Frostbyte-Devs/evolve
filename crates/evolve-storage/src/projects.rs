//! Repository for the `projects` table.

use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::ids::{AdapterId, ConfigId, ProjectId};
use uuid::Uuid;

/// Row in the `projects` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// Project identity.
    pub id: ProjectId,
    /// Adapter that manages this project ("claude-code", "cursor", "aider").
    pub adapter_id: AdapterId,
    /// Canonical absolute path to the project root.
    pub root_path: String,
    /// Human-readable name (usually the basename of `root_path`).
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Current champion config id; `None` only during `init` before the first
    /// AgentConfig row has been written.
    pub champion_config_id: Option<ConfigId>,
}

/// Repository for `projects`.
#[derive(Debug, Clone)]
pub struct ProjectRepo<'a> {
    storage: &'a Storage,
}

impl<'a> ProjectRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new project row. Caller supplies the id.
    pub async fn insert(&self, project: &Project) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO projects
                (id, adapter_id, root_path, name, created_at, champion_config_id)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(project.id.to_string())
        .bind(project.adapter_id.as_str())
        .bind(&project.root_path)
        .bind(&project.name)
        .bind(project.created_at.to_rfc3339())
        .bind(project.champion_config_id.map(|c| c.to_string()))
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// Fetch by id; returns `Ok(None)` if no row matches.
    pub async fn get_by_id(&self, id: ProjectId) -> Result<Option<Project>, StorageError> {
        let row: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, adapter_id, root_path, name, created_at, champion_config_id
             FROM projects WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_project).transpose()
    }

    /// Fetch by root path.
    pub async fn get_by_root_path(&self, root: &str) -> Result<Option<Project>, StorageError> {
        let row: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, adapter_id, root_path, name, created_at, champion_config_id
             FROM projects WHERE root_path = ?",
        )
        .bind(root)
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_project).transpose()
    }

    /// List all projects, most recently created first.
    pub async fn list(&self) -> Result<Vec<Project>, StorageError> {
        let rows: Vec<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, adapter_id, root_path, name, created_at, champion_config_id
             FROM projects ORDER BY created_at DESC",
        )
        .fetch_all(self.storage.pool())
        .await?;
        rows.into_iter().map(row_to_project).collect()
    }

    /// Delete a project and cascade (configs, experiments, sessions, signals go too).
    pub async fn delete(&self, id: ProjectId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await?;
        Ok(())
    }

    /// Update the champion config pointer (used when a challenger is promoted).
    pub async fn set_champion(
        &self,
        id: ProjectId,
        config_id: ConfigId,
    ) -> Result<(), StorageError> {
        sqlx::query("UPDATE projects SET champion_config_id = ? WHERE id = ?")
            .bind(config_id.to_string())
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await?;
        Ok(())
    }
}

fn row_to_project(
    (id, adapter_id, root_path, name, created_at, champion): (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
    ),
) -> Result<Project, StorageError> {
    Ok(Project {
        id: ProjectId::from_uuid(Uuid::parse_str(&id)?),
        adapter_id: AdapterId::new(adapter_id),
        root_path,
        name,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))?
            .with_timezone(&Utc),
        champion_config_id: champion
            .map(|s| Uuid::parse_str(&s).map(ConfigId::from_uuid))
            .transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(adapter: &str, root: &str) -> Project {
        Project {
            id: ProjectId::new(),
            adapter_id: AdapterId::new(adapter),
            root_path: root.to_string(),
            name: root.rsplit('/').next().unwrap_or(root).to_string(),
            created_at: Utc::now(),
            champion_config_id: None,
        }
    }

    #[tokio::test]
    async fn insert_then_get_by_id_roundtrips() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let p = sample("claude-code", "/tmp/proj-a");
        repo.insert(&p).await.unwrap();
        let back = repo.get_by_id(p.id).await.unwrap().unwrap();
        assert_eq!(back.id, p.id);
        assert_eq!(back.adapter_id.as_str(), "claude-code");
        assert_eq!(back.root_path, "/tmp/proj-a");
    }

    #[tokio::test]
    async fn get_by_id_returns_none_when_absent() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        assert!(repo.get_by_id(ProjectId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_by_root_path_finds_inserted_project() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let p = sample("cursor", "/tmp/proj-b");
        repo.insert(&p).await.unwrap();
        let back = repo.get_by_root_path("/tmp/proj-b").await.unwrap().unwrap();
        assert_eq!(back.id, p.id);
    }

    #[tokio::test]
    async fn list_orders_by_created_at_desc() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let older = Project {
            created_at: Utc::now() - chrono::Duration::hours(1),
            ..sample("aider", "/tmp/older")
        };
        let newer = sample("aider", "/tmp/newer");
        repo.insert(&older).await.unwrap();
        repo.insert(&newer).await.unwrap();
        let rows = repo.list().await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, newer.id);
        assert_eq!(rows[1].id, older.id);
    }

    #[tokio::test]
    async fn root_path_uniqueness_is_enforced() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let a = sample("claude-code", "/tmp/dup");
        let b = sample("cursor", "/tmp/dup");
        repo.insert(&a).await.unwrap();
        let err = repo.insert(&b).await.unwrap_err();
        assert!(
            matches!(err, StorageError::Sqlx(sqlx::Error::Database(_))),
            "expected UNIQUE violation; got {err:?}",
        );
    }

    #[tokio::test]
    async fn delete_removes_the_row() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let p = sample("claude-code", "/tmp/del");
        repo.insert(&p).await.unwrap();
        repo.delete(p.id).await.unwrap();
        assert!(repo.get_by_id(p.id).await.unwrap().is_none());
    }
}
