use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use redb::ReadableTable;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{watch, Semaphore};
use tower::ServiceExt;
use xp_api::{ApiConfig, AppState, Counters};
use xp_ingest::{IngestStatus, Mode};
use xp_store::{Reader, Store};

fn block(height: u32) -> xp_wire::DecodedBlock {
    xp_wire::decode_block(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/{height}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}
fn router(store: Arc<Store>) -> Router {
    let cfg = ApiConfig {
        per_second: 0,
        ..Default::default()
    };
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
    xp_api::router(
        AppState {
            store,
            status,
            counters: Arc::new(Counters::default()),
            read_permits: Arc::new(Semaphore::new(cfg.max_inflight_reads as usize)),
        },
        &cfg,
    )
}
fn fixture() -> (tempfile::TempDir, Arc<Store>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("network.redb")).unwrap());
    store
        .seed_for_tests(1865999, block(1866000).header.parent_id.0)
        .unwrap();
    store
        .apply_batch(&[block(1866000), block(1866001), block(1866002)], true)
        .unwrap();
    let app = router(store.clone());
    (dir, store, app)
}
async fn get(app: &Router, query: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/network/history?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
const RANGE: &str = "from_height=1866000&to_height=1866002&buckets=2";

#[tokio::test]
async fn real_applied_blocks_have_exact_complete_buckets_and_stable_append_pin() {
    let (_dir, store, app) = fixture();
    let (status, data) = get(&app, RANGE).await;
    assert_eq!(status, StatusCode::OK, "{data}");
    assert_eq!(data["scope"], "canonical_block_headers");
    assert_eq!(data["consistency"], "single_reader");
    assert_eq!(data["complete"], true);
    assert_eq!(data["full_history"], false);
    assert_eq!(data["partial_from"], 1865999);
    assert_eq!(data["totals"]["block_count"], 3);
    assert_eq!(data["bucket_width"], 2);
    assert_eq!(data["buckets"][0]["to_height"], 1866001);
    assert_eq!(data["buckets"][1]["block_count"], 1);
    let rd = Reader::new(&store).unwrap();
    let headers = (1866000..=1866002)
        .map(|h| rd.header_at(h).unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        data["totals"]["transaction_count"],
        headers
            .iter()
            .map(|h| u64::from(h.tx_count))
            .sum::<u64>()
            .to_string()
    );
    assert_eq!(
        data["totals"]["fees"],
        headers
            .iter()
            .map(|h| u128::from(h.fees))
            .sum::<u128>()
            .to_string()
    );
    let pin = format!(
        "{RANGE}&end_block_id={}",
        data["anchor"]["block_id"].as_str().unwrap()
    );
    drop(rd);
    let mut next = block(1866002);
    next.header.height = 1866003;
    next.header.parent_id = next.header.id;
    next.header.id = xp_types::HeaderId([0x93; 32]);
    next.txs.clear();
    store.apply_batch(&[next], true).unwrap();
    let (status, appended) = get(&app, &pin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(appended["totals"], data["totals"]);
    assert_eq!(appended["anchor"], data["anchor"]);
    assert_eq!(appended["indexed_height"], 1866003);
}

#[tokio::test]
async fn rollback_and_same_height_replacement_reject_the_old_pin() {
    let (_dir, store, app) = fixture();
    let (_, initial) = get(&app, RANGE).await;
    let query = format!(
        "{RANGE}&end_block_id={}",
        initial["anchor"]["block_id"].as_str().unwrap()
    );
    store.rollback_to(1866001).unwrap();
    let (status, error) = get(&app, &query).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "network_history_conflict");
    let mut replacement = block(1866002);
    replacement.header.id = xp_types::HeaderId([0x94; 32]);
    store.apply_batch(&[replacement], true).unwrap();
    assert_eq!(get(&app, &query).await.0, StatusCode::CONFLICT);
    assert_eq!(get(&app, RANGE).await.0, StatusCode::OK);
}

#[tokio::test]
async fn invalid_ranges_and_unretained_history_are_not_truncated() {
    let (_dir, _store, app) = fixture();
    for query in [
        "",
        "from_height=0&to_height=3",
        "from_height=3&to_height=2",
        "from_height=1&to_height=20161",
        "from_height=1&to_height=2&buckets=121",
        "from_height=1&to_height=2&buckets=0",
        "from_height=1&to_height=2&extra=1",
        "from_height=1&to_height=2&end_block_id=bad",
        "from_height=01&to_height=2",
    ] {
        assert_eq!(get(&app, query).await.0, StatusCode::BAD_REQUEST, "{query}");
    }
    let (status, error) = get(&app, "from_height=1865998&to_height=1866002").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error["code"], "network_history_incomplete");
    assert_eq!(
        get(&app, "from_height=1866000&to_height=1866003").await.0,
        StatusCode::NOT_FOUND
    );
}

fn mutate(dir: &tempfile::TempDir, edit: impl FnOnce(&mut redb::Table<&[u8], &[u8]>)) -> Router {
    let db = redb::Database::open(dir.path().join("network.redb")).unwrap();
    let write = db.begin_write().unwrap();
    {
        let mut table = write.open_table(xp_store::tables::HEADERS).unwrap();
        edit(&mut table);
    }
    write.commit().unwrap();
    drop(db);
    router(Arc::new(
        Store::open(&dir.path().join("network.redb")).unwrap(),
    ))
}
#[tokio::test]
async fn large_exact_totals_and_nonmonotonic_timestamps_do_not_lose_data() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let app = mutate(&dir, |table| {
        for height in 1866000u32..=1866002 {
            let key = height.to_be_bytes();
            let mut row = xp_store::rows::HeaderRow::decode(
                table.get(key.as_slice()).unwrap().unwrap().value(),
            )
            .unwrap();
            row.fees = u64::MAX;
            row.difficulty = u128::MAX - u128::from(height - 1866000);
            row.timestamp = [3000, 1000, 2000][(height - 1866000) as usize];
            table
                .insert(key.as_slice(), row.encode().as_slice())
                .unwrap();
        }
    });
    let (status, body) = get(&app, RANGE).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["totals"]["fees"],
        (u128::from(u64::MAX) * 3).to_string()
    );
    assert_eq!(body["totals"]["difficulty_max"], u128::MAX.to_string());
    assert_eq!(
        body["totals"]["difficulty_end"],
        (u128::MAX - 2).to_string()
    );
    assert_eq!(body["totals"]["first_timestamp"], 3000);
    assert_eq!(body["totals"]["last_timestamp"], 2000);
    assert_eq!(body["totals"]["earliest_timestamp"], 1000);
    assert_eq!(body["totals"]["latest_timestamp"], 3000);
}
#[tokio::test]
async fn a_missing_interior_header_fails_the_entire_range() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let app = mutate(&dir, |table| {
        table.remove(1866001u32.to_be_bytes().as_slice()).unwrap();
    });
    let (status, data) = get(&app, RANGE).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(data.get("buckets").is_none());
}
#[tokio::test]
async fn oversized_stored_header_is_rejected_before_malformed_decode() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let app = mutate(&dir, |table| {
        // Intentionally not a decodable HeaderRow: a 422 demonstrates admission happened first.
        let oversized = vec![0u8; 64 * 1024 * 1024 + 1];
        table
            .insert(1866001u32.to_be_bytes().as_slice(), oversized.as_slice())
            .unwrap();
    });
    let (status, data) = get(&app, RANGE).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{data}");
    assert_eq!(data["code"], "network_history_decode_limit");
}
