//! Connection pool + migration runner.

use crate::error::StorageError;

/// Handle to the SQLite database. Cheap to clone (wraps a `SqlitePool`).
#[derive(Debug, Clone)]
pub struct Storage {
    pool: sqlx::SqlitePool,
}

impl Storage {
    /// Borrow the underlying pool. Repositories take this by reference.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// Placeholder so the crate compiles before Task 2.3 wires this up.
    #[allow(dead_code)]
    pub(crate) fn from_pool(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    /// Silence unused-error warnings until Task 2.3 fills this in.
    #[allow(dead_code)]
    fn _unused_error_shape(_e: StorageError) {}
}
