//! The `LlmClient` trait and its shared types.

use crate::error::LlmError;
use async_trait::async_trait;

/// Token usage reported by the provider. Both fields are 0 if unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    /// Input tokens billed.
    pub input: u32,
    /// Output tokens billed.
    pub output: u32,
}

/// One completion response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResult {
    /// Assistant text.
    pub text: String,
    /// Token usage.
    pub usage: TokenUsage,
}

/// Shared interface for LLM clients.
#[async_trait]
pub trait LlmClient: Send + Sync + std::fmt::Debug {
    /// Run a single non-streaming completion.
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<CompletionResult, LlmError>;

    /// Stable identifier used by the cost tracker price table.
    fn model_id(&self) -> &str;
}

/// Placeholder client that always returns `LlmError::NoLlmAvailable`. Used by
/// callers that need a `&dyn LlmClient` value but know they will never actually
/// invoke it (e.g., when running only non-LLM mutators).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpLlmClient;

#[async_trait]
impl LlmClient for NoOpLlmClient {
    async fn complete(
        &self,
        _prompt: &str,
        _max_tokens: u32,
    ) -> Result<CompletionResult, LlmError> {
        Err(LlmError::NoLlmAvailable)
    }

    fn model_id(&self) -> &str {
        "noop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_default_is_zero() {
        let u = TokenUsage::default();
        assert_eq!(u.input, 0);
        assert_eq!(u.output, 0);
    }
}
