//! `#[ignore]`-gated smoke tests that hit real providers.
//!
//! Run with:
//!   cargo test -p evolve-llm --test smoke -- --ignored
//!
//! Requirements:
//! - `smoke_anthropic_round_trip`: `ANTHROPIC_API_KEY` set in environment.
//! - `smoke_ollama_round_trip`: Ollama running locally on 11434 with
//!   `qwen2.5-coder:7b` (or `OLLAMA_MODEL`) pulled.

use evolve_llm::{AnthropicHaikuClient, LlmClient, OllamaClient};

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn smoke_anthropic_round_trip() {
    let client = AnthropicHaikuClient::from_env().expect("ANTHROPIC_API_KEY must be set");
    let result = client
        .complete("Reply with exactly the word: pong", 16)
        .await
        .expect("anthropic call should succeed");
    assert!(!result.text.is_empty(), "expected non-empty assistant text");
    assert!(
        result.text.to_lowercase().contains("pong"),
        "expected 'pong' in response, got: {}",
        result.text,
    );
}

#[tokio::test]
#[ignore = "requires local Ollama with OLLAMA_MODEL pulled"]
async fn smoke_ollama_round_trip() {
    let client = OllamaClient::local();
    let result = client
        .complete("Reply with exactly the word: pong", 16)
        .await
        .expect("ollama call should succeed");
    assert!(!result.text.is_empty(), "expected non-empty assistant text");
}
