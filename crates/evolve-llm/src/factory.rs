//! Runtime client selection.

use crate::anthropic::AnthropicHaikuClient;
use crate::client::LlmClient;
use crate::error::LlmError;
use crate::ollama::OllamaClient;
use std::time::Duration;

const OLLAMA_BASE_URL_ENV: &str = "OLLAMA_BASE_URL";
const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";
const DEFAULT_ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com";
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// Select an LLM client at runtime, reading the environment:
///
/// 1. Probe Ollama at `OLLAMA_BASE_URL` (default `http://localhost:11434`)
///    via `GET /api/version`. If reachable in 500ms, use it.
/// 2. Otherwise, if `ANTHROPIC_API_KEY` is set, use [`AnthropicHaikuClient`].
/// 3. Otherwise, return [`LlmError::NoLlmAvailable`].
pub async fn pick_default_client() -> Result<Box<dyn LlmClient>, LlmError> {
    let ollama_endpoint =
        std::env::var(OLLAMA_BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_OLLAMA_ENDPOINT.to_string());
    let anthropic_key = std::env::var(ANTHROPIC_API_KEY_ENV).ok();
    pick_with(&ollama_endpoint, anthropic_key.as_deref()).await
}

/// Same selection logic as [`pick_default_client`] but with explicit inputs —
/// lets tests exercise the branches without touching process environment.
pub async fn pick_with(
    ollama_endpoint: &str,
    anthropic_key: Option<&str>,
) -> Result<Box<dyn LlmClient>, LlmError> {
    if ollama_reachable(ollama_endpoint).await {
        return Ok(Box::new(OllamaClient::with_endpoint(ollama_endpoint)));
    }
    match anthropic_key {
        Some(key) => Ok(Box::new(AnthropicHaikuClient::with_endpoint(
            key,
            DEFAULT_ANTHROPIC_ENDPOINT,
        ))),
        None => Err(LlmError::NoLlmAvailable),
    }
}

async fn ollama_reachable(endpoint: &str) -> bool {
    let url = format!("{endpoint}/api/version");
    let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn picks_ollama_when_reachable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/version"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"version":"0.1.0"}"#))
            .mount(&server)
            .await;

        let client = pick_with(&server.uri(), None).await.unwrap();
        assert!(
            !client.model_id().contains("claude"),
            "expected Ollama model id, got {}",
            client.model_id(),
        );
    }

    #[tokio::test]
    async fn falls_back_to_anthropic_when_ollama_unreachable_and_key_set() {
        // Point Ollama at a likely-closed port.
        let client = pick_with("http://127.0.0.1:1", Some("test-key"))
            .await
            .unwrap();
        assert_eq!(client.model_id(), "claude-haiku-4-5-20251001");
    }

    #[tokio::test]
    async fn returns_no_llm_available_when_nothing_configured() {
        let result = pick_with("http://127.0.0.1:1", None).await;
        assert!(matches!(result, Err(LlmError::NoLlmAvailable)));
    }
}
