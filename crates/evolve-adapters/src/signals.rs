//! Signal types emitted by adapters' `parse_session`.

use std::path::PathBuf;

/// Whether a signal came from an explicit user grade or was inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// User explicitly graded the session (`evolve good/bad/thumbs`).
    Explicit,
    /// Inferred by the adapter from the session log.
    Implicit,
}

/// One parsed signal ready to be persisted. The CLI layer attaches session_id
/// and recorded_at before inserting into `evolve-storage`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSignal {
    /// Explicit vs implicit source.
    pub kind: SignalKind,
    /// Short tag for the signal source (e.g., "tests_passed", "user_clear").
    pub source: String,
    /// Normalized score in `[0.0, 1.0]`.
    pub value: f64,
    /// Optional JSON payload. MUST NOT contain source code.
    pub payload_json: Option<String>,
}

/// Session log material handed to an adapter's `parse_session`.
#[derive(Debug, Clone)]
pub enum SessionLog {
    /// A path to a session transcript file (adapter decides how to read it).
    Transcript(PathBuf),
    /// A git commit SHA (used by Aider post-commit hooks).
    GitCommit(String),
    /// A pre-parsed event payload (used by the proxy's Cursor fallback).
    ProxyEvent(serde_json::Value),
}
