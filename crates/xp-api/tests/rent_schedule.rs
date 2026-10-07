//! Real-router rent calendar coverage: filtering precedes caps, integer quantities remain
//! exact, and the broader batch overview never borrows completeness from a short page.
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{watch, Semaphore};
use tower::ServiceExt;
use xp_api::{ApiConfig, AppState, Counters};
use xp_ingest::{IngestStatus, Mode};
use xp_store::{Reader, Store};
use xp_types::rent::RENT_PERIOD;
use xp_types::{BoxId, Hash32, HeaderId, TxId};
use xp_wire::{DecodedBlock, DecodedBox, DecodedTx};

const TREE: &[u8] = &[0, 8, 0xd3];
const TIP: u32 = RENT_PERIOD - 1;
const TIME: u64 = 1_800_000_000_000;
const REEMISSION: &str = "d9a2cc8a09abfaed87afacfbb7daee79a6b26f10c6613fc13d3f3953e5521d1a";

fn id(n: u32) -> Hash32 {
    let mut id = [0x97; 32];
    id[..4].copy_from_slice(&n.to_be_bytes());
    id
}

fn output(n: u32, creation: u32) -> DecodedBox {
    DecodedBox {
        id: BoxId(id(n)),
        value: 1_000_000,
        tree_bytes: TREE.to_vec(),
        tree_hash: xp_wire::tree_hash(TREE),
        creation_height: creation,
        tx_id: TxId(id(90_000)),
        index: 0,
        tokens: vec![],
        registers_json: "{}".into(),
        id_verified: false,
        size: 100,
    }
}

fn block(height: u32, n: u32, parent: Hash32, txs: Vec<DecodedTx>) -> DecodedBlock {
    let mut block =
        xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/1866000.json")).unwrap();
    block.header.height = height;
    block.header.id = HeaderId(id(n));
    block.header.parent_id = HeaderId(parent);
    block.header.timestamp = TIME;
    block.txs = txs;
    block
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

fn fixture(mut boxes: Vec<DecodedBox>) -> (tempfile::TempDir, Arc<Store>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("schedule.redb")).unwrap());
    store.seed_for_tests(TIP - 1, id(99_000)).unwrap();
    for (index, row) in boxes.iter_mut().enumerate() {
        row.index = index as u16;
    }
    store
        .apply_batch(
            &[block(
                TIP,
                99_001,
                id(99_000),
                vec![DecodedTx {
                    id: TxId(id(90_000)),
                    inputs: vec![BoxId(id(99_999))],
                    data_inputs: vec![],
                    outputs: boxes,
                    size: 100,
                }],
            )],
            true,
        )
        .unwrap();
    let app = router(store.clone());
    (dir, store, app)
}

async fn get(app: &Router, query: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/rent/schedule?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn continuation(page: &Value, query: &str) -> String {
    format!(
        "{query}&cursor={}&snapshot={}",
        page["next_cursor"].as_str().unwrap(),
        page["next_snapshot"].as_str().unwrap()
    )
}

#[tokio::test]
async fn filtered_candidates_use_exact_wrapping_fee_equality_and_protocol_guard() {
    let token = id(88_000);
    let mut rows: Vec<_> = (1..=6).map(|n| output(n, n - 1)).collect();
    rows[0].size = 2000; // Negative consensus fee; nominal fee would look positive.
    rows[1].value = 125_000_000; // Equality permits the full claim branch.
    rows[2].value = 125_000_001;
    rows[3].size = 3436; // Positive again after signed wrapping; no size cutoff.
    rows[3].value = 10_000;
    rows[4].tokens = vec![(xp_types::parse_hex32(REEMISSION).unwrap(), 1)];
    rows[5].value = 9_007_199_254_740_993;
    for row in &mut rows {
        row.tokens.push((token, 9_007_199_254_740_993));
    }
    let (_dir, _store, app) = fixture(rows);
    let query = format!("mode=full_claim&token_id={}&limit=1", hex::encode(token));
    let (status, first) = get(&app, &query).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["items"][0]["box_id"], hex::encode(id(2)));
    assert_eq!(first["items"][0]["value"], "125000000");
    assert_eq!(first["items"][0]["collectible_due_nano"], "125000000");
    assert_eq!(first["items"][0]["tokens"][0]["amount"], "9007199254740993");
    assert_eq!(first["schedule_context"]["scanned"], 2);
    let totals = &first["batches"][0];
    assert_eq!(totals["box_count"], 2);
    assert_eq!(totals["full_claim_count"], 2);
    assert_eq!(totals["full_claim_value_nano"], "125010000");
    assert_eq!(
        totals["selected_token_full_claim_amount"],
        "18014398509481986"
    );
    assert_eq!(first["schedule_context"]["batch_complete"], true);
    let (_, next) = get(&app, &continuation(&first, &query)).await;
    assert_eq!(next["items"][0]["box_id"], hex::encode(id(4)));
    assert_eq!(next["items"][0]["consensus_fee_nano"], "32704");
    assert!(next["batches"].is_null());
    let (_, all) = get(&app, "mode=all").await;
    assert_eq!(all["items"][0]["collectible"], false);
    assert_eq!(all["items"][4]["protocol_constrained"], true);
    assert_eq!(all["items"][4]["collectible"], false);
    assert_eq!(all["items"][4]["collectible_due_nano"], "0");
    assert_eq!(all["items"][4]["full_claim"], false);
    assert_eq!(all["items"][5]["value"], "9007199254740993");
}

#[tokio::test]
async fn windows_are_inclusive_and_order_is_maturity_then_index_not_box_id() {
    let rows = vec![
        output(5, 720),
        output(9, 0),
        output(2, 719),
        output(3, 0),
        output(6, 64_800),
    ];
    let (_dir, _store, app) = fixture(rows);
    let (status, page) = get(&app, "window=24h").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let ids: Vec<_> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["box_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        ids,
        vec![hex::encode(id(9)), hex::encode(id(3)), hex::encode(id(2))]
    );
    assert_eq!(page["items"][0]["estimated_maturity_ms"], TIME + 120_000);
    assert_eq!(page["schedule_context"]["from_height"], TIP + 1);
    assert_eq!(page["schedule_context"]["to_height"], TIP + 720);
    assert_eq!(page["schedule_context"]["full_history"], false);
    assert_eq!(page["schedule_context"]["partial_from"], TIP - 1);
    for (window, span, count) in [("7d", 5040, 4), ("30d", 21600, 4), ("90d", 64800, 4)] {
        let (_, page) = get(&app, &format!("window={window}")).await;
        assert_eq!(page["schedule_context"]["to_height"], TIP + span);
        assert_eq!(page["items"].as_array().unwrap().len(), count);
    }
}

#[tokio::test]
async fn empty_filtered_scan_pages_advance_and_batch_overview_is_not_page_capped() {
    let token = id(88_001);
    let mut rows: Vec<_> = (1..=2001).map(|n| output(n, 0)).collect();
    rows[2000].tokens = vec![(token, 17)];
    let (_dir, _store, app) = fixture(rows);
    let query = format!("token_id={}", hex::encode(token));
    let (status, first) = get(&app, &query).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["items"], json!([]));
    assert_eq!(first["schedule_context"]["scanned"], 2000);
    assert_eq!(first["schedule_context"]["scan_limit_reached"], true);
    assert_eq!(first["schedule_context"]["complete"], false);
    assert_eq!(first["schedule_context"]["batch_scanned"], 2001);
    assert_eq!(first["batches"][0]["box_count"], 1);
    let (_, second) = get(&app, &continuation(&first, &query)).await;
    assert_eq!(second["items"][0]["box_id"], hex::encode(id(2001)));
    assert_eq!(second["schedule_context"]["scanned"], 1);
    assert_eq!(second["schedule_context"]["complete"], true);
    assert!(second["next_cursor"].is_null());
    assert!(second["batches"].is_null());
    let (_, unfiltered) = get(&app, "limit=1").await;
    assert_eq!(unfiltered["items"].as_array().unwrap().len(), 1);
    assert_eq!(unfiltered["batches"][0]["box_count"], 2001);
}

#[tokio::test]
async fn current_state_continuation_binds_filters_and_rejects_appends_and_reorgs() {
    let (_dir, store, app) = fixture(vec![output(1, 0), output(2, 1)]);
    let query = "limit=1&window=24h&mode=all";
    let (_, first) = get(&app, query).await;
    let next = continuation(&first, query);
    assert_eq!(get(&app, &next).await.0, StatusCode::OK);
    for replacement in [
        next.replace("window=24h", "window=7d"),
        next.replace("mode=all", "mode=full_claim"),
        format!("{next}&token_id={}", hex::encode(id(88_000))),
    ] {
        assert_eq!(get(&app, &replacement).await.0, StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        get(
            &app,
            &format!("cursor={}", first["next_cursor"].as_str().unwrap())
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    store
        .apply_batch(&[block(TIP + 1, 99_002, id(99_001), vec![])], true)
        .unwrap();
    let (status, conflict) = get(&app, &next).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["code"], "snapshot_changed");
    store.rollback_to(TIP - 1).unwrap();
    store
        .apply_batch(&[block(TIP, 99_003, id(99_000), vec![])], true)
        .unwrap();
    assert_eq!(get(&app, &next).await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn unknown_anchor_and_invalid_queries_never_masquerade_as_empty_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("empty.redb")).unwrap());
    store.seed_genesis(&[]).unwrap();
    let app = router(store.clone());
    let (status, body) = get(&app, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "rent_schedule_unavailable");
    assert!(body.get("items").is_none());
    for query in [
        "window=1d",
        "mode=bad",
        "token_id=nope",
        "limit=101",
        "limit=0",
        "consistency=best_effort",
    ] {
        assert_eq!(get(&app, query).await.0, StatusCode::BAD_REQUEST, "{query}");
    }
    store
        .apply_batch(&[block(1, 99_001, [0; 32], vec![])], true)
        .unwrap();
    let (_, empty) = get(&app, "").await;
    assert_eq!(empty["items"], json!([]));
    assert_eq!(empty["schedule_context"]["full_history"], true);
    assert_eq!(empty["schedule_context"]["complete"], true);
    assert_eq!(empty["schedule_context"]["batch_complete"], true);
}

#[tokio::test]
async fn page_decode_limit_is_atomic_and_overview_decode_cutoff_is_explicit() {
    let mut oversized = output(2, 1);
    oversized.registers_json = "x".repeat(16 * 1024 * 1024 + 1);
    let (_dir, _store, app) = fixture(vec![output(1, 0), oversized]);
    let (status, page) = get(&app, "limit=1").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["schedule_context"]["batch_complete"], false);
    assert_eq!(
        page["schedule_context"]["batch_stop_reason"],
        "decode_limit"
    );
    assert_eq!(page["schedule_context"]["batch_scanned"], 1);
    assert_eq!(page["batches"][0]["box_count"], 1);
    let (status, error) = get(&app, &continuation(&page, "limit=1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error["code"], "rent_schedule_decode_limit");
    assert!(error.get("items").is_none());
    assert!(error.get("batches").is_none());
}

#[test]
fn unhydrated_range_handles_maximum_height_and_exclusive_final_cursor() {
    let (_dir, store, _app) = fixture(vec![output(1, u32::MAX), output(2, u32::MAX)]);
    let rd = Reader::new(&store).unwrap();
    let first = rd.rent_candidates(u32::MAX, u32::MAX, None, 1).unwrap();
    assert!(first.more);
    assert_eq!(first.items[0].box_id, id(1));
    let cursor = Some((u32::MAX, first.items[0].gidx));
    let second = rd.rent_candidates(u32::MAX, u32::MAX, cursor, 1).unwrap();
    assert!(!second.more);
    assert_eq!(second.items[0].box_id, id(2));
    let last = Some((u32::MAX, second.items[0].gidx));
    assert!(rd
        .rent_candidates(u32::MAX, u32::MAX, last, 1)
        .unwrap()
        .items
        .is_empty());
    assert!(rd.rent_candidates(0, u32::MAX, None, 2001).is_err());
}
