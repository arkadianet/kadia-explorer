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
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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

/// Decrements `Counters.inflight_reads` when dropped. Moved into the `spawn_blocking`
/// closure next to the semaphore permit so the two share identical lifetimes: both are
/// released when the read finishes, whether the handler future that awaited them is still
/// around to see it or not. That matters because `TimeoutLayer` drops the handler future on
/// timeout without waiting for the spawned task — a decrement placed after `.await` in
/// `blocking()` would then never run, leaking the counter upward by one per timeout.
struct InflightGuard(Arc<Counters>);

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.inflight_reads.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Runs `f` against a fresh [`Reader`] on the blocking pool. One reader per request keeps
/// every read in a single request consistent without holding a transaction across `.await`.
///
/// Bounded by `state.read_permits`: when the budget is exhausted this fails fast with
/// [`ApiError::Overloaded`] rather than queuing onto the blocking pool. `inflight_reads` is
/// incremented only once a permit is held, and is decremented by [`InflightGuard`]'s `Drop`
/// alongside the permit — on success, on an error from `f`, on a panic in the blocking task,
/// or on the handler future being dropped out from under it (request timeout/cancellation).
pub(crate) async fn blocking<T, F>(state: &AppState, f: F) -> Result<T, ApiError>
where
    F: FnOnce(&Reader) -> Result<T, ApiError> + Send + 'static,
    T: Send + 'static,
{
    let permit = state
        .read_permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::Overloaded)?;
    let counters = state.counters.clone();
    counters.inflight_reads.fetch_add(1, Ordering::Relaxed);
    let guard = InflightGuard(counters);
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || {
        // Held for the duration of the read; both drop together when it finishes.
        let _permit = permit;
        let _guard = guard;
        let rd = Reader::new(&store)?;
        f(&rd)
    })
    .await;
    result.map_err(|e| ApiError::Internal(format!("blocking task failed: {e}")))?
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

#[cfg(test)]
mod inflight_guard_tests {
    use super::*;

    fn state(max_inflight_reads: u32) -> AppState {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("x.redb")).unwrap();
        // Leaked so the temp dir outlives the test; these are short-lived unit tests, not a
        // long-running process, so the leak is bounded and acceptable.
        std::mem::forget(dir);
        let (_tx, rx) = watch::channel(IngestStatus {
            indexed: None,
            best: 0,
            mode: xp_ingest::Mode::Tip,
            source: "test".into(),
            halted: None,
            stalled: None,
        });
        AppState {
            store: Arc::new(store),
            status: rx,
            counters: Arc::new(Counters::default()),
            read_permits: Arc::new(Semaphore::new(max_inflight_reads as usize)),
        }
    }

    /// Regression test: a `blocking()` call cancelled mid-read (e.g. by `TimeoutLayer`
    /// dropping the handler future) must still release its inflight slot. Before the
    /// `InflightGuard` fix, the counter was decremented sequentially after `.await` inside
    /// `blocking()`, so dropping that future skipped the decrement entirely and leaked the
    /// counter upward by one per cancellation.
    #[tokio::test]
    async fn cancelling_the_handler_future_still_releases_the_inflight_slot() {
        let state = state(2);
        let (unblock_tx, unblock_rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();

        // A read that blocks until told to continue, so it is definitely still in flight
        // when we cancel the handler future awaiting it.
        let read = blocking(&state, move |_rd: &Reader| {
            started_tx.send(()).ok();
            unblock_rx.recv().ok();
            Ok::<(), ApiError>(())
        });

        // Simulate `TimeoutLayer` dropping the handler future: race `read` against a very
        // short timeout and let the timeout win. `read`'s first poll (which happens as soon
        // as `timeout` polls it) runs `blocking()` synchronously up to the point it submits
        // the closure to `spawn_blocking` and suspends awaiting the `JoinHandle` — so the
        // closure is already running independently on the blocking pool by the time the
        // timeout elapses and drops `read` (and everything *it* owns: the permit and the
        // `InflightGuard` are inside the spawned closure, not in `read` itself).
        let timed_out = tokio::time::timeout(std::time::Duration::from_millis(50), read)
            .await
            .is_err();
        assert!(timed_out, "expected the handler future to be cancelled");
        started_rx
            .await
            .expect("blocking task should have started before the timeout fired");

        // The blocking task is still running (parked on `unblock_rx.recv()`); the permit and
        // counter must not yet be released just because the awaiting future was dropped —
        // they live inside the still-running spawn_blocking closure.
        unblock_tx.send(()).unwrap();

        // Give the still-running spawn_blocking task a moment to finish and drop its guard.
        for _ in 0..200 {
            if state.counters.inflight_reads.load(Ordering::Relaxed) == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            state.counters.inflight_reads.load(Ordering::Relaxed),
            0,
            "inflight_reads must return to 0 once the cancelled read actually finishes"
        );
        assert_eq!(state.read_permits.available_permits(), 2);
    }
}
