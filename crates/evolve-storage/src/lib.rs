//! evolve-storage: SQLite persistence for Evolve.
//!
//! Opens a single database at the configured path (default `~/.evolve/evolve.db`),
//! applies embedded migrations, and exposes repository structs per table.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod agent_configs;
pub mod error;
pub mod experiments;
pub mod pool;
pub mod projects;
pub mod sessions;
pub mod signals;

pub use error::StorageError;
pub use pool::Storage;
