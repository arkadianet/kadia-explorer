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
    let store = Arc::new(Store::open(&dir.path().join("mining.redb")).unwrap());
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
                .uri(format!("/v1/mining?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
const RANGE: &str = "from_height=1866000&to_height=1866002&top=1";

#[tokio::test]
async fn real_blocks_cover_every_key_version_and_vote_with_exact_remainders() {
    let (_dir, store, app) = fixture();
    let (status, data) = get(&app, RANGE).await;
    assert_eq!(status, StatusCode::OK, "{data}");
    assert_eq!(data["scope"], "canonical_block_headers");
    assert_eq!(data["consistency"], "single_reader");
    assert_eq!(data["complete"], true);
    assert_eq!(data["block_count"], 3);
    assert_eq!(data["full_history"], false);
    assert_eq!(data["partial_from"], 1865999);
    let rd = Reader::new(&store).unwrap();
    let headers = (1866000..=1866002)
        .map(|h| rd.header_at(h).unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        data["totals"]["fees"],
        headers
            .iter()
            .map(|h| u128::from(h.fees))
            .sum::<u128>()
            .to_string()
    );
    assert_eq!(
        data["totals"]["transaction_count"],
        headers
            .iter()
            .map(|h| u64::from(h.tx_count))
            .sum::<u64>()
            .to_string()
    );
    let keys = &data["miner_keys"];
    assert_eq!(keys["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        keys["items"][0]["block_count"].as_u64().unwrap()
            + keys["other_block_count"].as_u64().unwrap(),
        3
    );
    assert_eq!(
        keys["items"][0]["fees"]
            .as_str()
            .unwrap()
            .parse::<u128>()
            .unwrap()
            + keys["other_fees"]
                .as_str()
                .unwrap()
                .parse::<u128>()
                .unwrap(),
        headers.iter().map(|h| u128::from(h.fees)).sum::<u128>()
    );
    assert_eq!(data["votes"]["known_blocks"], 3);
    assert_eq!(data["votes"]["unknown_blocks"], 0);
    assert_eq!(
        data["versions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["block_count"].as_u64().unwrap())
            .sum::<u64>(),
        3
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
    for field in ["totals", "miner_keys", "versions", "votes", "anchor"] {
        assert_eq!(appended[field], data[field]);
    }
    assert_eq!(appended["indexed_height"], 1866003);
}

#[tokio::test]
async fn rollback_and_same_height_replacement_reject_the_old_pin() {
    let (_dir, store, app) = fixture();
    let (_, data) = get(&app, RANGE).await;
    let query = format!(
        "{RANGE}&end_block_id={}",
        data["anchor"]["block_id"].as_str().unwrap()
    );
    store.rollback_to(1866001).unwrap();
    let (status, error) = get(&app, &query).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "mining_conflict");
    let mut replacement = block(1866002);
    replacement.header.id = xp_types::HeaderId([0x94; 32]);
    store.apply_batch(&[replacement], true).unwrap();
    assert_eq!(get(&app, &query).await.0, StatusCode::CONFLICT);
    assert_eq!(get(&app, RANGE).await.0, StatusCode::OK);
}

#[tokio::test]
async fn invalid_and_unretained_ranges_are_not_silently_truncated() {
    let (_dir, _store, app) = fixture();
    for query in [
        "",
        "from_height=0&to_height=3",
        "from_height=3&to_height=2",
        "from_height=1&to_height=20161",
        "from_height=1&to_height=2&top=51",
        "from_height=1&to_height=2&top=0",
        "from_height=1&to_height=2&extra=1",
        "from_height=1&to_height=2&end_block_id=bad",
        "from_height=01&to_height=2",
        "from_height=1&from_height=2&to_height=3",
    ] {
        assert_eq!(get(&app, query).await.0, StatusCode::BAD_REQUEST, "{query}");
    }
    let (status, error) = get(&app, "from_height=1865998&to_height=1866002").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error["code"], "mining_incomplete");
    assert_eq!(
        get(&app, "from_height=1866000&to_height=1866003").await.0,
        StatusCode::NOT_FOUND
    );
}

fn mutate(dir: &tempfile::TempDir, edit: impl FnOnce(&mut redb::Table<&[u8], &[u8]>)) -> Router {
    let db = redb::Database::open(dir.path().join("mining.redb")).unwrap();
    let write = db.begin_write().unwrap();
    {
        let mut table = write.open_table(xp_store::tables::HEADERS).unwrap();
        edit(&mut table);
    }
    write.commit().unwrap();
    drop(db);
    router(Arc::new(
        Store::open(&dir.path().join("mining.redb")).unwrap(),
    ))
}
#[tokio::test]
async fn unknown_votes_exact_fee_precision_and_stable_ties_are_preserved() {
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
            row.miner_pk = [0; 33];
            row.miner_pk[0] = 2;
            row.miner_pk[32] = (1866002 - height) as u8;
            row.version = (height - 1866000 + 1) as u8;
            row.raw_json = [
                r#"{"votes":"01ff00"}"#,
                r#"{"votes":"000000"}"#,
                r#"{"votes":"bad"}"#,
            ][(height - 1866000) as usize]
                .into();
            table
                .insert(key.as_slice(), row.encode().as_slice())
                .unwrap();
        }
    });
    let (status, data) = get(&app, RANGE).await;
    assert_eq!(status, StatusCode::OK, "{data}");
    assert_eq!(
        data["totals"]["fees"],
        (u128::from(u64::MAX) * 3).to_string()
    );
    let keys = &data["miner_keys"];
    assert_eq!(keys["distinct_count"], 3);
    assert_eq!(keys["other_key_count"], 2);
    assert_eq!(keys["other_block_count"], 2);
    assert_eq!(keys["other_fees"], (u128::from(u64::MAX) * 2).to_string());
    assert_eq!(keys["items"][0]["first_height"], 1866002);
    assert_eq!(keys["items"][0]["last_height"], 1866002);
    assert_eq!(data["votes"]["known_blocks"], 2);
    assert_eq!(data["votes"]["unknown_blocks"], 1);
    assert_eq!(data["votes"]["zero_vote_blocks"], 1);
    assert_eq!(data["votes"]["items"][0]["votes"], "000000");
    assert_eq!(data["votes"]["other_tuple_count"], 1);
    assert_eq!(data["votes"]["other_block_count"], 1);
    assert_eq!(
        data["versions"],
        serde_json::json!([{ "version":1,"block_count":1 },{ "version":2,"block_count":1 },{ "version":3,"block_count":1 }])
    );
}
#[tokio::test]
async fn missing_or_disconnected_headers_fail_the_whole_range() {
    for missing in [true, false] {
        let (dir, store, app) = fixture();
        drop(app);
        drop(store);
        let app = mutate(&dir, |table| {
            let key = 1866001u32.to_be_bytes();
            if missing {
                table.remove(key.as_slice()).unwrap();
            } else {
                let mut row = xp_store::rows::HeaderRow::decode(
                    table.get(key.as_slice()).unwrap().unwrap().value(),
                )
                .unwrap();
                row.parent_id = [0; 32];
                table
                    .insert(key.as_slice(), row.encode().as_slice())
                    .unwrap();
            }
        });
        let (status, data) = get(&app, RANGE).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(data.get("miner_keys").is_none());
    }
}
#[tokio::test]
async fn oversized_header_bytes_are_admitted_before_decode() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let app = mutate(&dir, |table| {
        let oversized = vec![0u8; 64 * 1024 * 1024 + 1];
        table
            .insert(1866001u32.to_be_bytes().as_slice(), oversized.as_slice())
            .unwrap();
    });
    let (status, data) = get(&app, RANGE).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{data}");
    assert_eq!(data["code"], "mining_decode_limit");
}
