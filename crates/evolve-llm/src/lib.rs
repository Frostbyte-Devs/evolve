//! evolve-llm: minimal LLM client for occasional challenger generation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod anthropic;
pub mod client;
pub mod error;

pub use anthropic::AnthropicHaikuClient;
pub use client::{CompletionResult, LlmClient, TokenUsage};
pub use error::LlmError;
