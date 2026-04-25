//! evolve-llm: minimal LLM client for occasional challenger generation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod anthropic;
pub mod client;
pub mod cost;
pub mod error;
pub mod factory;
pub mod ollama;

pub use anthropic::AnthropicHaikuClient;
pub use client::{CompletionResult, LlmClient, NoOpLlmClient, TokenUsage};
pub use cost::{CostTracker, Price};
pub use error::LlmError;
pub use factory::pick_default_client;
pub use ollama::OllamaClient;
