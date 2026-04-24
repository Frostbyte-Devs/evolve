//! Unified error type for the storage crate.

use thiserror::Error;

/// Errors produced by [`Storage`](crate::Storage) and its repositories.
#[derive(Debug, Error)]
pub enum StorageError {
    /// An underlying sqlx error (connection, query, migration).
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// A JSON serialization error on a payload column.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// A migration error from `sqlx::migrate!`.
    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// Failed to parse a UUID from a TEXT column.
    #[error("uuid: {0}")]
    Uuid(#[from] uuid::Error),
}
