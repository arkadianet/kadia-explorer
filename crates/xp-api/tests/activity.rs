//! Address activity exercises the real router and canonical store, including incomplete
//! references and bounded continuation. Amount assertions are raw integer strings.
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
use xp_types::{BoxId, Hash32, HeaderId, TxId};
use xp_wire::{DecodedBlock, DecodedBox, DecodedTx};

fn id(n: u32) -> Hash32 {
    let mut id = [0x91; 32];
    id[..4].copy_from_slice(&n.to_be_bytes());
    id
}
const A: &[u8] = &[0, 8, 0xd3];
const B: &[u8] = &[0, 8, 0xd2];
fn output(n: u32, tree: &[u8], value: u64, tokens: Vec<(Hash32, u64)>) -> DecodedBox {
    DecodedBox {
        id: BoxId(id(n)),
        value,
        tree_bytes: tree.to_vec(),
        tree_hash: xp_wire::tree_hash(tree),
        creation_height: 0,
        tx_id: TxId([0; 32]),
        index: 0,
        tokens,
        registers_json: "{}".into(),
        id_verified: false,
        size: 100,
    }
}
fn tx(n: u32, inputs: &[u32], mut outputs: Vec<DecodedBox>) -> DecodedTx {
    for (index, output) in outputs.iter_mut().enumerate() {
        output.tx_id = TxId(id(n));
        output.index = index as u16;
    }
    DecodedTx {
        id: TxId(id(n)),
        inputs: inputs.iter().map(|n| BoxId(id(*n))).collect(),
        data_inputs: vec![],
        outputs,
        size: 100,
    }
}
fn block(height: u32, n: u32, parent: Hash32, txs: Vec<DecodedTx>) -> DecodedBlock {
    let mut b =
        xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/1866000.json")).unwrap();
    b.header.height = height;
    b.header.id = HeaderId(id(n));
    b.header.parent_id = HeaderId(parent);
    b.header.timestamp = u64::from(height) * 1000;
    b.txs = txs;
    b
}
fn fee_tree() -> Vec<u8> {
    xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/1866000.json"))
        .unwrap()
        .txs
        .into_iter()
        .flat_map(|tx| tx.outputs)
        .find(|b| b.tree_hash.0 == xp_store::FEE_TREE_HASH)
        .unwrap()
        .tree_bytes
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
async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn path(tree: &[u8], query: &str) -> String {
    format!(
        "/v1/addresses/{}/activity?{query}",
        xp_wire::tree_info(tree).unwrap().address
    )
}
fn pair(page: &Value) -> String {
    format!(
        "cursor={}&snapshot={}",
        page["next_cursor"].as_str().unwrap(),
        page["next_snapshot"].as_str().unwrap()
    )
}
fn fixture() -> (tempfile::TempDir, Arc<Store>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("activity.redb")).unwrap());
    store
        .seed_genesis(&[output(1, A, 10_000, vec![(id(9), 100)])])
        .unwrap();
    let fee = fee_tree();
    let b1 = block(
        1,
        100,
        [0; 32],
        vec![tx(
            10,
            &[1],
            vec![
                output(2, A, 7_000, vec![(id(9), 60)]),
                output(3, B, 2_900, vec![(id(9), 40)]),
                output(4, &fee, 100, vec![]),
            ],
        )],
    );
    let b2 = block(
        2,
        101,
        b1.header.id.0,
        vec![tx(
            11,
            &[2, 3],
            vec![
                output(5, A, 7_050, vec![(id(9), 50)]),
                output(6, B, 2_750, vec![(id(9), 50)]),
                output(7, &fee, 100, vec![]),
            ],
        )],
    );
    store.apply_batch(&[b1, b2], true).unwrap();
    let app = router(store.clone());
    (dir, store, app)
}

#[tokio::test]
async fn exact_deltas_conserve_value_and_fee_is_not_deducted_twice() {
    let (_dir, _store, app) = fixture();
    let (status, a) = get(&app, &path(A, "dir=asc")).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["consistency"], "strict");
    assert!(a["partial_from"].is_null());
    assert_eq!(a["items"][0]["erg_delta"], "-3000");
    assert_eq!(a["items"][0]["tokens"][0]["delta"], "-40");
    assert_eq!(a["items"][0]["fee"], "100");
    assert_eq!(a["items"][0]["direction"], "sent");
    assert_eq!(a["items"][1]["erg_delta"], "50");
    assert_eq!(a["items"][1]["tokens"][0]["delta"], "-10");
    assert_eq!(a["items"][1]["direction"], "mixed");
    assert_eq!(
        a["items"][1]["coverage"],
        serde_json::json!({"complete":true,"resolved_inputs":2,"total_inputs":2})
    );
    let (_, b) = get(&app, &path(B, "dir=asc")).await;
    let (_, fees) = get(&app, &path(&fee_tree(), "dir=asc")).await;
    for index in 0..2 {
        let sum: i128 = [&a, &b, &fees]
            .into_iter()
            .map(|p| {
                p["items"][index]["erg_delta"]
                    .as_str()
                    .unwrap()
                    .parse::<i128>()
                    .unwrap()
            })
            .sum();
        assert_eq!(sum, 0);
        let token_sum: i128 = [&a, &b]
            .into_iter()
            .map(|p| {
                p["items"][index]["tokens"][0]["delta"]
                    .as_str()
                    .unwrap()
                    .parse::<i128>()
                    .unwrap()
            })
            .sum();
        assert_eq!(token_sum, 0);
    }
    let (_, erg) = get(&app, &path(A, "asset=erg&direction=received")).await;
    assert_eq!(erg["items"].as_array().unwrap().len(), 1);
    assert_eq!(erg["items"][0]["id"], hex::encode(id(11)));
    assert_eq!(erg["items"][0]["direction"], "received");
    let (_, token) = get(
        &app,
        &path(A, &format!("asset={}&direction=sent", hex::encode(id(9)))),
    )
    .await;
    assert_eq!(token["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn partial_inputs_withhold_every_delta_and_keep_uncertain_token_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("partial.redb")).unwrap());
    store.seed_for_tests(0, [0; 32]).unwrap();
    let b = block(
        1,
        100,
        [0; 32],
        vec![tx(10, &[999], vec![output(2, A, 7_000, vec![(id(9), 60)])])],
    );
    store.apply_batch(&[b], true).unwrap();
    let app = router(store);
    let (status, page) = get(&app, &path(A, "")).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page["partial_from"].is_number());
    let row = &page["items"][0];
    assert!(row["erg_delta"].is_null());
    assert!(row["tokens"][0]["delta"].is_null());
    assert_eq!(row["direction"], "unknown");
    assert_eq!(row["coverage"]["resolved_inputs"], 0);
    let (_, known) = get(&app, &path(A, &format!("asset={}", hex::encode(id(9))))).await;
    assert_eq!(known["items"][0]["asset_match"], "definite");
    let (_, uncertain) = get(&app, &path(A, &format!("asset={}", hex::encode(id(8))))).await;
    assert_eq!(uncertain["items"][0]["asset_match"], "uncertain");
    let (_, sent) = get(&app, &path(A, "direction=sent")).await;
    assert!(sent["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn utc_boundaries_are_half_open_and_continuations_bind_every_filter() {
    let (_dir, _store, app) = fixture();
    let (_, window) = get(&app, &path(A, "from_ms=1000&to_ms=2000")).await;
    assert_eq!(window["items"].as_array().unwrap().len(), 1);
    assert_eq!(window["items"][0]["height"], 1);
    let (_, page) = get(
        &app,
        &path(A, "limit=1&dir=asc&asset=erg&from_ms=0&to_ms=3000"),
    )
    .await;
    let pair = pair(&page);
    let (status, next) = get(
        &app,
        &path(
            A,
            &format!("limit=1&dir=asc&asset=erg&from_ms=0&to_ms=3000&{pair}"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["items"][0]["height"], 2);
    for query in [
        "dir=desc&asset=erg&from_ms=0&to_ms=3000",
        "dir=asc&asset=all&from_ms=0&to_ms=3000",
        "dir=asc&asset=erg&from_ms=1&to_ms=3000",
        "dir=asc&asset=erg&from_ms=0&to_ms=3001",
        "dir=asc&asset=erg&from_ms=0&to_ms=3000&direction=received",
    ] {
        assert_eq!(
            get(&app, &path(A, &format!("{query}&{pair}"))).await.0,
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
    assert_eq!(
        get(
            &app,
            &path(B, &format!("dir=asc&asset=erg&from_ms=0&to_ms=3000&{pair}"))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    for bad in [
        "consistency=best_effort",
        "limit=0",
        "limit=101",
        "from_ms=2000&to_ms=2000",
        "from_ms=-1",
        "to_ms=8640000000000001",
        "asset=garbage",
        "direction=garbage",
        "cursor=1",
    ] {
        assert_eq!(
            get(&app, &path(A, bad)).await.0,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn strict_continuation_rejects_append_rollback_and_same_height_replacement() {
    let (_dir, store, app) = fixture();
    let (_, page) = get(&app, &path(A, "limit=1")).await;
    let continuation = path(A, &pair(&page));
    let b3 = block(3, 102, id(101), vec![]);
    store.apply_batch(&[b3], true).unwrap();
    let (status, error) = get(&app, &continuation).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "snapshot_changed");
    store.rollback_to(1).unwrap();
    assert_eq!(get(&app, &continuation).await.0, StatusCode::CONFLICT);
    let replacement = block(2, 103, id(100), vec![]);
    store.apply_batch(&[replacement], true).unwrap();
    assert_eq!(get(&app, &continuation).await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn empty_filtered_scan_batches_continue_and_zero_changes_are_neutral() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("scan.redb")).unwrap());
    store.seed_genesis(&[output(1, A, 10000, vec![])]).unwrap();
    let mut previous = 1;
    let mut txs = vec![];
    for n in 0..205 {
        let next = 1000 + n;
        txs.push(tx(
            2000 + n,
            &[previous],
            vec![output(next, A, 10000, vec![])],
        ));
        previous = next;
    }
    store
        .apply_batch(&[block(1, 100, [0; 32], txs)], true)
        .unwrap();
    let app = router(store);
    let filter = format!("asset={}&dir=asc", hex::encode(id(9)));
    let (status, first) = get(&app, &path(A, &filter)).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert!(first["items"].as_array().unwrap().is_empty());
    assert_eq!(first["scanned"], 200);
    assert_eq!(first["scan_limit_reached"], true);
    let (_, last) = get(&app, &path(A, &format!("{filter}&{}", pair(&first)))).await;
    assert_eq!(last["scanned"], 5);
    assert_eq!(last["scan_limit_reached"], false);
    assert!(last["next_cursor"].is_null());
    let (_, neutral) = get(&app, &path(A, "direction=neutral&limit=1")).await;
    assert_eq!(neutral["items"][0]["erg_delta"], "0");
    assert_eq!(neutral["items"][0]["direction"], "neutral");
}

#[tokio::test]
async fn admission_rejects_large_encoded_box_before_decode_without_partial_money() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let dbpath = dir.path().join("activity.redb");
    {
        let db = redb::Database::create(&dbpath).unwrap();
        let write = db.begin_write().unwrap();
        {
            let mut boxes = write.open_table(xp_store::tables::BOXES).unwrap();
            boxes
                .insert(id(5).as_slice(), vec![0u8; 2 * 1024 * 1024 + 1].as_slice())
                .unwrap();
        }
        write.commit().unwrap();
    }
    let store = Arc::new(Store::open(&dbpath).unwrap());
    let app = router(store);
    let (status, error) = get(&app, &path(A, "")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
    assert_eq!(error["code"], "activity_decode_limit");
    assert!(error.get("items").is_none());
}

#[tokio::test]
async fn raw_amounts_above_javascript_precision_remain_exact() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("precision.redb")).unwrap());
    let amount = 9_007_199_254_740_999;
    store
        .seed_genesis(&[output(1, A, amount, vec![(id(9), amount)])])
        .unwrap();
    store
        .apply_batch(
            &[block(
                1,
                100,
                [0; 32],
                vec![tx(
                    10,
                    &[1],
                    vec![output(2, B, amount, vec![(id(9), amount)])],
                )],
            )],
            true,
        )
        .unwrap();
    let app = router(store);
    let (_, sent) = get(&app, &path(A, "")).await;
    let (_, received) = get(&app, &path(B, "")).await;
    for (a, b) in [
        (
            &sent["items"][0]["erg_delta"],
            &received["items"][0]["erg_delta"],
        ),
        (
            &sent["items"][0]["tokens"][0]["delta"],
            &received["items"][0]["tokens"][0]["delta"],
        ),
    ] {
        assert_eq!(a, "-9007199254740999");
        assert_eq!(b, "9007199254740999");
    }
}

#[tokio::test]
async fn oversized_transaction_fails_work_budget_before_box_resolution() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let dbpath = dir.path().join("activity.redb");
    {
        let db = redb::Database::create(&dbpath).unwrap();
        let write = db.begin_write().unwrap();
        {
            let mut txs = write.open_table(xp_store::tables::TXS).unwrap();
            let mut row =
                xp_store::rows::TxRow::decode(txs.get(id(11).as_slice()).unwrap().unwrap().value())
                    .unwrap();
            row.inputs = vec![id(999); 10_001];
            txs.insert(id(11).as_slice(), row.encode().as_slice())
                .unwrap();
        }
        write.commit().unwrap();
    }
    let store = Arc::new(Store::open(&dbpath).unwrap());
    let app = router(store.clone());
    let (status, error) = get(&app, &path(A, "")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
    assert_eq!(error["code"], "activity_work_limit");
    assert!(error.get("items").is_none());
    assert_eq!(
        store.lookup_counts()[0],
        0,
        "work admission must precede any box lookup"
    );
}

#[test]
fn header_byte_admission_rejects_large_raw_json_before_owned_decode() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let dbpath = dir.path().join("activity.redb");
    {
        let db = redb::Database::create(&dbpath).unwrap();
        let write = db.begin_write().unwrap();
        {
            let mut headers = write.open_table(xp_store::tables::HEADERS).unwrap();
            let key = 1u32.to_be_bytes();
            let mut header = xp_store::rows::HeaderRow::decode(
                headers.get(key.as_slice()).unwrap().unwrap().value(),
            )
            .unwrap();
            header.raw_json = "x".repeat(2 * 1024 * 1024);
            headers
                .insert(key.as_slice(), header.encode().as_slice())
                .unwrap();
        }
        write.commit().unwrap();
    }
    let store = Store::open(&dbpath).unwrap();
    let rd = Reader::new(&store).unwrap();
    let result = rd.header_at_admitted(1, |bytes| {
        assert!(bytes > 2 * 1024 * 1024);
        Err(xp_store::StoreError::ReadLimit("header_bytes"))
    });
    assert!(matches!(
        result,
        Err(xp_store::StoreError::ReadLimit("header_bytes"))
    ));
    assert_eq!(rd.header_id_at(1).unwrap(), Some(id(100)));
}

fn token_path(query: &str) -> String {
    format!("/v1/tokens/{}/txs?{query}", hex::encode(id(9)))
}

#[tokio::test]
async fn token_history_is_deduplicated_and_strict_append_safe_with_bound_cursors() {
    let (_dir, store, app) = fixture();
    let (status, first) = get(&app, &token_path("limit=1&dir=asc")).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["consistency"], "strict");
    assert_eq!(first["items"][0]["id"], hex::encode(id(10)));
    assert_eq!(first["history_context"]["scope"], "indexed_token_touches");
    assert!(first["history_context"]["partial_from"].is_null());
    let pair = pair(&first);
    store
        .apply_batch(
            &[block(
                3,
                102,
                id(101),
                vec![tx(12, &[5], vec![output(8, A, 7050, vec![(id(9), 50)])])],
            )],
            true,
        )
        .unwrap();
    let (status, next) = get(&app, &token_path(&format!("limit=1&dir=asc&{pair}"))).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["items"][0]["id"], hex::encode(id(11)));
    assert_eq!(next["anchor"], first["anchor"]);
    assert_eq!(next["observed_anchor"]["height"], 3);
    let (_, end) = get(
        &app,
        &token_path(&format!("limit=1&dir=asc&{}", crate::pair(&next))),
    )
    .await;
    assert!(end["items"].as_array().unwrap().is_empty());
    assert!(end["next_cursor"].is_null());
    for query in [
        format!("dir=desc&{pair}"),
        "cursor=1".into(),
        "consistency=best_effort".into(),
        "limit=101".into(),
    ] {
        assert_eq!(
            get(&app, &token_path(&query)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let other = format!("/v1/tokens/{}/txs?dir=asc&{pair}", hex::encode(id(8)));
    assert_eq!(get(&app, &other).await.0, StatusCode::BAD_REQUEST);
    store.rollback_to(1).unwrap();
    let (status, error) = get(&app, &token_path(&format!("dir=asc&{pair}"))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "snapshot_changed");
}

#[tokio::test]
async fn token_history_preparing_never_returns_empty_and_rebuild_preserves_fingerprint() {
    let (dir, store, app) = fixture();
    let before = store.fingerprint().unwrap();
    drop(app);
    drop(store);
    let path = dir.path().join("activity.redb");
    {
        let db = redb::Database::open(&path).unwrap();
        let write = db.begin_write().unwrap();
        for table in [
            xp_store::token_history::TOKEN_TXS,
            xp_store::token_history::TX_TOKENS,
            xp_store::token_history::TOKEN_HISTORY_META,
        ] {
            write.delete_table(table).unwrap();
        }
        write.commit().unwrap();
    }
    let store = Arc::new(Store::open(&path).unwrap());
    let app = router(store.clone());
    let (status, error) = get(&app, &token_path("")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{error}");
    assert_eq!(error["code"], "token_history_preparing");
    assert!(error.get("items").is_none());
    let mut ready = false;
    for _ in 0..20 {
        ready = store.backfill_token_history_batch(2).unwrap().ready;
        if ready {
            break;
        }
    }
    assert!(ready, "token history rebuild must make bounded progress");
    let (status, page) = get(&app, &token_path("")).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(store.fingerprint().unwrap(), before);
}

#[tokio::test]
async fn token_history_discloses_partial_range_without_claiming_missing_input_touches() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("partial.redb")).unwrap());
    store.seed_for_tests(0, [0; 32]).unwrap();
    store
        .apply_batch(
            &[block(
                1,
                100,
                [0; 32],
                vec![
                    tx(10, &[99], vec![output(2, A, 7000, vec![(id(9), 60)])]),
                    tx(11, &[98], vec![output(3, B, 7000, vec![])]),
                ],
            )],
            true,
        )
        .unwrap();
    let app = router(store);
    let (status, page) = get(&app, &token_path("")).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["history_context"]["partial_from"], 0);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["id"], hex::encode(id(10)));
    assert!(page["items"][0].get("coverage").is_none());
}

#[tokio::test]
async fn token_history_http_admits_transaction_bytes_before_decode() {
    let (dir, store, app) = fixture();
    drop(app);
    drop(store);
    let path = dir.path().join("activity.redb");
    {
        let db = redb::Database::open(&path).unwrap();
        let write = db.begin_write().unwrap();
        {
            write
                .open_table(xp_store::tables::TXS)
                .unwrap()
                .insert(id(11).as_slice(), vec![0u8; 2 * 1024 * 1024 + 1].as_slice())
                .unwrap();
        }
        write.commit().unwrap();
    }
    let app = router(Arc::new(Store::open(&path).unwrap()));
    let (status, error) = get(&app, &token_path("")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
    assert_eq!(error["code"], "token_history_budget");
    assert!(error.get("items").is_none());
}

#[tokio::test]
async fn missing_full_history_input_is_integrity_error_and_admission_precedes_tx_decode() {
    let (dir, store, app) = fixture();
    {
        let rd = Reader::new(&store).unwrap();
        let tree = xp_wire::tree_hash(A).0;
        let result =
            rd.tree_txs_bounded_admitted(&tree, None, 1, xp_store::read::Dir::Asc, None, |_| {
                Err(xp_store::StoreError::ReadLimit("test_admission"))
            });
        assert!(matches!(
            result,
            Err(xp_store::StoreError::ReadLimit("test_admission"))
        ));
    }
    drop(app);
    drop(store);
    let dbpath = dir.path().join("activity.redb");
    {
        let db = redb::Database::create(&dbpath).unwrap();
        let write = db.begin_write().unwrap();
        {
            write
                .open_table(xp_store::tables::BOXES)
                .unwrap()
                .remove(id(1).as_slice())
                .unwrap();
        }
        write.commit().unwrap();
    }
    let app = router(Arc::new(Store::open(&dbpath).unwrap()));
    let (status, error) = get(&app, &path(A, "dir=asc")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    assert_eq!(error["code"], "integrity_error");
    assert!(error.get("items").is_none());
}
