//! A group read must never add aliases twice, mix snapshots, or invent missing balances.
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
use xp_types::{BoxId, Hash32, HeaderId, TxId};
use xp_wire::{DecodedBlock, DecodedBox, DecodedTx};

fn id(n: u32) -> Hash32 {
    let mut id = [0x85; 32];
    id[..4].copy_from_slice(&n.to_be_bytes());
    id
}
fn a() -> Vec<u8> {
    hex::decode("0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798").unwrap()
}
const B: &[u8] = &[0, 8, 0xd3];
const UNSEEN: &[u8] = &[0, 8, 0xd2];
fn address(tree: &[u8]) -> String {
    xp_wire::tree_info(tree).unwrap().address
}
fn output(n: u32, tree: &[u8], nano: u64, amount: u64) -> DecodedBox {
    DecodedBox {
        id: BoxId(id(n)),
        value: nano,
        tree_bytes: tree.to_vec(),
        tree_hash: xp_wire::tree_hash(tree),
        creation_height: 0,
        tx_id: TxId([0; 32]),
        index: 0,
        tokens: vec![(id(9), amount)],
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
async fn post_raw(app: &Router, body: String) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/addresses/balances")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn post(app: &Router, addresses: &[String]) -> (StatusCode, Value) {
    post_raw(app, json!({"addresses":addresses}).to_string()).await
}
fn fixture() -> (tempfile::TempDir, Arc<Store>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("groups.redb")).unwrap());
    store
        .seed_genesis(&[
            output(1, &a(), 9_007_199_254_740_993, 9_007_199_254_740_993),
            output(2, B, 7, 2),
        ])
        .unwrap();
    store
        .apply_batch(&[block(1, 100, [0; 32], vec![])], true)
        .unwrap();
    let app = router(store.clone());
    (dir, store, app)
}

// A P2S wrapper of a P2PK script is a distinct, valid address for the same canonical tree.
fn p2s_alias(tree: &[u8]) -> String {
    let alphabet = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut bytes = vec![3];
    bytes.extend_from_slice(tree);
    bytes.extend_from_slice(&xp_wire::tree::blake2b256(&bytes)[..4]);
    let mut digits = vec![0u32];
    for byte in bytes {
        let mut carry = u32::from(byte);
        for digit in &mut digits {
            carry += *digit * 256;
            *digit = carry % 58;
            carry /= 58;
        }
        while carry > 0 {
            digits.push(carry % 58);
            carry /= 58;
        }
    }
    digits
        .into_iter()
        .rev()
        .map(|digit| alphabet[digit as usize] as char)
        .collect()
}

#[tokio::test]
async fn aliases_are_counted_once_and_raw_amounts_remain_exact() {
    let (_dir, _store, app) = fixture();
    let canonical = address(&a());
    let alias = p2s_alias(&a());
    assert_ne!(alias, canonical);
    assert_eq!(
        xp_wire::tree::address_tree_hash(&alias).unwrap(),
        xp_wire::tree_hash(&a())
    );
    let (status, body) = post(
        &app,
        &[
            canonical.clone(),
            alias.clone(),
            address(B),
            canonical.clone(),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["consistency"], "single_reader");
    assert_eq!(body["scope"], "selected_address_scripts");
    assert_eq!(
        body["anchor"],
        json!({"height":1,"block_id":hex::encode(id(100))})
    );
    assert_eq!(body["indexed_height"], 1);
    assert_eq!(body["full_history"], true);
    assert!(body["partial_from"].is_null());
    assert_eq!(body["complete"], true);
    assert_eq!(body["requested_count"], 4);
    assert_eq!(body["resolved_script_count"], 2);
    assert_eq!(body["members"][0]["balance"]["nano"], "9007199254740993");
    assert_eq!(body["members"][1]["address"], alias);
    for index in [1, 3] {
        assert_eq!(body["members"][index]["status"], "duplicate");
        assert_eq!(body["members"][index]["duplicate_of"], 0);
        assert!(body["members"][index]["balance"].is_null());
    }
    assert_eq!(body["observed_totals"]["nano"], "9007199254741000");
    assert_eq!(
        body["observed_totals"]["tokens"],
        json!([{"id":hex::encode(id(9)),"amount":"9007199254740995"}])
    );
}

#[tokio::test]
async fn invalid_and_unseen_members_withhold_totals_without_hiding_known_balances() {
    let (_dir, _store, app) = fixture();
    let unseen = address(UNSEEN);
    let (status, body) = post(
        &app,
        &[
            address(&a()),
            unseen.clone(),
            "not-an-address".into(),
            unseen,
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["complete"], false);
    assert!(body["observed_totals"].is_null());
    assert_eq!(body["resolved_script_count"], 1);
    assert_eq!(body["members"][0]["balance"]["nano"], "9007199254740993");
    assert_eq!(body["members"][1]["status"], "unseen");
    assert_eq!(
        body["members"][1]["tree_hash"],
        hex::encode(xp_wire::tree_hash(UNSEEN).0)
    );
    assert_eq!(body["members"][2]["status"], "invalid");
    assert!(body["members"][2]["tree_hash"].is_null());
    assert_eq!(body["members"][3]["duplicate_of"], 1);
}

#[tokio::test]
async fn partial_store_returns_observed_balances_with_explicit_incomplete_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("partial.redb")).unwrap());
    store.seed_for_tests(10, id(100)).unwrap();
    let mut out = output(2, B, 7000, 50);
    out.tx_id = TxId(id(20));
    let tx = DecodedTx {
        id: TxId(id(20)),
        inputs: vec![BoxId(id(999))],
        data_inputs: vec![],
        outputs: vec![out],
        size: 100,
    };
    store
        .apply_batch(&[block(11, 101, id(100), vec![tx])], true)
        .unwrap();
    let (status, body) = post(&router(store), &[address(B)]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["full_history"], false);
    assert_eq!(body["complete"], false);
    assert_eq!(body["partial_from"], 10);
    assert_eq!(
        body["anchor"],
        json!({"height":11,"block_id":hex::encode(id(101))})
    );
    assert_eq!(body["observed_totals"]["nano"], "7000");
}

#[tokio::test]
async fn empty_and_genesis_only_stores_have_no_invented_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("empty.redb")).unwrap());
    let app = router(store.clone());
    let (_, empty) = post(&app, &[address(B)]).await;
    assert!(empty["anchor"].is_null());
    assert!(empty["indexed_height"].is_null());
    assert_eq!(empty["full_history"], false);
    assert!(empty["observed_totals"].is_null());
    store.seed_genesis(&[output(1, B, 5, 2)]).unwrap();
    let (_, genesis) = post(&app, &[address(B)]).await;
    assert!(genesis["anchor"].is_null());
    assert!(genesis["indexed_height"].is_null());
    assert_eq!(genesis["full_history"], true);
    assert_eq!(genesis["complete"], true);
    assert_eq!(genesis["observed_totals"]["nano"], "5");
}

#[tokio::test]
async fn append_and_reorg_return_balances_and_anchor_from_the_same_snapshot() {
    let (_dir, store, app) = fixture();
    let make_tx = |n, value| {
        let mut out = output(n, B, value, 2);
        out.tx_id = TxId(id(n + 20));
        DecodedTx {
            id: TxId(id(n + 20)),
            inputs: vec![BoxId(id(2))],
            data_inputs: vec![],
            outputs: vec![out],
            size: 100,
        }
    };
    store
        .apply_batch(&[block(2, 101, id(100), vec![make_tx(3, 6)])], true)
        .unwrap();
    let (_, before) = post(&app, &[address(B)]).await;
    assert_eq!(before["anchor"]["block_id"], hex::encode(id(101)));
    assert_eq!(before["observed_totals"]["nano"], "6");
    store.rollback_to(1).unwrap();
    store
        .apply_batch(&[block(2, 102, id(100), vec![make_tx(4, 4)])], true)
        .unwrap();
    let (_, after) = post(&app, &[address(B)]).await;
    assert_eq!(after["anchor"]["height"], 2);
    assert_eq!(after["anchor"]["block_id"], hex::encode(id(102)));
    assert_eq!(after["observed_totals"]["nano"], "4");
}

#[tokio::test]
async fn request_shape_count_and_byte_limits_fail_before_reading_members() {
    let (_dir, _store, app) = fixture();
    for value in [
        json!({"addresses":[]}),
        json!({"addresses":vec![address(B);101]}),
        json!({"addresses":[12]}),
        json!({"addresses":[address(B)],"label":"must stay local"}),
    ] {
        let (status, body) = post_raw(&app, value.to_string()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "invalid_group_request");
        assert!(body.get("members").is_none());
    }
    for raw in [
        " ".repeat(512_001),
        json!({"addresses":["x".repeat(4097)]}).to_string(),
        json!({"addresses":vec!["x".repeat(4096);32]}).to_string(),
    ] {
        let (status, body) = post_raw(&app, raw).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
        assert_eq!(body["code"], "group_request_limit");
        assert!(body.get("members").is_none());
    }
}

#[tokio::test]
async fn encoded_balance_bytes_and_token_counts_are_admitted_before_decode() {
    for (bytes, count, expected) in [
        (2 * 1024 * 1024 + 1, 0, "group_decode_limit"),
        (36, 5001, "group_work_limit"),
    ] {
        let (dir, store, app) = fixture();
        drop(app);
        drop(store);
        let path = dir.path().join("groups.redb");
        {
            let db = redb::Database::create(&path).unwrap();
            let write = db.begin_write().unwrap();
            let mut raw = vec![0u8; bytes];
            raw[8..12].copy_from_slice(&(count as u32).to_be_bytes());
            write
                .open_table(xp_store::tables::TREE_BALANCE)
                .unwrap()
                .insert(xp_wire::tree_hash(B).0.as_slice(), raw.as_slice())
                .unwrap();
            write.commit().unwrap();
        }
        let store = Arc::new(Store::open(&path).unwrap());
        let rd = Reader::new(&store).unwrap();
        let mut admitted = false;
        let error = rd.balance_admitted(&xp_wire::tree_hash(B).0, |length, tokens| {
            assert_eq!((length, tokens), (bytes, count));
            admitted = true;
            Err(xp_store::StoreError::ReadLimit("test_admission"))
        });
        assert!(admitted);
        assert!(matches!(
            error,
            Err(xp_store::StoreError::ReadLimit("test_admission"))
        ));
        let (status, body) = post(&router(store), &[address(B)]).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["code"], expected);
        assert!(body.get("observed_totals").is_none());
    }
}
