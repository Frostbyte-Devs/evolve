//! The [`Adapter`] trait: one implementor per supported tool.

use crate::signals::{ParsedSignal, SessionLog};
use async_trait::async_trait;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::AdapterId;
use std::path::Path;
use thiserror::Error;

/// Errors common to all adapters.
#[derive(Debug, Error)]
pub enum AdapterError {
    /// I/O error reading or writing adapter files.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON error parsing or writing settings/transcript.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Adapter was asked to operate on a root that it does not recognize.
    #[error("adapter not applicable at {path}")]
    NotApplicable {
        /// Path the caller asked the adapter to operate on.
        path: String,
    },
    /// Adapter-specific parse failure (bad transcript line, etc.).
    #[error("parse: {0}")]
    Parse(String),
}

/// What [`Adapter::detect`] found in a target directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterDetection {
    /// Strong signal the tool is in use here (config files, binaries, etc.).
    Detected,
    /// Nothing conclusive found.
    NotDetected,
}

/// Per-tool integration contract. See design doc section 4.4.
#[async_trait]
pub trait Adapter: Send + Sync {
    /// Stable identifier (e.g., "claude-code", "cursor", "aider").
    fn id(&self) -> AdapterId;

    /// Detect whether the tool is in use at `root` (or in the user's home).
    fn detect(&self, root: &Path) -> AdapterDetection;

    /// Install hooks/config so the tool reports sessions back to Evolve. Idempotent.
    async fn install(&self, root: &Path, config: &AgentConfig) -> Result<(), AdapterError>;

    /// Write the given config into the tool's files (inside managed-section markers).
    async fn apply_config(&self, root: &Path, config: &AgentConfig) -> Result<(), AdapterError>;

    /// Parse a session log into fitness signals.
    async fn parse_session(&self, log: SessionLog) -> Result<Vec<ParsedSignal>, AdapterError>;

    /// Remove everything this adapter installed, restoring the pre-evolve state.
    async fn forget(&self, root: &Path) -> Result<(), AdapterError>;
}
