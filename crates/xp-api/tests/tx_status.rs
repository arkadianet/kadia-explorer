use axum::{
    body::Body,
    http::{Request, StatusCode},
    Extension, Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::sync::{watch, Semaphore};
use tower::ServiceExt;
use xp_api::{ApiConfig, AppState, Counters};
use xp_ingest::{IngestStatus, Mode};
use xp_source::{BlockSource, SourceError};
use xp_store::Store;
use xp_types::{hex32, Hash32};

fn raw() -> &'static str {
    include_str!("../../../tests/fixtures/blocks/1866000.json")
}
fn block() -> xp_wire::DecodedBlock {
    xp_wire::decode_block(raw()).unwrap()
}
fn pending() -> String {
    let block: Value = serde_json::from_str(raw()).unwrap();
    block["blockTransactions"]["transactions"][1].to_string()
}
struct Source {
    mode: AtomicUsize,
    calls: AtomicUsize,
    raw: String,
    confirm_on_lookup: Mutex<Option<Arc<Store>>>,
}
impl Source {
    fn new(raw: String) -> Arc<Self> {
        Arc::new(Self {
            mode: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            raw,
            confirm_on_lookup: Mutex::new(None),
        })
    }
}
#[async_trait::async_trait]
impl BlockSource for Source {
    fn name(&self) -> &str {
        "fixture-node"
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Ok(1866000)
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
    async fn unconfirmed_transaction_json(
        &self,
        _: &Hash32,
    ) -> Result<Option<String>, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(store) = self.confirm_on_lookup.lock().unwrap().take() {
            store.apply_batch(&[block()], true).unwrap();
        }
        match self.mode.load(Ordering::SeqCst) {
            0 => Ok(Some(self.raw.clone())),
            1 => Ok(None),
            2 => Err(SourceError::Unavailable),
            _ => Err(SourceError::Capability("no mempool endpoint".into())),
        }
    }
}
fn app(source: Option<Arc<Source>>) -> (tempfile::TempDir, Arc<Store>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("status.redb")).unwrap());
    let block = block();
    store
        .seed_for_tests(block.header.height - 1, block.header.parent_id.0)
        .unwrap();
    let (_, status) = watch::channel(IngestStatus {
        source_observed_at_ms: None,
        source_error: None,
        indexed: Some(block.header.height - 1),
        best: block.header.height,
        mode: Mode::Tip,
        source: "test".into(),
        halted: None,
        stalled: None,
    });
    let state = AppState {
        store: store.clone(),
        status,
        counters: Arc::new(Counters::default()),
        read_permits: Arc::new(Semaphore::new(32)),
    };
    let app = xp_api::router(
        state,
        &ApiConfig {
            per_second: 0,
            ..Default::default()
        },
    );
    let app = match source {
        Some(source) => app.layer(Extension(source as Arc<dyn BlockSource>)),
        None => app,
    };
    (dir, store, app)
}
async fn status(app: &Router, id: Hash32) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/txs/{}/status", hex32(&id)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn pending_confirmed_removed_and_reincluded_are_distinct() {
    let source = Source::new(pending());
    let (_dir, store, app) = app(Some(source.clone()));
    let block = block();
    let id = block.txs[1].id.0;
    let initial = status(&app, id).await;
    assert_eq!(initial["state"], "pending");
    assert!(initial["inclusion"].is_null());
    assert!(initial["mempool"]["first_seen_at_ms"].is_u64());
    assert_eq!(initial["pending"]["input_count"], block.txs[1].inputs.len());
    assert_eq!(
        initial["pending"]["details"]["inputs"][0],
        hex32(&block.txs[1].inputs[0].0)
    );
    assert_eq!(
        initial["pending"]["details"]["outputs"][0]["value"],
        block.txs[1].outputs[0].value.to_string()
    );
    assert_eq!(status(&app, id).await["pending"], initial["pending"]);
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        1,
        "details must share the cached observation"
    );
    store
        .apply_batch(std::slice::from_ref(&block), true)
        .unwrap();
    let confirmed = status(&app, id).await;
    assert_eq!(confirmed["state"], "confirmed");
    assert_eq!(confirmed["inclusion"]["confirmations"], 1);
    assert_eq!(
        confirmed["inclusion"]["block_id"],
        hex32(&block.header.id.0)
    );
    assert_eq!(confirmed["mempool"]["observation"], "not_checked");
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    store.rollback_to(block.header.height - 1).unwrap();
    source.mode.store(1, Ordering::SeqCst);
    let removed = status(&app, id).await;
    assert_eq!(removed["state"], "no_longer_observed");
    assert!(removed["inclusion"].is_null());
    assert_eq!(
        removed["previous_inclusion"]["block_id"],
        hex32(&block.header.id.0)
    );
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        2,
        "pre-confirmation observation must not survive inclusion removal"
    );
    let mut replacement = block.clone();
    replacement.header.id.0 = [123; 32];
    store.apply_batch(&[replacement], true).unwrap();
    let reincluded = status(&app, id).await;
    assert_eq!(reincluded["state"], "confirmed");
    assert_eq!(reincluded["inclusion"]["block_id"], hex32(&[123; 32]));
    assert_eq!(
        reincluded["previous_inclusion"]["block_id"],
        hex32(&block.header.id.0)
    );
}

#[tokio::test]
async fn node_outage_preserves_observation_and_absence_is_not_rejection() {
    let source = Source::new(pending());
    let (_dir, _, app) = app(Some(source.clone()));
    let id = block().txs[1].id.0;
    let initial = status(&app, id).await;
    source.mode.store(2, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(5050)).await;
    let outage = status(&app, id).await;
    assert_eq!(outage["state"], "unavailable");
    assert_eq!(outage["mempool"]["observation"], "unavailable");
    assert_eq!(
        outage["mempool"]["last_seen_at_ms"],
        initial["mempool"]["last_seen_at_ms"]
    );
    assert_eq!(outage["pending"], initial["pending"]);
    source.mode.store(1, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(2050)).await;
    let absent = status(&app, id).await;
    assert_eq!(absent["state"], "no_longer_observed");
    assert_eq!(
        absent["mempool"]["first_seen_at_ms"],
        initial["mempool"]["first_seen_at_ms"]
    );
    assert!(absent["mempool"]["error"].is_null());
}

#[tokio::test]
async fn unknown_unavailable_and_unsupported_are_not_conflated() {
    for (mode, expected, error) in [
        (1, "not_observed", Value::Null),
        (2, "unavailable", json!("unavailable")),
        (3, "unavailable", json!("unsupported")),
    ] {
        let source = Source::new(pending());
        source.mode.store(mode, Ordering::SeqCst);
        let (_dir, _, app) = app(Some(source));
        let result = status(&app, [88; 32]).await;
        assert_eq!(result["state"], expected);
        assert_eq!(result["mempool"]["error"], error);
        assert!(result["mempool"]["first_seen_at_ms"].is_null());
        assert!(result["pending"].is_null());
    }
}

#[tokio::test]
async fn mismatched_node_body_is_unavailable_not_a_pending_transaction() {
    let (_dir, _, app) = app(Some(Source::new(pending())));
    let result = status(&app, [88; 32]).await;
    assert_eq!(result["state"], "unavailable");
    assert!(result["pending"].is_null());
    assert!(result["mempool"]["first_seen_at_ms"].is_null());
}

#[tokio::test]
async fn missing_optional_output_metadata_preserves_summary_with_explicit_partial_details() {
    let id = block().txs[1].id.0;
    let raw = json!({"id":hex32(&id),"inputs":[{"boxId":hex32(&[9;32])}],"dataInputs":[],
        "outputs":[{"value":9007199254740993u64,"ergoTree":"00"}],"size":100})
    .to_string();
    let source = Source::new(raw);
    let (_dir, _, app) = app(Some(source.clone()));
    let result = status(&app, id).await;
    assert_eq!(result["state"], "pending");
    assert_eq!(result["pending"]["output_count"], 1);
    assert_eq!(result["pending"]["fee"], "0");
    let detail = &result["pending"]["details"];
    assert_eq!(detail["outputs"][0]["value"], "9007199254740993");
    assert!(detail["outputs"][0]["id"].is_null());
    assert!(detail["outputs"][0]["token_count"].is_null());
    assert_eq!(detail["complete"], false);
    assert_eq!(status(&app, id).await["pending"], result["pending"]);
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn confirmed_lookup_works_without_a_node_and_invalid_id_is_400() {
    let (_dir, store, app) = app(None);
    store.apply_batch(&[block()], true).unwrap();
    let result = status(&app, block().txs[1].id.0).await;
    assert_eq!(result["state"], "confirmed");
    assert_eq!(
        status(&app, [88; 32]).await["mempool"]["error"],
        "unsupported"
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/txs/not-an-id/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn simultaneous_requests_share_one_node_observation() {
    let source = Source::new(pending());
    let (_dir, _, app) = app(Some(source.clone()));
    let mut requests = Vec::new();
    for _ in 0..8 {
        let app = app.clone();
        requests.push(tokio::spawn(async move {
            status(&app, block().txs[1].id.0).await
        }));
    }
    for request in requests {
        assert_eq!(request.await.unwrap()["state"], "pending");
    }
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn inclusion_during_node_io_wins_over_the_pending_response() {
    let source = Source::new(pending());
    let (_dir, store, app) = app(Some(source.clone()));
    *source.confirm_on_lookup.lock().unwrap() = Some(store);
    let result = status(&app, block().txs[1].id.0).await;
    assert_eq!(result["state"], "confirmed");
    assert_eq!(result["inclusion"]["confirmations"], 1);
    assert_eq!(result["mempool"]["observation"], "not_checked");
}

#[tokio::test]
async fn confirmed_first_tracking_remembers_inputs_for_later_conflicts() {
    let first = block();
    let mut later =
        xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/1866001.json")).unwrap();
    let input = first.txs[0].outputs[0].id.0;
    later.txs[0].inputs = vec![xp_types::BoxId(input)];
    let id = later.txs[0].id.0;
    let source = Source::new(pending());
    source.mode.store(1, Ordering::SeqCst);
    let (_dir, store, app) = app(Some(source.clone()));
    store
        .apply_batch(&[first.clone(), later.clone()], true)
        .unwrap();
    assert_eq!(status(&app, id).await["state"], "confirmed");
    assert_eq!(source.calls.load(Ordering::SeqCst), 0);
    store.rollback_to(first.header.height).unwrap();
    later.header.id.0 = [87; 32];
    later.txs[0].id.0 = [86; 32];
    for output in &mut later.txs[0].outputs {
        output.tx_id.0 = [86; 32];
    }
    store.apply_batch(&[later], true).unwrap();
    let result = status(&app, id).await;
    assert_eq!(result["state"], "conflicted");
    assert_eq!(result["conflicts"][0]["input_id"], hex32(&input));
    assert_eq!(result["conflicts"][0]["tx_id"], hex32(&[86; 32]));
    assert!(result["previous_inclusion"].is_object());
    assert!(result["pending"].is_null());
}

#[tokio::test]
async fn conflicts_require_an_indexed_spend_and_clear_after_rollback() {
    let block = block();
    let mut later =
        xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/1866001.json")).unwrap();
    let target = block.txs[0].outputs[0].id.0;
    // Give the next fixture a definite canonical spend of a known earlier output.
    later.txs[0].inputs = vec![xp_types::BoxId(target)];
    let spender = later.txs[0].id.0;
    let id = [88; 32];
    let raw = json!({"id":hex32(&id),"inputs":[{"boxId":hex32(&target)}],"dataInputs":[],"outputs":[{"value":1000000,"ergoTree":"0008cd02"}],"size":100}).to_string();
    let (_dir, store, app) = app(Some(Source::new(raw)));
    store
        .apply_batch(&[block.clone(), later.clone()], true)
        .unwrap();
    let result = status(&app, id).await;
    assert_eq!(result["state"], "conflicted");
    assert_eq!(result["conflicts"][0]["tx_id"], hex32(&spender));
    assert_eq!(result["conflicts"][0]["height"], later.header.height);
    store.rollback_to(block.header.height).unwrap();
    let result = status(&app, id).await;
    assert_eq!(result["state"], "pending");
    assert_eq!(result["conflicts"], json!([]));
}
