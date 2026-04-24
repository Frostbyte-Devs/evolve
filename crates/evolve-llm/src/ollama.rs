//! Ollama native `/api/chat` client. No auth.

use crate::client::{CompletionResult, LlmClient, TokenUsage};
use crate::error::LlmError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
const DEFAULT_MODEL_ENV: &str = "OLLAMA_MODEL";
const FALLBACK_MODEL: &str = "qwen2.5-coder:7b";
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Minimal client for Ollama's native chat endpoint.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    endpoint: String,
    model: String,
    http: reqwest::Client,
}

impl OllamaClient {
    /// Construct with the default local endpoint and `OLLAMA_MODEL` (or the
    /// fallback) as the model tag.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        let model = std::env::var(DEFAULT_MODEL_ENV).unwrap_or_else(|_| FALLBACK_MODEL.to_string());
        Self {
            endpoint: endpoint.into(),
            model,
            http: reqwest::Client::new(),
        }
    }

    /// Construct with explicit endpoint + model (used by tests).
    pub fn with_endpoint_and_model(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Default constructor: `http://localhost:11434`.
    pub fn local() -> Self {
        Self::with_endpoint(DEFAULT_ENDPOINT)
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    stream: bool,
    options: Options,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct Options {
    num_predict: u32,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: AssistantMessage,
    #[serde(default)]
    prompt_eval_count: u32,
    #[serde(default)]
    eval_count: u32,
}

#[derive(Deserialize)]
struct AssistantMessage {
    content: String,
}

#[async_trait]
impl LlmClient for OllamaClient {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<CompletionResult, LlmError> {
        let url = format!("{}/api/chat", self.endpoint);
        let body = ChatRequest {
            model: &self.model,
            messages: vec![Message {
                role: "user",
                content: prompt,
            }],
            stream: false,
            options: Options {
                num_predict: max_tokens,
            },
        };

        for attempt in 0..=1 {
            let result = self.http.post(&url).json(&body).send().await;

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let raw = resp.text().await?;
                        let parsed: ChatResponse = serde_json::from_str(&raw)?;
                        return Ok(CompletionResult {
                            text: parsed.message.content,
                            usage: TokenUsage {
                                input: parsed.prompt_eval_count,
                                output: parsed.eval_count,
                            },
                        });
                    }
                    let retryable = status.as_u16() == 429 || status.is_server_error();
                    if retryable && attempt == 0 {
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    let body = resp.text().await.unwrap_or_default();
                    let snippet = if body.len() > 512 {
                        &body[..512]
                    } else {
                        &body
                    };
                    return Err(LlmError::UnexpectedStatus {
                        status: status.as_u16(),
                        body: snippet.to_string(),
                    });
                }
                Err(e) if attempt == 0 && e.is_connect() => {
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                Err(e) => return Err(LlmError::Http(e)),
            }
        }
        unreachable!("retry loop exits via return")
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SAMPLE_RESPONSE: &str = r#"{
        "model": "qwen2.5-coder:7b",
        "created_at": "2026-04-23T10:00:00Z",
        "message": {"role": "assistant", "content": "Hello from mock Ollama"},
        "done": true,
        "prompt_eval_count": 10,
        "eval_count": 4
    }"#;

    #[tokio::test]
    async fn happy_path_parses_text_and_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RESPONSE))
            .mount(&server)
            .await;

        let client = OllamaClient::with_endpoint_and_model(server.uri(), "qwen2.5-coder:7b");
        let got = client.complete("hi", 16).await.unwrap();
        assert_eq!(got.text, "Hello from mock Ollama");
        assert_eq!(got.usage.input, 10);
        assert_eq!(got.usage.output, 4);
    }

    #[tokio::test]
    async fn retries_once_on_5xx_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(502))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RESPONSE))
            .mount(&server)
            .await;

        let client = OllamaClient::with_endpoint_and_model(server.uri(), "qwen2.5-coder:7b");
        let got = client.complete("hi", 16).await.unwrap();
        assert_eq!(got.text, "Hello from mock Ollama");
    }

    #[tokio::test]
    async fn gives_up_after_second_5xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = OllamaClient::with_endpoint_and_model(server.uri(), "qwen2.5-coder:7b");
        let err = client.complete("hi", 16).await.unwrap_err();
        assert!(matches!(
            err,
            LlmError::UnexpectedStatus { status: 500, .. }
        ));
    }

    #[tokio::test]
    async fn does_not_retry_on_4xx_except_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(404).set_body_string(r#"{"error":"model missing"}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = OllamaClient::with_endpoint_and_model(server.uri(), "nonexistent");
        let err = client.complete("hi", 16).await.unwrap_err();
        assert!(matches!(
            err,
            LlmError::UnexpectedStatus { status: 404, .. }
        ));
    }
}
