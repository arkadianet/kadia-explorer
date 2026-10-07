//! Compact rent reads retain one canonical context and refuse unbounded hydration.
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
use xp_store::Store;
use xp_types::{BoxId, Hash32, HeaderId, TxId};
use xp_wire::{DecodedBlock, DecodedBox, DecodedTx};

const TREE: &[u8] = &[0, 8, 0xd3];
fn id(n: u32) -> Hash32 {
    let mut id = [0x95; 32];
    id[..4].copy_from_slice(&n.to_be_bytes());
    id
}
fn address() -> String {
    xp_wire::tree_info(TREE).unwrap().address
}
fn output(n: u32) -> DecodedBox {
    DecodedBox {
        id: BoxId(id(n)),
        value: 1_000_000_000,
        tree_bytes: TREE.to_vec(),
        tree_hash: xp_wire::tree_hash(TREE),
        creation_height: 0,
        tx_id: TxId([0; 32]),
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
async fn get(app: &Router, query: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/addresses/{}/rent{query}", address()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn fixture(boxes: &[DecodedBox]) -> (tempfile::TempDir, Arc<Store>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("rent.redb")).unwrap());
    store.seed_genesis(boxes).unwrap();
    let app = router(store.clone());
    (dir, store, app)
}

#[tokio::test]
async fn compact_view_omits_hydration_and_legacy_remains_compatible() {
    let (_dir, store, app) = fixture(&[output(1)]);
    store
        .apply_batch(&[block(1, 100, [0; 32], vec![])], true)
        .unwrap();
    let (status, body) = get(&app, "?view=exposure").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["context"],
        json!({"scope":"indexed_unspent_boxes","address":address(),
        "tree_hash":hex::encode(xp_wire::tree_hash(TREE).0),"indexed_height":1,
        "anchor":{"height":1,"block_id":hex::encode(id(100))},"full_history":true,"partial_from":null,
        "scanned_count":1,"scan_limit":5000,"scan_complete":true})
    );
    let item = &body["items"][0];
    assert_eq!(item["value"], "1000000000");
    assert_eq!(item["token_count"], 0);
    assert_eq!(item["rent"]["consensus_fee_nano"], "125000000");
    for field in ["registers", "ergo_tree", "address", "tokens"] {
        assert!(item.get(field).is_none(), "unexpected {field}");
    }
    let (status, legacy) = get(&app, "").await;
    assert_eq!(status, StatusCode::OK, "{legacy}");
    assert!(legacy.get("context").is_none());
    assert!(legacy["items"][0].get("tokens").is_some());
    assert_eq!(get(&app, "?view=unknown").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn genesis_without_tip_does_not_assert_maturity_and_noncollectible_fee_stays_signed() {
    let mut overflow = output(1);
    overflow.size = 2000;
    let (_dir, _store, app) = fixture(&[overflow]);
    let (status, body) = get(&app, "?view=exposure").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["context"]["anchor"].is_null());
    assert!(body["context"]["indexed_height"].is_null());
    assert_eq!(body["context"]["full_history"], true);
    assert_eq!(body["items"][0]["rent"]["claimable_at_tip"], false);
    assert_eq!(body["items"][0]["rent"]["collectible"], false);
    assert_eq!(
        body["items"][0]["rent"]["consensus_fee_nano"],
        "-1794967296"
    );
}

#[tokio::test]
async fn exact_scan_cap_is_complete_but_one_more_key_is_truncated_before_decode() {
    for count in [5000, 5001] {
        let mut boxes: Vec<_> = (1..=count).map(output).collect();
        if count == 5001 {
            boxes[5000].registers_json = "x".repeat(2 * 1024 * 1024 + 1);
        }
        let (_dir, _store, app) = fixture(&boxes);
        let (status, body) = get(&app, "?view=exposure").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["items"].as_array().unwrap().len(), 5000);
        assert_eq!(body["truncated"], count == 5001);
        assert_eq!(body["context"]["scan_complete"], count == 5000);
    }
}

#[tokio::test]
async fn oversized_rows_and_token_work_fail_without_partial_totals_or_register_parsing() {
    let mut invalid_json = output(1);
    invalid_json.registers_json = "this is not JSON".into();
    let (_dir, _store, app) = fixture(&[invalid_json]);
    assert_eq!(get(&app, "?view=exposure").await.0, StatusCode::OK);
    for (tokens, registers, code) in [
        (
            vec![],
            "x".repeat(2 * 1024 * 1024 + 1),
            "rent_exposure_decode_limit",
        ),
        (
            (1..=10_001).map(|n| (id(n), 1)).collect(),
            "{}".into(),
            "rent_exposure_work_limit",
        ),
    ] {
        let mut row = output(1);
        row.tokens = tokens;
        row.registers_json = registers;
        let (_dir, _store, app) = fixture(&[row]);
        let (status, body) = get(&app, "?view=exposure").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["code"], code);
        assert!(body.get("items").is_none());
        assert!(body.get("context").is_none());
    }
}

#[tokio::test]
async fn partial_snapshot_keeps_its_own_maturity_and_canonical_anchor_after_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("partial.redb")).unwrap());
    store.seed_for_tests(1_051_199, id(100)).unwrap();
    let make_tx = |n, size| {
        let mut row = output(n);
        row.tx_id = TxId(id(n + 20));
        row.size = size;
        DecodedTx {
            id: row.tx_id,
            inputs: vec![BoxId(id(999))],
            data_inputs: vec![],
            outputs: vec![row],
            size: 100,
        }
    };
    store
        .apply_batch(
            &[block(1_051_200, 101, id(100), vec![make_tx(1, 100)])],
            true,
        )
        .unwrap();
    let app = router(store.clone());
    let (_, before) = get(&app, "?view=exposure").await;
    assert_eq!(before["context"]["partial_from"], 1_051_199);
    assert_eq!(before["context"]["full_history"], false);
    assert_eq!(
        before["context"]["anchor"]["block_id"],
        hex::encode(id(101))
    );
    assert_eq!(before["items"][0]["rent"]["claimable_at_tip"], true);
    store.rollback_to(1_051_199).unwrap();
    store
        .apply_batch(
            &[block(1_051_200, 102, id(100), vec![make_tx(2, 2000)])],
            true,
        )
        .unwrap();
    let (_, after) = get(&app, "?view=exposure").await;
    assert_eq!(after["context"]["anchor"]["block_id"], hex::encode(id(102)));
    assert_eq!(after["items"][0]["id"], hex::encode(id(2)));
    assert_eq!(after["items"][0]["rent"]["claimable_at_tip"], true);
    assert_eq!(after["items"][0]["rent"]["collectible"], false);
}

#[tokio::test]
async fn spent_boxes_are_excluded_and_a_known_empty_address_is_distinct_from_unseen() {
    let (_dir, store, app) = fixture(&[output(1)]);
    let tx = DecodedTx {
        id: TxId(id(20)),
        inputs: vec![BoxId(id(1))],
        data_inputs: vec![],
        outputs: vec![],
        size: 100,
    };
    store
        .apply_batch(&[block(1, 100, [0; 32], vec![tx])], true)
        .unwrap();
    let (status, body) = get(&app, "?view=exposure").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"], json!([]));
    assert_eq!(body["context"]["scan_complete"], true);
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("empty.redb")).unwrap());
    assert_eq!(
        get(&router(store), "?view=exposure").await.0,
        StatusCode::NOT_FOUND
    );
}
