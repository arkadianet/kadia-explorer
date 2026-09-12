//! Fixed-size process-local operator telemetry. No label is derived from request content.
use crate::{Allowlist, AppState};
use axum::{
    body::HttpBody,
    extract::{ConnectInfo, MatchedPath, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    fmt::Write,
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ROUTES: [&str; 31] = [
    "/v1/metrics",
    "/v1/register-capacity",
    "/v1/status",
    "/v1/blocks",
    "/v1/blocks/{height_or_id}",
    "/v1/blocks/{height_or_id}/txs",
    "/v1/tx-summaries",
    "/v1/blocks/{height_or_id}/tx-summaries",
    "/v1/txs",
    "/v1/txs/{id}",
    "/v1/boxes/{id}",
    "/v1/boxes/{id}/rent",
    "/v1/addresses/{addr}",
    "/v1/addresses/{addr}/boxes",
    "/v1/addresses/{addr}/balance/at",
    "/v1/addresses/{addr}/boxes/at",
    "/v1/addresses/{addr}/txs",
    "/v1/addresses/{addr}/rent",
    "/v1/tokens",
    "/v1/tokens/{id}",
    "/v1/tokens/{id}/holders",
    "/v1/tokens/{id}/boxes",
    "/v1/templates/{hash}",
    "/v1/templates/{hash}/boxes",
    "/v1/registers/{reg}/{value}/boxes",
    "/v1/richlist",
    "/v1/supply",
    "/v1/rent/upcoming",
    "/v1/rent/eligible",
    "/v1/search",
    "unmatched",
];

const CLASSES: [&str; 5] = ["1xx", "2xx", "3xx", "4xx", "5xx"];
const BOUNDS: [f64; 8] = [0.001, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 30.0];
#[derive(Clone, Copy)]
pub(crate) enum Event {
    Rejection,
    Timeout,
    Overload,
    Integrity,
}
const EVENTS: [&str; 5] = ["rejection", "timeout", "overload", "integrity", "error"];
#[derive(Debug, Default)]
pub(crate) struct Histogram {
    buckets: [AtomicU64; 8],
    count: AtomicU64,
    micros: AtomicU64,
}
impl Histogram {
    pub(crate) fn observe(&self, elapsed: Duration) {
        for (i, bound) in BOUNDS.iter().enumerate() {
            if elapsed.as_secs_f64() <= *bound {
                self.buckets[i].fetch_add(1, Relaxed);
            }
        }
        self.micros
            .fetch_add(elapsed.as_micros().min(u64::MAX as u128) as u64, Relaxed);
        self.count.fetch_add(1, Relaxed);
    }
    fn emit(&self, out: &mut String, name: &str, labels: &str) {
        for (i, bound) in BOUNDS.iter().enumerate() {
            writeln!(
                out,
                "{name}_bucket{{{labels}le=\"{bound}\"}} {}",
                self.buckets[i].load(Relaxed)
            )
            .unwrap();
        }
        writeln!(
            out,
            "{name}_bucket{{{labels}le=\"+Inf\"}} {}",
            self.count.load(Relaxed)
        )
        .unwrap();
        let labels = labels.trim_end_matches(',');
        writeln!(
            out,
            "{name}_sum{{{labels}}} {}",
            self.micros.load(Relaxed) as f64 / 1_000_000.0
        )
        .unwrap();
        writeln!(out, "{name}_count{{{labels}}} {}", self.count.load(Relaxed)).unwrap();
    }
}
#[derive(Debug, Default)]
struct Route {
    requests: [AtomicU64; 5],
    duration: Histogram,
    bytes: AtomicU64,
    events: [AtomicU64; 5],
}
#[derive(Debug)]
pub(crate) struct Metrics {
    routes: [Route; ROUTES.len()],
    pub(crate) worker: Histogram,
    started: f64,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
            routes: std::array::from_fn(|_| Route::default()),
            worker: Histogram::default(),
            started: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        }
    }
}
pub(crate) async fn observe(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let index = request
        .extensions()
        .get::<MatchedPath>()
        .and_then(|path| ROUTES.iter().position(|route| *route == path.as_str()))
        .unwrap_or(ROUTES.len() - 1);
    let start = Instant::now();
    let response = next.run(request).await;
    let route = &state.counters.metrics.routes[index];
    let class = (usize::from(response.status().as_u16()) / 100).clamp(1, 5) - 1;
    route.requests[class].fetch_add(1, Relaxed);
    route.duration.observe(start.elapsed());
    // All API responses are materialized JSON/text/empty bodies with exact size hints.
    // Counts produced body bytes, excluding headers and transport framing (including HEAD's empty body).
    route
        .bytes
        .fetch_add(response.body().size_hint().exact().unwrap_or(0), Relaxed);
    let event = match response.extensions().get::<Event>() {
        Some(Event::Rejection) => Some(0),
        Some(Event::Timeout) => Some(1),
        Some(Event::Overload) => Some(2),
        Some(Event::Integrity) => Some(3),
        None if response.status() == StatusCode::REQUEST_TIMEOUT => Some(1),
        None if response.status().is_client_error() || response.status().is_server_error() => {
            Some(4)
        }
        None => None,
    };
    if let Some(event) = event {
        route.events[event].fetch_add(1, Relaxed);
    }
    response
}
pub(crate) async fn endpoint(state: AppState, request: Request, allowlist: Allowlist) -> Response {
    // Only the transport peer is authoritative. Forwarding headers never grant access.
    if !request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| allowlist.contains(peer.0.ip()))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let metrics = &state.counters.metrics;
    let mut out = String::new();
    for (name, route) in ROUTES.iter().zip(&metrics.routes) {
        for (class, count) in CLASSES.iter().zip(&route.requests) {
            writeln!(
                out,
                "explorer_requests_total{{route=\"{name}\",status_class=\"{class}\"}} {}",
                count.load(Relaxed)
            )
            .unwrap();
        }
        route.duration.emit(
            &mut out,
            "explorer_request_duration_seconds",
            &format!("route=\"{name}\","),
        );
        writeln!(
            out,
            "explorer_response_bytes_total{{route=\"{name}\"}} {}",
            route.bytes.load(Relaxed)
        )
        .unwrap();
        for (event, count) in EVENTS.iter().zip(&route.events) {
            writeln!(
                out,
                "explorer_request_events_total{{route=\"{name}\",event=\"{event}\"}} {}",
                count.load(Relaxed)
            )
            .unwrap();
        }
    }
    metrics
        .worker
        .emit(&mut out, "explorer_blocking_permit_duration_seconds", "");
    let status = state.status.borrow();
    for (name, value) in [
        ("process_start_time_seconds", metrics.started),
        (
            "active_readers",
            state.counters.inflight_reads.load(Relaxed) as f64,
        ),
        (
            "available_read_permits",
            state.read_permits.available_permits() as f64,
        ),
        (
            "available_history_permits",
            state.counters.history_permits.available_permits() as f64,
        ),
        ("ingest_indexed_height", status.indexed.unwrap_or(0) as f64),
        ("ingest_best_height", status.best as f64),
        (
            "ingest_lag_blocks",
            status.best.saturating_sub(status.indexed.unwrap_or(0)) as f64,
        ),
        ("ingest_halted", u8::from(status.halted.is_some()) as f64),
        ("ingest_stalled", u8::from(status.stalled.is_some()) as f64),
        (
            "ingest_source_error",
            u8::from(status.source_error.is_some()) as f64,
        ),
        (
            "register_entries",
            state.store.cached_register_index_entries() as f64,
        ),
        (
            "register_ceiling",
            state
                .store
                .register_index_ceiling()
                .map_or(f64::INFINITY, |v| v as f64),
        ),
    ] {
        if value == f64::INFINITY {
            writeln!(out, "explorer_{name} +Inf").unwrap();
        } else {
            writeln!(out, "explorer_{name} {value}").unwrap();
        }
    }
    (
        [
            ("content-type", "text/plain; version=0.0.4; charset=utf-8"),
            ("cache-control", "no-store"),
        ],
        out,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{inflight_guard_tests::state, ApiConfig, ApiError};
    use axum::{body::Body, routing::get, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn request(path: &str, peer: Option<&str>) -> Request {
        let mut request = Request::builder().uri(path).body(Body::empty()).unwrap();
        if let Some(peer) = peer {
            request
                .extensions_mut()
                .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        }
        request
    }
    fn config() -> ApiConfig {
        ApiConfig {
            per_second: 0,
            metrics_allowlist: Allowlist::parse(&["192.0.2.1".into()]).unwrap(),
            ..ApiConfig::default()
        }
    }
    async fn scrape(app: Router) -> String {
        let response = app
            .oneshot(request("/v1/metrics", Some("192.0.2.1:1234")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn templates_start_marker_bounded_series_and_no_store_reads() {
        let state = state(2);
        let app = crate::router(state.clone(), &config());
        let id = "ab".repeat(32);
        let response = app
            .clone()
            .oneshot(request(&format!("/v1/blocks/{id}"), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .len();
        let before = state.store.read_transaction_count();
        let text = scrape(app.clone()).await;
        assert_eq!(state.store.read_transaction_count(), before);
        assert!(!text.contains(&id));
        assert!(text.contains(
            "explorer_requests_total{route=\"/v1/blocks/{height_or_id}\",status_class=\"4xx\"} 1"
        ));
        assert!(text.contains(&format!(
            "explorer_response_bytes_total{{route=\"/v1/blocks/{{height_or_id}}\"}} {bytes}"
        )));
        assert!(text.contains(&format!(
            "explorer_process_start_time_seconds {}",
            state.counters.metrics.started
        )));
        assert!(state.counters.metrics.started > 0.0);
        assert_eq!(text.lines().count(), ROUTES.len() * 22 + 11 + 12);
        assert_eq!(ROUTES.len(), 31);
        let second = scrape(app).await;
        let marker = |s: &str| {
            s.lines()
                .find(|l| l.starts_with("explorer_process_start_time_seconds "))
                .unwrap()
                .to_owned()
        };
        assert_eq!(marker(&text), marker(&second));
    }
    #[tokio::test]
    async fn exposure_defaults_denied_and_ignores_forwarding_headers() {
        let state = state(2);
        let default = crate::router(state.clone(), &ApiConfig::default());
        assert_eq!(
            default
                .oneshot(request("/v1/metrics", Some("127.0.0.1:1")))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        let app = crate::router(state, &config());
        for peer in [None, Some("192.0.2.2:1")] {
            let mut request = request("/v1/metrics", peer);
            request
                .headers_mut()
                .insert("x-forwarded-for", "192.0.2.1".parse().unwrap());
            assert_eq!(
                app.clone().oneshot(request).await.unwrap().status(),
                StatusCode::NOT_FOUND
            );
        }
    }
    #[tokio::test]
    async fn real_admission_rejection_and_overload_count_once() {
        let state = state(1);
        let mut cfg = config();
        cfg.per_second = 1;
        cfg.burst = 1;
        let app = crate::router(state.clone(), &cfg);
        assert_eq!(
            app.clone()
                .oneshot(request("/v1/status", None))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(request("/v1/status", None))
                .await
                .unwrap()
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        let permit = state.read_permits.clone().acquire_owned().await.unwrap();
        let app = crate::router(state.clone(), &config());
        assert_eq!(
            app.clone()
                .oneshot(request("/v1/blocks/1", None))
                .await
                .unwrap()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let text = scrape(app).await;
        assert!(text
            .contains("explorer_request_events_total{route=\"/v1/status\",event=\"rejection\"} 1"));
        assert!(text.contains("explorer_request_events_total{route=\"/v1/blocks/{height_or_id}\",event=\"overload\"} 1"));
        assert!(text.contains(
            "explorer_request_events_total{route=\"/v1/blocks/{height_or_id}\",event=\"error\"} 0"
        ));
        drop(permit);
    }
    #[tokio::test]
    async fn timeout_worker_drain_and_errors_are_distinct() {
        let state = state(1);
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let receiver = std::sync::Arc::new(std::sync::Mutex::new(Some(finish_rx)));
        let worker_state = state.clone();
        let app = Router::new()
            .route(
                "/v1/blocks/{height_or_id}",
                get(move || {
                    let state = worker_state.clone();
                    let rx = receiver.lock().unwrap().take().unwrap();
                    async move {
                        crate::blocking(&state, move |_| {
                            rx.recv().unwrap();
                            Ok(())
                        })
                        .await
                    }
                }),
            )
            .route(
                "/v1/txs",
                get(|| async { ApiError::Internal("test".into()) }),
            )
            .route(
                "/v1/boxes/{id}",
                get(|| async { ApiError::Integrity("test".into()) }),
            )
            .layer(tower_http::timeout::TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                Duration::from_millis(30),
            ))
            .layer(axum::middleware::from_fn_with_state(state.clone(), observe));
        assert_eq!(
            app.clone()
                .oneshot(request("/v1/blocks/1", None))
                .await
                .unwrap()
                .status(),
            StatusCode::REQUEST_TIMEOUT
        );
        assert_eq!(state.read_permits.available_permits(), 0);
        assert_eq!(state.counters.metrics.worker.count.load(Relaxed), 0);
        for path in ["/v1/txs", "/v1/boxes/abc"] {
            assert_eq!(
                app.clone()
                    .oneshot(request(path, None))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
        finish_tx.send(()).unwrap();
        for _ in 0..200 {
            if state.counters.inflight_reads.load(Relaxed) == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(state.read_permits.available_permits(), 1);
        assert_eq!(state.counters.metrics.worker.count.load(Relaxed), 1);
        assert!(state.counters.metrics.worker.micros.load(Relaxed) >= 30_000);
        let text = scrape(crate::router(state, &config())).await;
        for (route, event) in [
            ("/v1/blocks/{height_or_id}", "timeout"),
            ("/v1/txs", "error"),
            ("/v1/boxes/{id}", "integrity"),
        ] {
            assert!(text.contains(&format!(
                "explorer_request_events_total{{route=\"{route}\",event=\"{event}\"}} 1"
            )));
        }
    }
}
