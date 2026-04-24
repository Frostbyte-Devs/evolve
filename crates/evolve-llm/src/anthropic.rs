//! Anthropic Messages API client, Haiku-only.

use crate::client::{CompletionResult, LlmClient, TokenUsage};
use crate::error::LlmError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Minimal client for Anthropic's Messages API, wired for Haiku.
#[derive(Debug, Clone)]
pub struct AnthropicHaikuClient {
    api_key: String,
    endpoint: String,
    model: String,
    http: reqwest::Client,
}

impl AnthropicHaikuClient {
    /// Build from the `ANTHROPIC_API_KEY` env var, using the production endpoint.
    pub fn from_env() -> Result<Self, LlmError> {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| LlmError::NoApiKey)?;
        Ok(Self::with_endpoint(key, DEFAULT_ENDPOINT))
    }

    /// Construct with a caller-supplied endpoint (used by cassette tests).
    pub fn with_endpoint(api_key: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: endpoint.into(),
            model: DEFAULT_MODEL.to_string(),
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<UsageReport>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct UsageReport {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[async_trait]
impl LlmClient for AnthropicHaikuClient {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<CompletionResult, LlmError> {
        let url = format!("{}/v1/messages", self.endpoint);
        let body = MessagesRequest {
            model: &self.model,
            max_tokens,
            messages: vec![Message {
                role: "user",
                content: prompt,
            }],
        };

        for attempt in 0..=1 {
            let result = self
                .http
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await;

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let raw = resp.text().await?;
                        let parsed: MessagesResponse = serde_json::from_str(&raw)?;
                        let text = parsed
                            .content
                            .into_iter()
                            .filter(|b| b.kind == "text")
                            .filter_map(|b| b.text)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let usage = parsed
                            .usage
                            .map(|u| TokenUsage {
                                input: u.input_tokens,
                                output: u.output_tokens,
                            })
                            .unwrap_or_default();
                        return Ok(CompletionResult { text, usage });
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
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SAMPLE_RESPONSE: &str = r#"{
        "id": "msg_01",
        "type": "message",
        "role": "assistant",
        "model": "claude-haiku-4-5-20251001",
        "content": [{"type":"text","text":"Hello from mock Haiku"}],
        "usage": {"input_tokens": 12, "output_tokens": 5}
    }"#;

    #[tokio::test]
    async fn happy_path_parses_text_and_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RESPONSE))
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let got = client.complete("hi", 16).await.unwrap();
        assert_eq!(got.text, "Hello from mock Haiku");
        assert_eq!(got.usage.input, 12);
        assert_eq!(got.usage.output, 5);
    }

    #[tokio::test]
    async fn retries_once_on_5xx_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RESPONSE))
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let got = client.complete("hi", 16).await.unwrap();
        assert_eq!(got.text, "Hello from mock Haiku");
    }

    #[tokio::test]
    async fn gives_up_after_second_5xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let err = client.complete("hi", 16).await.unwrap_err();
        assert!(matches!(
            err,
            LlmError::UnexpectedStatus { status: 503, .. }
        ));
    }

    #[tokio::test]
    async fn does_not_retry_on_4xx_except_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_string(r#"{"error":"bad"}"#))
            .expect(1)
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let err = client.complete("hi", 16).await.unwrap_err();
        assert!(matches!(
            err,
            LlmError::UnexpectedStatus { status: 400, .. }
        ));
    }
}
