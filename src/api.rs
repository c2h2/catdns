use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use std::sync::Arc;
use tracing::info;

use crate::cache::DnsCache;
use crate::handler::DnsHandler;
use crate::history::QueryHistory;
use crate::upstream::UpstreamGroup;

pub struct AppState {
    pub handler: Arc<DnsHandler>,
    pub cache: Arc<DnsCache>,
    pub history: Arc<QueryHistory>,
    pub china_upstream: Arc<UpstreamGroup>,
    pub global_upstream: Arc<UpstreamGroup>,
    pub start_time: std::time::Instant,
}

pub async fn run_api_server(addr: &str, state: Arc<AppState>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/stats", get(stats_handler))
        .route("/history", get(history_handler))
        .route("/cache/stats", get(cache_stats_handler))
        .route("/upstreams", get(upstreams_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("HTTP API server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

// --- Handlers ---

async fn index_handler() -> &'static str {
    concat!(
        "catdns API\n",
        "  GET /stats      - server statistics\n",
        "  GET /history    - recent query history\n",
        "  GET /cache/stats - cache statistics\n",
        "  GET /upstreams  - upstream server stats\n"
    )
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
