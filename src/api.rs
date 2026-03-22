use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

use crate::cache::DnsCache;
use crate::handler::DnsHandler;
use crate::history::QueryHistory;
use crate::upstream::UpstreamGroup;
use crate::web_ui;

pub struct AppState {
    pub handler: Arc<DnsHandler>,
    pub cache: Arc<DnsCache>,
    pub history: Arc<QueryHistory>,
    pub china_upstream: Arc<UpstreamGroup>,
    pub global_upstream: Arc<UpstreamGroup>,
    pub start_time: std::time::Instant,
    pub config_path: PathBuf,
}

pub async fn run_api_server(addr: &str, state: Arc<AppState>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(ui_handler))
        .route("/stats", get(stats_handler))
        .route("/history", get(history_handler))
        .route("/cache/stats", get(cache_stats_handler))
        .route("/upstreams", get(upstreams_handler))
        .route("/config", get(config_get_handler).put(config_put_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("HTTP API server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

// --- Handlers ---

async fn ui_handler() -> impl IntoResponse {
    Html(web_ui::INDEX_HTML)
}

#[derive(Serialize)]
struct StatsResponse {
    uptime_seconds: u64,
    handler: crate::handler::HandlerStats,
    cache: crate::cache::CacheStatsSnapshot,
}

async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    Json(StatsResponse {
        uptime_seconds: state.start_time.elapsed().as_secs(),
        handler: state.handler.stats(),
        cache: state.cache.stats(),
    })
}

async fn history_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<crate::history::QueryRecord>> {
    Json(state.history.recent(100))
}

async fn cache_stats_handler(
    State(state): State<Arc<AppState>>,
) -> Json<crate::cache::CacheStatsSnapshot> {
    Json(state.cache.stats())
}

#[derive(Serialize)]
struct UpstreamsResponse {
    china: Vec<crate::upstream::UpstreamStats>,
    global: Vec<crate::upstream::UpstreamStats>,
}

async fn upstreams_handler(State(state): State<Arc<AppState>>) -> Json<UpstreamsResponse> {
    Json(UpstreamsResponse {
        china: state.china_upstream.stats(),
        global: state.global_upstream.stats(),
    })
}

async fn config_get_handler(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let content = tokio::fs::read_to_string(&state.config_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to read config: {}", e)})),
            )
        })?;

    // Parse to validate, then return the raw JSON
    let val: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Config parse error: {}", e)})),
        )
    })?;

    Ok(Json(val))
}

async fn config_put_handler(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Validate that the body is valid JSON and a valid Config
    let _config: crate::config::Config = serde_json::from_str(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid config: {}", e)})),
        )
    })?;

    // Pretty-print before saving
    let val: serde_json::Value = serde_json::from_str(&body).unwrap();
    let pretty = serde_json::to_string_pretty(&val).unwrap();

    tokio::fs::write(&state.config_path, pretty.as_bytes())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to write config: {}", e)})),
            )
        })?;

    info!("config saved via web UI to {:?}", state.config_path);
    Ok(Json(serde_json::json!({"status": "ok", "message": "Config saved. Restart required for changes to take effect."})))
}
