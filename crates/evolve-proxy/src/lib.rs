//! OpenAI-compat HTTP proxy.
//!
//! Started via `evolve proxy --for cursor`. Sits between Cursor (or similar)
//! and the real upstream provider. On each request:
//!   1. Inject the active config's `system_prompt_prefix` into the `messages`.
//!   2. Forward to upstream.
//!   3. Emit a signal candidate describing the interaction.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Proxy configuration.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Upstream base URL, e.g. `https://api.openai.com`.
    pub upstream: String,
    /// Auth token to forward to upstream (Bearer token).
    pub upstream_token: Option<String>,
    /// System prompt prefix to inject into every request.
    pub prefix: String,
}

/// Handle for emitting signal events from the proxy.
pub type SignalSink = Arc<Mutex<Vec<Value>>>;

/// Application state shared by all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Current config (swap-able at runtime in future).
    pub config: ProxyConfig,
    /// Sink where proxy events land — the CLI later flushes these to storage.
    pub signals: SignalSink,
    /// Reqwest client for upstream forwarding.
    pub http: reqwest::Client,
}

/// Build the axum router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .with_state(state)
}

/// Bind a listener and serve until the OS signals shutdown.
pub async fn serve(addr: SocketAddr, state: AppState) -> Result<(), std::io::Error> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(?addr, "evolve-proxy listening");
    axum::serve(listener, app).await
}

async fn chat_completions(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<Response, ProxyHandlerError> {
    // Inject the system prefix as a prepended system message.
    if let Some(messages) = body.get_mut("messages").and_then(|m| m.as_array_mut()) {
        messages.insert(
            0,
            serde_json::json!({
                "role": "system",
                "content": state.config.prefix,
            }),
        );
    } else {
        body["messages"] = serde_json::json!([
            {"role": "system", "content": state.config.prefix}
        ]);
    }

    let url = format!("{}/v1/chat/completions", state.config.upstream);
    let mut req = state.http.post(&url).json(&body);
    if let Some(token) = &state.config.upstream_token {
        req = req.bearer_auth(token);
    }
    let upstream_resp = req
        .send()
        .await
        .map_err(|e| ProxyHandlerError::Upstream(format!("forward failed: {e}")))?;
    let status = upstream_resp.status();
    let upstream_body_bytes = upstream_resp
        .bytes()
        .await
        .map_err(|e| ProxyHandlerError::Upstream(format!("upstream body read failed: {e}")))?;

    // Emit a signal candidate. (CLI flushes these.)
    {
        let mut sink = state.signals.lock().await;
        sink.push(serde_json::json!({
            "event": "proxy_request_forwarded",
            "status": status.as_u16(),
            "prefix_injected": true,
        }));
    }

    let mut response = Response::new(axum::body::Body::from(upstream_body_bytes));
    *response.status_mut() = axum::http::StatusCode::from_u16(status.as_u16())
        .unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
    response
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    Ok(response)
}

/// Internal handler error.
#[derive(Debug)]
pub enum ProxyHandlerError {
    /// Upstream-side failure.
    Upstream(String),
}

impl IntoResponse for ProxyHandlerError {
    fn into_response(self) -> Response {
        match self {
            ProxyHandlerError::Upstream(msg) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": msg})),
            )
                .into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn fresh_sink() -> SignalSink {
        Arc::new(Mutex::new(Vec::new()))
    }

    async fn setup(upstream_uri: String, prefix: &str) -> (Router, SignalSink) {
        let sink = fresh_sink();
        let state = AppState {
            config: ProxyConfig {
                upstream: upstream_uri,
                upstream_token: Some("test-token".into()),
                prefix: prefix.to_string(),
            },
            signals: sink.clone(),
            http: reqwest::Client::new(),
        };
        (router(state), sink)
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let (app, _) = setup("http://localhost:0".into(), "").await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn forwards_and_injects_system_prefix() {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"choices":[{"message":{"content":"pong"}}]}"#),
            )
            .mount(&upstream)
            .await;

        let (app, sink) = setup(upstream.uri(), "INJECTED PREFIX").await;
        let body = serde_json::json!({
            "model": "gpt-4",
            "messages": [{"role":"user","content":"ping"}],
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Check the proxy recorded a forwarded event.
        let recorded = sink.lock().await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0]["event"], "proxy_request_forwarded");
        assert_eq!(recorded[0]["status"], 200);

        // Sanity: body contains upstream's output
        let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body_bytes);
        assert!(text.contains("pong"));
    }

    #[tokio::test]
    async fn returns_502_on_upstream_failure() {
        // Use a known-closed port for upstream.
        let (app, _) = setup("http://127.0.0.1:1".into(), "x").await;
        let body = serde_json::json!({"messages": [{"role":"user","content":"x"}]});
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
