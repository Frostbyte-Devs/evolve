//! Connection pool + migration runner.

use crate::error::StorageError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Handle to the SQLite database. Cheap to clone (wraps a `SqlitePool`).
#[derive(Debug, Clone)]
pub struct Storage {
    pool: sqlx::SqlitePool,
}

impl Storage {
    /// Open the database at `path`, creating it if missing. Applies all
    /// embedded migrations before returning.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// Open a fresh in-memory database with all migrations applied.
    /// Useful only in tests.
    pub async fn in_memory_for_tests() -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // single connection for in-memory (per-conn DBs)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// Borrow the underlying pool. Repositories take this by reference.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_storage_applies_migrations() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let exists: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='projects'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap();
        assert_eq!(exists.0, 1);
    }

    #[tokio::test]
    async fn foreign_keys_pragma_is_enabled() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let fk: (i64,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        assert_eq!(fk.0, 1, "foreign_keys pragma must be ON");
    }

    #[tokio::test]
    async fn all_five_tables_exist_after_migration() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let names: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(storage.pool())
                .await
                .unwrap();
        let got: Vec<&str> = names.iter().map(|(n,)| n.as_str()).collect();
        for expected in [
            "agent_configs",
            "experiments",
            "projects",
            "sessions",
            "signals",
        ] {
            assert!(
                got.contains(&expected),
                "missing table {expected}; got {got:?}",
            );
        }
    }
}
