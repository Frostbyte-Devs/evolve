//! Local-only web dashboard for Evolve.
//!
//! Serves a tiny static SPA (bundled via `rust-embed`) plus a REST API over
//! `evolve-storage`. Bound to 127.0.0.1 by default.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use evolve_storage::Storage;
use evolve_storage::experiments::ExperimentRepo;
use evolve_storage::projects::ProjectRepo;
use evolve_storage::sessions::SessionRepo;
use rust_embed::RustEmbed;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

/// Shared state: a storage handle.
#[derive(Clone)]
pub struct AppState {
    /// Storage handle.
    pub storage: Arc<Storage>,
}

/// Build the axum router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(static_handler))
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{id}", get(get_project))
        .route("/api/projects/{id}/sessions", get(project_sessions))
        .route(
            "/api/projects/{id}/experiment",
            get(project_running_experiment),
        )
        .route(
            "/api/projects/{id}/promotion-log",
            get(project_promotion_log),
        )
        .route("/api/projects/{id}/success-rate", get(project_success_rate))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(static_handler)
        .with_state(state)
}

/// Bind + serve.
pub async fn serve(addr: SocketAddr, state: AppState) -> Result<(), std::io::Error> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(?addr, "evolve-dashboard listening");
    axum::serve(listener, app).await
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let target = if path.is_empty() { "index.html" } else { path };
    match Assets::get(target) {
        Some(content) => {
            let mime = mime_guess::from_path(target).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => {
            // Fallback to index.html (SPA router).
            match Assets::get("index.html") {
                Some(content) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html".to_string())],
                    content.data.into_owned(),
                )
                    .into_response(),
                None => (StatusCode::NOT_FOUND, "not found").into_response(),
            }
        }
    }
}

async fn list_projects(State(state): State<AppState>) -> Response {
    match ProjectRepo::new(&state.storage).list().await {
        Ok(rows) => {
            let payload: Vec<_> = rows
                .into_iter()
                .map(|p| {
                    json!({
                        "id": p.id.to_string(),
                        "adapter_id": p.adapter_id.as_str(),
                        "root_path": p.root_path,
                        "name": p.name,
                        "created_at": p.created_at,
                        "champion_config_id": p.champion_config_id.map(|c| c.to_string()),
                    })
                })
                .collect();
            Json(payload).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_project(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> Response {
    let pid = match uuid::Uuid::parse_str(&id).map(evolve_core::ids::ProjectId::from_uuid) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    match ProjectRepo::new(&state.storage).get_by_id(pid).await {
        Ok(Some(p)) => Json(json!({
            "id": p.id.to_string(),
            "adapter_id": p.adapter_id.as_str(),
            "root_path": p.root_path,
            "name": p.name,
            "created_at": p.created_at,
            "champion_config_id": p.champion_config_id.map(|c| c.to_string()),
        }))
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn project_running_experiment(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let pid = match uuid::Uuid::parse_str(&id).map(evolve_core::ids::ProjectId::from_uuid) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    match ExperimentRepo::new(&state.storage)
        .get_running_for_project(pid)
        .await
    {
        Ok(Some(exp)) => Json(json!({
            "id": exp.id.to_string(),
            "champion_config_id": exp.champion_config_id.to_string(),
            "challenger_config_id": exp.challenger_config_id.to_string(),
            "traffic_share": exp.traffic_share,
            "started_at": exp.started_at,
        }))
        .into_response(),
        Ok(None) => Json(json!(null)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn project_promotion_log(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let pid = match uuid::Uuid::parse_str(&id).map(evolve_core::ids::ProjectId::from_uuid) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    match ExperimentRepo::new(&state.storage)
        .list_completed(pid)
        .await
    {
        Ok(rows) => {
            let payload: Vec<_> = rows
                .into_iter()
                .map(|exp| {
                    json!({
                        "id": exp.id.to_string(),
                        "champion_config_id": exp.champion_config_id.to_string(),
                        "challenger_config_id": exp.challenger_config_id.to_string(),
                        "status": format!("{:?}", exp.status),
                        "traffic_share": exp.traffic_share,
                        "started_at": exp.started_at,
                        "decided_at": exp.decided_at,
                        "decision_posterior": exp.decision_posterior,
                    })
                })
                .collect();
            Json(payload).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Per-project success-rate-over-time metric. Returns one bucket per
/// (date, variant) with the mean session aggregate score for that bucket.
/// This is the killer metric: "is the new champion actually better than the
/// old one?" — answered with a line chart.
async fn project_success_rate(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    use evolve_core::promotion::{
        AggregationConfig, SignalInput, SignalKind as PromSignalKind, aggregate,
    };
    use evolve_storage::signals::{SignalKind as StorageSignalKind, SignalRepo};
    use std::collections::BTreeMap;

    let pid = match uuid::Uuid::parse_str(&id).map(evolve_core::ids::ProjectId::from_uuid) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let sessions = match SessionRepo::new(&state.storage)
        .list_recent(pid, 5_000)
        .await
    {
        Ok(rows) => rows,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let cfg = AggregationConfig::default();
    let signal_repo = SignalRepo::new(&state.storage);

    // (date_string, variant) -> Vec<aggregated_score>
    let mut buckets: BTreeMap<(String, String), Vec<f64>> = BTreeMap::new();
    for sess in sessions {
        let date = sess.started_at.format("%Y-%m-%d").to_string();
        let variant = format!("{:?}", sess.variant).to_lowercase();
        let sigs = match signal_repo.list_for_session(sess.id).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        let inputs: Vec<SignalInput> = sigs
            .into_iter()
            .map(|s| SignalInput {
                kind: match s.kind {
                    StorageSignalKind::Explicit => PromSignalKind::Explicit,
                    StorageSignalKind::Implicit => PromSignalKind::Implicit,
                },
                value: s.value,
            })
            .collect();
        let score = aggregate(&inputs, &cfg);
        buckets.entry((date, variant)).or_default().push(score);
    }

    let series: Vec<_> = buckets
        .into_iter()
        .map(|((date, variant), scores)| {
            let n = scores.len();
            let mean = scores.iter().sum::<f64>() / (n as f64).max(1.0);
            json!({
                "date": date,
                "variant": variant,
                "session_count": n,
                "mean_score": mean,
            })
        })
        .collect();
    Json(series).into_response()
}

async fn project_sessions(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let pid = match uuid::Uuid::parse_str(&id).map(evolve_core::ids::ProjectId::from_uuid) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    match SessionRepo::new(&state.storage).list_recent(pid, 50).await {
        Ok(rows) => {
            let payload: Vec<_> = rows
                .into_iter()
                .map(|s| {
                    json!({
                        "id": s.id.to_string(),
                        "started_at": s.started_at,
                        "ended_at": s.ended_at,
                        "variant": format!("{:?}", s.variant),
                    })
                })
                .collect();
            Json(payload).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn app_with_empty_storage() -> Router {
        let storage = Arc::new(Storage::in_memory_for_tests().await.unwrap());
        router(AppState { storage })
    }

    #[tokio::test]
    async fn serves_index_html_at_root() {
        let app = app_with_empty_storage().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("Evolve"));
    }

    #[tokio::test]
    async fn api_projects_returns_empty_list_when_no_data() {
        let app = app_with_empty_storage().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/projects")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert_eq!(text.trim(), "[]");
    }

    #[tokio::test]
    async fn healthz_ok() {
        let app = app_with_empty_storage().await;
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
}
