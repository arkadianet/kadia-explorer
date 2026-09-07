//! `xp-api`: the read-only HTTP surface over an [`xp_store::Store`].
//!
//! [`router`] builds the whole `/v1` tree over an [`AppState`]. Every handler opens its own
//! [`Reader`] (a point-in-time snapshot) inside `spawn_blocking`, since redb reads are
//! synchronous and may touch disk.

pub mod dto;
pub mod error;
pub mod handlers;
pub mod limit;

use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::sync::Semaphore;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use xp_ingest::IngestStatus;
use xp_store::{Reader, Store};

pub use error::ApiError;
pub use limit::Allowlist;

/// Runtime knobs for the public API (spec §3–§5). `bin/explorer` builds it from TOML.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub per_second: u32,
    pub burst: u32,
    pub allowlist: Allowlist,
    pub trusted_proxies: Allowlist,
    pub max_inflight_reads: u32,
}

impl Default for ApiConfig {
    fn default() -> ApiConfig {
        ApiConfig {
            per_second: 10,
            burst: 30,
            allowlist: Allowlist::default(),
            trusted_proxies: Allowlist::parse(&["127.0.0.1".into(), "::1".into()]).expect("static"),
            max_inflight_reads: 32,
        }
    }
}

/// Process-lifetime counters surfaced on `/v1/status`.
#[derive(Debug, Default)]
pub struct Counters {
    pub rate_limited_total: AtomicU64,
    pub inflight_reads: AtomicU32,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub status: watch::Receiver<IngestStatus>,
    pub counters: Arc<Counters>,
    /// Bounds concurrent blocking store reads; wired into `blocking()` in Task 4.
    pub read_permits: Arc<Semaphore>,
}

/// Runs `f` against a fresh [`Reader`] on the blocking pool. One reader per request keeps
/// every read in a single request consistent without holding a transaction across `.await`.
pub(crate) async fn blocking<T, F>(state: &AppState, f: F) -> Result<T, ApiError>
where
    F: FnOnce(&Reader) -> Result<T, ApiError> + Send + 'static,
    T: Send + 'static,
{
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || {
        let rd = Reader::new(&store)?;
        f(&rd)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("blocking task failed: {e}")))?
}

pub fn router(state: AppState, cfg: &ApiConfig) -> Router {
    let rate_limit = limit::RateLimit::new(cfg, state.counters.clone());
    Router::new()
        .route("/v1/status", get(handlers::status::status))
        .route("/v1/blocks", get(handlers::blocks::list))
        .route("/v1/blocks/{height_or_id}", get(handlers::blocks::get_one))
        .route(
            "/v1/blocks/{height_or_id}/txs",
            get(handlers::blocks::block_txs),
        )
        .route("/v1/txs", get(handlers::txs::list))
        .route("/v1/txs/{id}", get(handlers::txs::get_one))
        .route("/v1/boxes/{id}", get(handlers::boxes::get_one))
        .route("/v1/boxes/{id}/rent", get(handlers::boxes::rent))
        .route("/v1/addresses/{addr}", get(handlers::addresses::get_one))
        .route(
            "/v1/addresses/{addr}/boxes",
            get(handlers::addresses::boxes),
        )
        .route("/v1/addresses/{addr}/txs", get(handlers::addresses::txs))
        .route("/v1/addresses/{addr}/rent", get(handlers::addresses::rent))
        .route("/v1/tokens", get(handlers::tokens::list))
        .route("/v1/tokens/{id}", get(handlers::tokens::get_one))
        .route("/v1/tokens/{id}/holders", get(handlers::tokens::holders))
        .route("/v1/tokens/{id}/boxes", get(handlers::tokens::boxes))
        .route("/v1/templates/{hash}", get(handlers::templates::get_one))
        .route(
            "/v1/templates/{hash}/boxes",
            get(handlers::templates::boxes),
        )
        .route(
            "/v1/registers/{reg}/{value}/boxes",
            get(handlers::registers::boxes),
        )
        .route("/v1/richlist", get(handlers::richlist::list))
        .route("/v1/rent/upcoming", get(handlers::rent::upcoming))
        .route("/v1/rent/eligible", get(handlers::rent::eligible))
        .route("/v1/search", get(handlers::search::search))
        // 5 s ceiling per request: every handler is a bounded store read, so anything
        // slower is a stuck disk rather than a legitimately long query.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(5),
        ))
        // Outside the timeout, so a rejected request never enters the timeout budget…
        .layer(rate_limit)
        // …and inside CORS, so a browser can read the 429 body.
        .layer(CorsLayer::permissive())
        .with_state(state)
}
