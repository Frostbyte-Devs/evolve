//! Unified error type for the LLM client crate.

use thiserror::Error;

/// Errors produced by LLM clients.
#[derive(Debug, Error)]
pub enum LlmError {
    /// Environment variable `ANTHROPIC_API_KEY` was not set when an Anthropic
    /// client was requested.
    #[error("ANTHROPIC_API_KEY not set")]
    NoApiKey,
    /// Transport or TLS failure.
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    /// Server returned a non-2xx status after retries were exhausted.
    #[error("unexpected status {status}: {body}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
        /// Body snippet (truncated to 512 chars).
        body: String,
    },
    /// Response body did not match the expected schema.
    #[error("parse: {0}")]
    ParseFailure(#[from] serde_json::Error),
    /// Neither Ollama nor Anthropic was reachable / configured.
    #[error("no llm available")]
    NoLlmAvailable,
}
