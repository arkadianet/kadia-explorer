use axum::{
    body::Body,
    http::{Request, StatusCode},
    Extension, Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{watch, Notify, Semaphore};
use tower::ServiceExt;
use xp_api::{ApiConfig, AppState, Counters};
use xp_ingest::{IngestStatus, Mode};
use xp_source::{BlockSource, Fallback, SourceError};
use xp_store::Store;
use xp_types::Hash32;

fn tx(n: u32) -> Value {
    let mut id = [0x47; 32];
    id[..4].copy_from_slice(&n.to_be_bytes());
    json!({ "id":hex::encode(id), "inputs":[{"boxId":hex::encode([0x11;32])}], "dataInputs":[],
        "outputs":[{"value":9007199254740993u64,"ergoTree":xp_store::FEE_TREE_HEX},{"value":7,"ergoTree":xp_store::FEE_TREE_HEX}], "size":123 })
}
struct Source {
    mode: AtomicUsize,
    calls: AtomicUsize,
    body: Mutex<String>,
    entered: Notify,
}
impl Source {
    fn new(body: Value) -> Arc<Self> {
        Arc::new(Self {
            mode: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            body: Mutex::new(body.to_string()),
            entered: Notify::new(),
        })
    }
}
#[async_trait::async_trait]
impl BlockSource for Source {
    fn name(&self) -> &str {
        "private-node-url-not-exposed"
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Ok(0)
    }
    async fn header_id_at(&self, _: u32) -> Result<Option<Hash32>, SourceError> {
        Ok(None)
    }
    async fn full_block_json(&self, _: &Hash32) -> Result<Option<String>, SourceError> {
        Ok(None)
    }
    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        Ok("[]".into())
    }
    async fn unconfirmed_transactions_json(
        &self,
        limit: u16,
        max: usize,
    ) -> Result<String, SourceError> {
        assert_eq!(limit, 100);
        assert_eq!(max, 2 * 1024 * 1024);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        match self.mode.load(Ordering::SeqCst) {
            1 => Err(SourceError::Unavailable),
            2 => Err(SourceError::Capability("private detail".into())),
            3 => std::future::pending().await,
            _ => Ok(self.body.lock().unwrap().clone()),
        }
    }
}
fn app(source: Option<Arc<dyn BlockSource>>) -> (tempfile::TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("pool.redb")).unwrap());
    let (_, status) = watch::channel(IngestStatus {
        source_observed_at_ms: None,
        source_error: None,
        indexed: None,
        best: 0,
        mode: Mode::Tip,
        source: "test".into(),
        halted: None,
        stalled: None,
    });
    let app = xp_api::router(
        AppState {
            store,
            status,
            counters: Arc::new(Counters::default()),
            read_permits: Arc::new(Semaphore::new(32)),
        },
        &ApiConfig {
            per_second: 0,
            ..Default::default()
        },
    );
    (
        dir,
        if let Some(source) = source {
            app.layer(Extension(source))
        } else {
            app
        },
    )
}
async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    if status == StatusCode::SERVICE_UNAVAILABLE {
        assert_eq!(response.headers()["retry-after"], "1");
    }
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}
#[tokio::test]
async fn exact_summaries_are_node_local_and_cached_without_extra_requests() {
    let source = Source::new(json!([tx(1)]));
    let (_dir, app) = app(Some(source.clone()));
    assert_eq!(source.calls.load(Ordering::SeqCst), 0);
    let (status, body) = get(&app, "/v1/mempool").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scope"], "configured_node_mempool");
    assert_eq!(body["source"], "configured_primary_node");
    assert_eq!(body["items"][0]["fee"], "9007199254741000");
    assert_eq!(body["items"][0]["size"], 123);
    assert_eq!(body["items"][0]["input_count"], 1);
    assert_eq!(body["items"][0]["output_count"], 2);
    assert_eq!(body["observed_count"], 1);
    assert_eq!(body["limit_reached"], false);
    assert_eq!(body["cached"], false);
    assert_eq!(
        body["expires_at_ms"].as_u64().unwrap() - body["checked_at_ms"].as_u64().unwrap(),
        5000
    );
    let (_, cached) = get(&app, "/v1/mempool").await;
    assert_eq!(cached["cached"], true);
    assert_eq!(cached["checked_at_ms"], body["checked_at_ms"]);
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert!(body.get("total_count").is_none());
}
#[tokio::test]
async fn empty_and_limit_reached_are_successful_observations_with_distinct_meanings() {
    let (_dir, empty) = app(Some(Source::new(json!([]))));
    let (status, body) = get(&empty, "/v1/mempool").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"], json!([]));
    assert_eq!(body["limit_reached"], false);
    let (_dir, full) = app(Some(Source::new(json!((0..100)
        .map(tx)
        .collect::<Vec<_>>()))));
    let (status, body) = get(&full, "/v1/mempool").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 100);
    assert_eq!(body["observed_count"], 100);
    assert_eq!(body["limit_reached"], true);
}
#[tokio::test]
async fn unavailable_unsupported_and_not_configured_are_never_empty_success() {
    let (_dir, no_source) = app(None);
    let (status, body) = get(&no_source, "/v1/mempool").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "mempool_not_configured");
    for (mode, code) in [(1, "mempool_unavailable"), (2, "mempool_unsupported")] {
        let source = Source::new(json!([]));
        source.mode.store(mode, Ordering::SeqCst);
        let (_dir, app) = app(Some(source.clone()));
        let (status, body) = get(&app, "/v1/mempool").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], code);
        assert!(body.get("items").is_none());
        get(&app, "/v1/mempool").await;
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn stale_success_is_not_served_after_a_failed_refresh_and_errors_expire() {
    let source = Source::new(json!([tx(1)]));
    let (_dir, app) = app(Some(source.clone()));
    get(&app, "/v1/mempool").await;
    source.mode.store(1, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(5100)).await;
    let (status, body) = get(&app, "/v1/mempool").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.get("items").is_none());
    source.mode.store(0, Ordering::SeqCst);
    assert_eq!(
        get(&app, "/v1/mempool").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert_eq!(get(&app, "/v1/mempool").await.0, StatusCode::OK);
    assert_eq!(source.calls.load(Ordering::SeqCst), 3);
}
#[tokio::test]
async fn bounded_concurrency_and_cancellation_release_admission() {
    let source = Source::new(json!([]));
    source.mode.store(3, Ordering::SeqCst);
    let (_dir, app) = app(Some(source.clone()));
    let first_app = app.clone();
    let pending = tokio::spawn(async move { get(&first_app, "/v1/mempool").await });
    tokio::time::timeout(Duration::from_secs(1), source.entered.notified())
        .await
        .unwrap();
    let (status, body) = get(&app, "/v1/mempool").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "mempool_busy");
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    pending.abort();
    let _ = pending.await;
    source.mode.store(0, Ordering::SeqCst);
    assert_eq!(get(&app, "/v1/mempool").await.0, StatusCode::OK);
}
#[tokio::test]
async fn node_timeout_and_primary_only_fallback_fail_closed() {
    let source = Source::new(json!([]));
    source.mode.store(3, Ordering::SeqCst);
    let (_dir, slow) = app(Some(source));
    let result = tokio::time::timeout(Duration::from_secs(3), get(&slow, "/v1/mempool"))
        .await
        .unwrap();
    assert_eq!(result.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(result.1["code"], "mempool_unavailable");
    let primary = Source::new(json!([]));
    primary.mode.store(1, Ordering::SeqCst);
    // A public fallback that could return [] must not hide the primary outage.
    let secondary = xp_source::RustNode::new("http://127.0.0.1:1");
    let fallback = Fallback::new(primary.clone(), secondary);
    let (_dir, app) = app(Some(Arc::new(fallback)));
    assert_eq!(
        get(&app, "/v1/mempool").await.1["code"],
        "mempool_unavailable"
    );
    assert_eq!(primary.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn malformed_oversized_duplicate_and_unbounded_queries_are_rejected() {
    let mut duplicate = tx(1);
    duplicate["inputs"] = json!([{"boxId":hex::encode([1;32])},{"boxId":hex::encode([1;32])}]);
    let mut bad_tree = tx(2);
    bad_tree["outputs"][0]["ergoTree"] = json!("xyz");
    let mut huge = tx(3);
    huge["outputs"][0]["value"] = json!(u64::MAX);
    for body in [
        json!({"items":[]}),
        json!([tx(1), tx(1)]),
        json!([duplicate]),
        json!([bad_tree]),
        json!([huge]),
        json!((0..101).map(tx).collect::<Vec<_>>()),
    ] {
        let (_dir, app) = app(Some(Source::new(body)));
        let (status, body) = get(&app, "/v1/mempool").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "mempool_invalid_response");
        assert!(body.get("items").is_none());
    }
    let source = Source::new(json!([]));
    *source.body.lock().unwrap() = " ".repeat(2 * 1024 * 1024 + 1);
    let (_dir, app) = app(Some(source));
    assert_eq!(
        get(&app, "/v1/mempool").await.1["code"],
        "mempool_invalid_response"
    );
    assert_eq!(
        get(&app, "/v1/mempool?limit=1000000").await.0,
        StatusCode::BAD_REQUEST
    );
}
