//! Route-level tests: drive the real `axum::Router` returned by `xp_api::router` with
//! `tower::ServiceExt::oneshot` over a store holding the three block fixtures.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::watch;
use tower::ServiceExt;
use xp_ingest::{IngestStatus, Mode, StalledInfo};
use xp_store::Store;

fn fixture(h: u32) -> xp_wire::DecodedBlock {
    xp_wire::decode_block(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/{h}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}

/// A store with the three fixture blocks applied, plus the router over it. The `TempDir` is
/// returned so the caller keeps the database alive for the duration of the test.
fn app() -> (tempfile::TempDir, Router) {
    app_with_stall(None)
}

/// Same store and router, but with `stalled` on the published ingest status set to `stall`.
fn app_with_stall(stall: Option<StalledInfo>) -> (tempfile::TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("x.redb")).unwrap();
    let b0 = fixture(1866000);
    store
        .seed_for_tests(1865999, b0.header.parent_id.0)
        .unwrap();
    store
        .apply_batch(
            &[fixture(1866000), fixture(1866001), fixture(1866002)],
            true,
        )
        .unwrap();
    // The sender is dropped immediately; `Receiver::borrow` still yields the last value, so
    // the handlers see the stub regardless.
    let (_tx, rx) = watch::channel(IngestStatus {
        indexed: Some(1866002),
        best: 1866002,
        mode: Mode::Tip,
        source: "test".into(),
        halted: None,
        stalled: stall,
    });
    let state = xp_api::AppState {
        store: Arc::new(store),
        status: rx,
    };
    (dir, xp_api::router(state))
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

const COINBASE_TX_HEIGHT: u32 = 1866000;
const UNKNOWN_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000001";

#[tokio::test]
async fn status_reflects_the_stubbed_ingest_status() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/status").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["indexed"], 1866002);
    assert_eq!(v["best"], 1866002);
    assert_eq!(v["mode"], "tip");
    assert_eq!(v["source"], "test");
    assert!(v["halted"].is_null());
    assert_eq!(v["lag_blocks"], 0);
    assert!(
        v["stalled"].is_null(),
        "a healthy indexer reports stalled: null, not a missing key"
    );
}

/// A stall is a first-class, machine-readable part of the status: a client watching a frozen
/// `indexed` must be able to see *why* without reading the server's logs.
#[tokio::test]
async fn status_serialises_a_stall() {
    let (_d, app) = app_with_stall(Some(StalledInfo {
        height: 545_684,
        since_secs: 900,
        reason: "source announced a header at 545684 but serves no block body".into(),
    }));
    let (st, v) = get(&app, "/v1/status").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["stalled"]["height"], 545_684);
    assert_eq!(v["stalled"]["since_secs"], 900);
    assert_eq!(
        v["stalled"]["reason"],
        "source announced a header at 545684 but serves no block body"
    );
    assert!(v["halted"].is_null(), "a stall is not a halt");
}

#[tokio::test]
async fn blocks_list_is_newest_first_and_pages_by_cursor() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/blocks?limit=2").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["items"][0]["height"], 1866002);
    assert_eq!(v["items"][1]["height"], 1866001);
    assert_eq!(v["next_cursor"], "1866001");

    let (st, v2) = get(&app, "/v1/blocks?limit=2&cursor=1866001").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v2["items"][0]["height"], 1866000);
}

/// `/v1/blocks` is descending-only: `dir=asc` is a 400, not a silently ignored parameter.
#[tokio::test]
async fn blocks_list_rejects_ascending_and_bogus_dir() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/blocks?dir=asc").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(v["status"], 400);

    let (st, _) = get(&app, "/v1/blocks?dir=bogus").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    let (st, _) = get(&app, "/v1/blocks?dir=desc").await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn block_by_height_and_by_id() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/blocks/1866001").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["height"], 1866001);
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 64);
    assert!(v["difficulty"].is_string());
    assert!(v["fees"].is_string());
    assert!(v["reward"].is_string());
    assert_eq!(v["miner_pk"].as_str().unwrap().len(), 66);

    let (st, byid) = get(&app, &format!("/v1/blocks/{id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(byid["height"], 1866001);
}

#[tokio::test]
async fn block_txs_returns_every_tx_of_the_block() {
    let (_d, app) = app();
    let expected = fixture(1866001).txs.len();
    let (st, v) = get(&app, "/v1/blocks/1866001/txs").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), expected);
    for tx in v.as_array().unwrap() {
        assert_eq!(tx["height"], 1866001);
    }
}

#[tokio::test]
async fn unknown_block_is_404_and_bad_id_is_400() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/blocks/999999999").await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(v["type"], "about:blank");
    assert_eq!(v["status"], 404);
    assert!(v["title"].is_string());

    let (st, _) = get(&app, &format!("/v1/blocks/{UNKNOWN_HEX}")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let (st, v) = get(&app, "/v1/blocks/not-a-height").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(v["status"], 400);
}

#[tokio::test]
async fn tx_by_id_resolves_inputs_and_outputs() {
    let (_d, app) = app();
    let block = fixture(COINBASE_TX_HEIGHT);
    let tx = &block.txs[0];
    let id = hex::encode(tx.id.0);
    let (st, v) = get(&app, &format!("/v1/txs/{id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["id"], id);
    assert_eq!(v["height"], COINBASE_TX_HEIGHT);
    assert_eq!(v["outputs"].as_array().unwrap().len(), tx.outputs.len());
    assert_eq!(v["inputs"].as_array().unwrap().len(), tx.inputs.len());
    assert!(v["fee"].is_string());
    let out0 = &v["outputs"][0];
    assert!(out0["value"].is_string());
    assert!(out0["ergo_tree"].is_string());
    assert!(out0["address"].is_string());
    assert!(out0["rent"]["maturity_height"].is_number());
    assert!(out0["rent"]["due_nano"].is_string());
    assert!(out0["rent"]["claimable_at_tip"].is_boolean());
    for input in v["inputs"].as_array().unwrap() {
        assert_eq!(input["id"].as_str().unwrap().len(), 64);
    }
}

#[tokio::test]
async fn txs_list_is_newest_first_and_bad_limit_is_400() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/txs?limit=3").await;
    assert_eq!(st, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["height"], 1866002);

    let (st, v) = get(&app, "/v1/txs?limit=0").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(v["status"], 400);
    let (st, _) = get(&app, "/v1/txs?limit=abc").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_tx_is_404_and_bad_hex_is_400() {
    let (_d, app) = app();
    let (st, _) = get(&app, &format!("/v1/txs/{UNKNOWN_HEX}")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, "/v1/txs/zzzz").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn box_by_id_and_rent() {
    let (_d, app) = app();
    let block = fixture(COINBASE_TX_HEIGHT);
    let b = &block.txs[0].outputs[0];
    let id = hex::encode(b.id.0);
    let (st, v) = get(&app, &format!("/v1/boxes/{id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["id"], id);
    assert_eq!(v["value"], b.value.to_string());
    assert_eq!(v["creation_height"], b.creation_height);
    assert!(v["tokens"].is_array());
    assert!(v["registers"].is_object() || v["registers"].is_null());

    let (st, r) = get(&app, &format!("/v1/boxes/{id}/rent")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(r["box_id"], id);
    assert_eq!(
        r["maturity_height"],
        xp_types::rent::maturity_height(b.creation_height)
    );
    assert!(r["due_nano"].is_string());
    assert_eq!(r["claimable_at_tip"], false);

    let (st, _) = get(&app, &format!("/v1/boxes/{UNKNOWN_HEX}")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, "/v1/boxes/nothex").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

fn coinbase_address() -> String {
    // The store's own tree row carries the encoded address for the coinbase output tree.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("y.redb")).unwrap();
    let b0 = fixture(1866000);
    store
        .seed_for_tests(1865999, b0.header.parent_id.0)
        .unwrap();
    store.apply_batch(&[fixture(1866000)], true).unwrap();
    let rd = xp_store::Reader::new(&store).unwrap();
    let tree = fixture(1866000).txs[0].outputs[0].tree_hash.0;
    rd.tree_row(&tree).unwrap().unwrap().address
}

#[tokio::test]
async fn address_summary_and_unknown_address() {
    let (_d, app) = app();
    let addr = coinbase_address();
    let (st, v) = get(&app, &format!("/v1/addresses/{addr}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["address"], addr);
    assert_eq!(v["tree_hash"].as_str().unwrap().len(), 64);
    assert!(v["balance"]["nano"].is_string());
    assert!(v["balance"]["tokens"].is_array());
    assert!(v["box_count"].is_number());
    assert!(v["first_seen"].is_number());
    assert!(v["last_seen"].is_number());

    let (st, _) = get(&app, "/v1/addresses/9notarealaddressatall").await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn address_boxes_pagination_walks_every_box_of_the_tree() {
    let (_d, app) = app();
    let addr = coinbase_address();
    let tree = fixture(1866000).txs[0].outputs[0].tree_hash.0;
    let mut expected: HashSet<String> = HashSet::new();
    for h in [1866000, 1866001, 1866002] {
        for tx in &fixture(h).txs {
            for o in &tx.outputs {
                if o.tree_hash.0 == tree {
                    expected.insert(hex::encode(o.id.0));
                }
            }
        }
    }
    assert!(!expected.is_empty());

    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor: Option<String> = None;
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard < 1000, "pagination did not terminate");
        let path = match &cursor {
            Some(c) => format!("/v1/addresses/{addr}/boxes?limit=1&cursor={c}"),
            None => format!("/v1/addresses/{addr}/boxes?limit=1"),
        };
        let (st, v) = get(&app, &path).await;
        assert_eq!(st, StatusCode::OK);
        for item in v["items"].as_array().unwrap() {
            seen.insert(item["id"].as_str().unwrap().to_string());
        }
        match v["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }
    assert_eq!(seen, expected);

    let (st, v) = get(&app, &format!("/v1/addresses/{addr}/boxes?unspent=true")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(v["items"].as_array().unwrap().len() <= expected.len());
    for item in v["items"].as_array().unwrap() {
        assert!(item["spent_by"].is_null());
    }
}

#[tokio::test]
async fn address_txs_and_rent() {
    let (_d, app) = app();
    let addr = coinbase_address();
    let (st, v) = get(&app, &format!("/v1/addresses/{addr}/txs?limit=2")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(!v["items"].as_array().unwrap().is_empty());
    for tx in v["items"].as_array().unwrap() {
        assert_eq!(tx["id"].as_str().unwrap().len(), 64);
    }

    let (st, v) = get(&app, &format!("/v1/addresses/{addr}/rent")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["truncated"], false);
    let items = v["items"].as_array().unwrap();
    let mut prev = 0u64;
    for it in items {
        let m = it["rent"]["maturity_height"].as_u64().unwrap();
        assert!(m >= prev, "rent items must be sorted by maturity ascending");
        prev = m;
    }
}

#[tokio::test]
async fn richlist_is_sorted_descending_by_nano() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/richlist?limit=3").await;
    assert_eq!(st, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let mut prev = u64::MAX;
    for it in items {
        let n: u64 = it["nano"].as_str().unwrap().parse().unwrap();
        assert!(n <= prev, "richlist must be descending");
        prev = n;
        assert_eq!(it["tree_hash"].as_str().unwrap().len(), 64);
    }
    let (st, _) = get(&app, "/v1/richlist?limit=0").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// The richlist cursor is composite (`"<nano>:<tree hex>"`), so it is the one route cursor
/// that can silently round-trip wrong. Walk it one item at a time and prove the walk
/// recovers exactly the unpaginated set, in order, with no repeats.
#[tokio::test]
async fn richlist_cursor_round_trips_and_walks_the_whole_list() {
    let (_d, app) = app();

    let (st, full) = get(&app, "/v1/richlist?limit=500").await;
    assert_eq!(st, StatusCode::OK);
    let full_items = full["items"].as_array().unwrap().clone();
    assert!(full_items.len() > 1, "need >1 tree to exercise the cursor");
    let full_trees: Vec<String> = full_items
        .iter()
        .map(|i| i["tree_hash"].as_str().unwrap().to_string())
        .collect();

    // First page of one, then the same cursor handed straight back.
    let (st, p1) = get(&app, "/v1/richlist?limit=1").await;
    assert_eq!(st, StatusCode::OK);
    let first = p1["items"][0].clone();
    let cursor = p1["next_cursor"].as_str().unwrap().to_string();
    let (st, p2) = get(&app, &format!("/v1/richlist?limit=1&cursor={cursor}")).await;
    assert_eq!(st, StatusCode::OK);
    let second = p2["items"][0].clone();
    assert_ne!(second["tree_hash"], first["tree_hash"]);
    let n1: u64 = first["nano"].as_str().unwrap().parse().unwrap();
    let n2: u64 = second["nano"].as_str().unwrap().parse().unwrap();
    assert!(n2 <= n1);

    // Full walk with limit=1 must reproduce the unpaginated ordering exactly.
    let mut walked: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard < 1000, "richlist pagination did not terminate");
        let path = match &cursor {
            Some(c) => format!("/v1/richlist?limit=1&cursor={c}"),
            None => "/v1/richlist?limit=1".to_string(),
        };
        let (st, v) = get(&app, &path).await;
        assert_eq!(st, StatusCode::OK);
        for it in v["items"].as_array().unwrap() {
            walked.push(it["tree_hash"].as_str().unwrap().to_string());
        }
        match v["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }
    assert_eq!(walked, full_trees);
    let unique: HashSet<&String> = walked.iter().collect();
    assert_eq!(unique.len(), walked.len(), "cursor walk repeated a tree");

    let (st, _) = get(&app, "/v1/richlist?cursor=notacursor").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rent_upcoming_and_eligible() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/rent/upcoming?blocks=1000000&limit=5").await;
    assert_eq!(st, StatusCode::OK);
    for it in v["items"].as_array().unwrap() {
        assert!(it["maturity_height"].is_number());
        assert_eq!(it["box"]["id"].as_str().unwrap().len(), 64);
    }

    let (st, v) = get(&app, "/v1/rent/eligible?limit=5").await;
    assert_eq!(st, StatusCode::OK);
    // Nothing in the fixture window is claimable at the indexed tip (every fixture output was
    // created at ~1866000, so it matures ~1M blocks later), so the eligible set is empty and
    // the page terminates immediately. The composite cursor itself is round-tripped by the
    // `parse_rent_cursor`/`format_rent_cursor` unit tests in `dto.rs`.
    assert!(v["items"].as_array().unwrap().is_empty());
    assert!(v["next_cursor"].is_null());

    let (st, _) = get(&app, "/v1/rent/eligible?cursor=notacursor").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn search_resolves_height_header_tx_box_and_address() {
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/search?q=1866001").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["kind"], "block");
    assert_eq!(v["id"], "1866001");

    let (_, block) = get(&app, "/v1/blocks/1866001").await;
    let header_id = block["id"].as_str().unwrap().to_string();
    let (st, v) = get(&app, &format!("/v1/search?q={header_id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["kind"], "block");

    let tx_id = hex::encode(fixture(1866000).txs[0].id.0);
    let (st, v) = get(&app, &format!("/v1/search?q={tx_id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["kind"], "tx");

    let box_id = hex::encode(fixture(1866000).txs[0].outputs[0].id.0);
    let (st, v) = get(&app, &format!("/v1/search?q={box_id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["kind"], "box");

    let addr = coinbase_address();
    let (st, v) = get(&app, &format!("/v1/search?q={addr}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["kind"], "address");
    assert_eq!(v["id"], addr);

    let (st, _) = get(&app, &format!("/v1/search?q={UNKNOWN_HEX}")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, "/v1/search?q=%20").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// Fees are real numbers over the API, not the constant 0 the old `value_in - value_out`
/// produced, and every fee output is labelled `kind: "fee"`.
#[tokio::test]
async fn tx_and_block_fees_are_exposed_with_fee_box_kind() {
    let (_d, app) = app();
    let block = fixture(1866000);

    let (st, v) = get(&app, "/v1/blocks/1866000").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["fees"], "26600000");

    // tx 1 pays a single 0.0015 ERG fee; tx 14 pays 0.008 ERG.
    for (index, fee) in [(1usize, "1500000"), (14, "8000000")] {
        let id = hex::encode(block.txs[index].id.0);
        let (st, v) = get(&app, &format!("/v1/txs/{id}")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["fee"], fee, "tx {index}");
        let kinds: Vec<&str> = v["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds.iter().filter(|k| **k == "fee").count(), 1);
        assert!(kinds.iter().all(|k| *k == "fee" || *k == "box"));
    }

    // The block's last tx collects the fees: it creates no fee output, so its own fee is 0.
    let last = hex::encode(block.txs.last().unwrap().id.0);
    let (st, v) = get(&app, &format!("/v1/txs/{last}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["fee"], "0");
    assert_eq!(v["outputs"][0]["kind"], "box");
    assert_eq!(v["outputs"][0]["value"], "26600000");
    // Its inputs are exactly the block's fee boxes.
    for inp in v["inputs"].as_array().unwrap() {
        assert_eq!(inp["box"]["kind"], "fee");
    }
}

// ---------------------------------------------------------------------------------------
// Schema-v2 routes: tokens, templates, register search, search kinds, token-name enrichment.
// ---------------------------------------------------------------------------------------

/// The SigUSD token id — equal to the id of its mint transaction's first input box.
const SIGUSD: &str = "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04";

fn hash32(hex_str: &str) -> xp_types::Hash32 {
    let mut h = [0u8; 32];
    h.copy_from_slice(&hex::decode(hex_str).unwrap());
    h
}

fn router_over(store: Store, indexed: u32) -> Router {
    let (_tx, rx) = watch::channel(IngestStatus {
        indexed: Some(indexed),
        best: indexed,
        mode: Mode::Tip,
        source: "test".into(),
        halted: None,
        stalled: None,
    });
    xp_api::router(xp_api::AppState {
        store: Arc::new(store),
        status: rx,
    })
}

/// A store seeded just below the SigUSD mint block, with 453051 applied — one token, one
/// holder, and four token ids carried by boxes but minted before the seed height.
fn app_sigusd() -> (tempfile::TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("t.redb")).unwrap();
    let b = fixture(453051);
    store.seed_for_tests(453050, b.header.parent_id.0).unwrap();
    store.apply_batch(&[b], true).unwrap();
    (dir, router_over(store, 453051))
}

/// 453051 plus a synthetic block that spends the SigUSD mint box onto two distinct trees
/// (so SigUSD ends with two holders) while minting a second token on one of them (one
/// holder). Gives both token listings more than one row, with distinct holder counts.
///
/// Returns the router and the second token's id.
fn app_two_tokens() -> (tempfile::TempDir, Router, String) {
    use xp_types::{BoxId, HeaderId, TxId};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("t.redb")).unwrap();
    let b = fixture(453051);
    store.seed_for_tests(453050, b.header.parent_id.0).unwrap();
    store.apply_batch(&[fixture(453051)], true).unwrap();

    let mint_box = b.txs[1].outputs[0].clone();
    // Ergo's mint rule: the minted token's id is the id of the tx's first input box.
    let new_token = mint_box.id.0;
    // Two prototypes on distinct trees, taken from the fixture so their trees are indexed.
    let mut protos: Vec<xp_wire::DecodedBox> = Vec::new();
    for tx in &b.txs {
        for o in &tx.outputs {
            if !protos.iter().any(|p| p.tree_hash.0 == o.tree_hash.0) {
                protos.push(o.clone());
            }
        }
    }
    assert!(protos.len() >= 2);

    let mut o0 = protos[0].clone();
    o0.id = BoxId([0xE0u8; 32]);
    o0.tx_id = TxId([0xDDu8; 32]);
    o0.index = 0;
    o0.value = 1_000_000;
    o0.tokens = vec![(hash32(SIGUSD), 3), (new_token, 7)];
    let mut o1 = protos[1].clone();
    o1.id = BoxId([0xE1u8; 32]);
    o1.tx_id = TxId([0xDDu8; 32]);
    o1.index = 1;
    o1.value = 1_000_000;
    o1.tokens = vec![(hash32(SIGUSD), 2)];

    let mut b2 = fixture(453051);
    b2.header.height = 453052;
    b2.header.parent_id = b.header.id;
    b2.header.id = HeaderId([0xCAu8; 32]);
    b2.txs = vec![xp_wire::DecodedTx {
        id: TxId([0xDDu8; 32]),
        inputs: vec![mint_box.id],
        data_inputs: vec![],
        outputs: vec![o0, o1],
        size: 100,
    }];
    store.apply_batch(&[b2], true).unwrap();
    (dir, router_over(store, 453052), hex::encode(new_token))
}

/// Walks a paged route one item at a time and returns the values of `field` in visit order.
async fn walk(app: &Router, base: &str, field: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard < 1000, "pagination did not terminate: {base}");
        let sep = if base.contains('?') { '&' } else { '?' };
        let path = match &cursor {
            Some(c) => format!("{base}{sep}limit=1&cursor={c}"),
            None => format!("{base}{sep}limit=1"),
        };
        let (st, v) = get(app, &path).await;
        assert_eq!(st, StatusCode::OK, "{path}");
        for it in v["items"].as_array().unwrap() {
            out.push(it[field].as_str().unwrap().to_string());
        }
        match v["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }
    out
}

#[tokio::test]
async fn token_by_id_exposes_the_mint_row_and_404s_on_an_unknown_id() {
    let (_d, app) = app_sigusd();
    let (st, v) = get(&app, &format!("/v1/tokens/{SIGUSD}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["id"], SIGUSD);
    assert_eq!(v["name"], "SigUSD");
    assert_eq!(v["decimals"], 2);
    // SigUSD carries no EIP-4 R7, so it is a plain token.
    assert_eq!(v["kind"], "token");
    assert_eq!(v["mint_height"], 453051);
    assert_eq!(v["mint_tx"].as_str().unwrap().len(), 64);
    assert_eq!(v["mint_box"].as_str().unwrap().len(), 64);
    assert_eq!(v["holder_count"], 1);
    assert!(v["box_count"].is_number());
    // Amounts are decimal strings, and supply is emission minus burned.
    let emission: u64 = v["emission"].as_str().unwrap().parse().unwrap();
    let burned: u64 = v["burned"].as_str().unwrap().parse().unwrap();
    let supply: u64 = v["supply"].as_str().unwrap().parse().unwrap();
    assert_eq!(emission, 10_000_000_000_001);
    assert_eq!(supply, emission - burned);

    let (st, _) = get(&app, &format!("/v1/tokens/{UNKNOWN_HEX}")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, "/v1/tokens/nothex").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn tokens_list_defaults_to_newest_and_rejects_an_unknown_sort() {
    let (_d, app, new_token) = app_two_tokens();

    let (st, v) = get(&app, "/v1/tokens").await;
    assert_eq!(st, StatusCode::OK);
    let ids: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    // The synthetic mint is newer, so it leads the default (newest-first) ordering.
    assert_eq!(ids, vec![new_token.as_str(), SIGUSD]);
    let (_, explicit) = get(&app, "/v1/tokens?sort=newest").await;
    assert_eq!(explicit["items"], v["items"]);

    let (st, v) = get(&app, "/v1/tokens?sort=sideways").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(v["status"], 400);
    let (st, _) = get(&app, "/v1/tokens?limit=0").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// `sort=holders` uses a composite `"<count>:<token id>"` cursor, so walk it one item at a
/// time and prove the walk reproduces the unpaginated ordering with no repeats.
#[tokio::test]
async fn tokens_by_holders_cursor_round_trips() {
    let (_d, app, new_token) = app_two_tokens();

    let (st, full) = get(&app, "/v1/tokens?sort=holders&limit=500").await;
    assert_eq!(st, StatusCode::OK);
    let expected: Vec<String> = full["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    // SigUSD ends on two trees, the synthetic mint on one.
    assert_eq!(expected, vec![SIGUSD.to_string(), new_token.clone()]);
    assert_eq!(full["items"][0]["holder_count"], 2);
    assert_eq!(full["items"][1]["holder_count"], 1);

    let (st, p1) = get(&app, "/v1/tokens?sort=holders&limit=1").await;
    assert_eq!(st, StatusCode::OK);
    let cursor = p1["next_cursor"].as_str().unwrap().to_string();
    assert_eq!(cursor, format!("2:{SIGUSD}"));

    let walked = walk(&app, "/v1/tokens?sort=holders", "id").await;
    assert_eq!(walked, expected);
    let unique: HashSet<&String> = walked.iter().collect();
    assert_eq!(unique.len(), walked.len(), "cursor walk repeated a token");

    let (st, _) = get(&app, "/v1/tokens?sort=holders&cursor=notacursor").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn token_holders_are_ordered_by_amount_with_shares_and_a_cursor() {
    let (_d, app, _) = app_two_tokens();

    let (st, v) = get(&app, &format!("/v1/tokens/{SIGUSD}/holders?limit=500")).await;
    assert_eq!(st, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let mut prev = u64::MAX;
    for it in items {
        let amount: u64 = it["amount"].as_str().unwrap().parse().unwrap();
        assert!(amount <= prev, "holders must be descending by amount");
        prev = amount;
        assert_eq!(it["tree_hash"].as_str().unwrap().len(), 64);
        // Two units out of a 10^13 supply round to 0.00, and never to a bare "0".
        let share = it["share_pct"].as_str().unwrap();
        assert!(share.contains('.'), "share_pct has two decimals: {share}");
        assert_eq!(share.split('.').nth(1).unwrap().len(), 2);
    }
    assert_eq!(items[0]["amount"], "3");
    assert_eq!(items[1]["amount"], "2");

    // The cursor is "<amount>:<tree hex>" and a limit-1 walk reproduces the whole list.
    let (st, p1) = get(&app, &format!("/v1/tokens/{SIGUSD}/holders?limit=1")).await;
    assert_eq!(st, StatusCode::OK);
    let cursor = p1["next_cursor"].as_str().unwrap().to_string();
    assert_eq!(
        cursor,
        format!("3:{}", items[0]["tree_hash"].as_str().unwrap())
    );
    let walked = walk(&app, &format!("/v1/tokens/{SIGUSD}/holders"), "amount").await;
    assert_eq!(walked, vec!["3".to_string(), "2".to_string()]);

    let (st, _) = get(&app, &format!("/v1/tokens/{UNKNOWN_HEX}/holders")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, &format!("/v1/tokens/{SIGUSD}/holders?cursor=nope")).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// A share of exactly one third of the supply renders as `33.33`, proving the two-decimal
/// basis-point rendering rather than an integer percent.
#[tokio::test]
async fn share_pct_renders_two_decimals() {
    assert_eq!(xp_api::dto::share_pct(1, 3), "33.33");
    assert_eq!(xp_api::dto::share_pct(1, 1), "100.00");
    assert_eq!(xp_api::dto::share_pct(1234, 10_000), "12.34");
    assert_eq!(xp_api::dto::share_pct(0, 100), "0.00");
    // A fully burned token has no meaningful share, and must not divide by zero.
    assert_eq!(xp_api::dto::share_pct(5, 0), "0.00");
    // `amount * 10_000` overflows u64 here; the u128 path keeps it exact.
    assert_eq!(xp_api::dto::share_pct(u64::MAX, u64::MAX), "100.00");
}

#[tokio::test]
async fn token_boxes_filters_unspent_and_404s_for_an_unknown_token() {
    let (_d, app, _) = app_two_tokens();

    let (st, all) = get(&app, &format!("/v1/tokens/{SIGUSD}/boxes?limit=500")).await;
    assert_eq!(st, StatusCode::OK);
    let all_items = all["items"].as_array().unwrap();
    // The mint box (now spent) plus the two synthetic outputs.
    assert_eq!(all_items.len(), 3);
    for it in all_items {
        assert!(it["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == SIGUSD));
    }

    let (st, unspent) = get(
        &app,
        &format!("/v1/tokens/{SIGUSD}/boxes?unspent=true&limit=500"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let unspent_items = unspent["items"].as_array().unwrap();
    assert_eq!(unspent_items.len(), 2);
    for it in unspent_items {
        assert!(it["spent_by"].is_null());
    }

    // A limit-1 walk visits every box exactly once.
    let walked = walk(&app, &format!("/v1/tokens/{SIGUSD}/boxes"), "id").await;
    assert_eq!(walked.len(), 3);
    assert_eq!(walked.iter().collect::<HashSet<_>>().len(), 3);

    let (st, _) = get(&app, &format!("/v1/tokens/{UNKNOWN_HEX}/boxes")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, &format!("/v1/tokens/{SIGUSD}/boxes?unspent=maybe")).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// The template hash carried by the most fixture boxes, and the address of its example tree.
fn busiest_template() -> String {
    let mut counts: std::collections::HashMap<xp_types::Hash32, usize> = Default::default();
    for h in [1866000, 1866001, 1866002] {
        for tx in &fixture(h).txs {
            for o in &tx.outputs {
                if let Ok(t) = xp_wire::template_hash_of(&o.tree_bytes) {
                    *counts.entry(t).or_default() += 1;
                }
            }
        }
    }
    let best = counts
        .iter()
        .max_by_key(|(h, n)| (**n, **h))
        .expect("some template")
        .0;
    hex::encode(best)
}

#[tokio::test]
async fn template_and_its_boxes() {
    let (_d, app) = app();
    let tmpl = busiest_template();

    let (st, v) = get(&app, &format!("/v1/templates/{tmpl}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["hash"], tmpl);
    let box_count = v["box_count"].as_u64().unwrap();
    let unspent_count = v["unspent_count"].as_u64().unwrap();
    assert!(box_count > 0);
    assert!(unspent_count <= box_count);
    assert!(v["first_seen"].is_number());
    assert!(
        v["example_address"].is_string(),
        "the example tree is indexed, so its address resolves"
    );

    let (st, all) = get(&app, &format!("/v1/templates/{tmpl}/boxes?limit=500")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(all["items"].as_array().unwrap().len() as u64, box_count);
    let (st, unspent) = get(
        &app,
        &format!("/v1/templates/{tmpl}/boxes?unspent=true&limit=500"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let unspent_items = unspent["items"].as_array().unwrap();
    assert_eq!(unspent_items.len() as u64, unspent_count);
    for it in unspent_items {
        assert!(it["spent_by"].is_null());
    }

    // Every returned box really is on this template.
    for it in all["items"].as_array().unwrap() {
        assert_eq!(it["template_hash"], tmpl);
    }

    let (st, _) = get(&app, &format!("/v1/templates/{UNKNOWN_HEX}")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, &format!("/v1/templates/{UNKNOWN_HEX}/boxes")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&app, "/v1/templates/nothex").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// An R4 value taken from a fixture output, with the box that carries it.
fn fixture_r4() -> (String, String) {
    for h in [1866000, 1866001, 1866002] {
        for tx in &fixture(h).txs {
            for o in &tx.outputs {
                if let Some(after) = o.registers_json.split_once("\"R4\":\"") {
                    if let Some(end) = after.1.find('"') {
                        return (after.1[..end].to_string(), hex::encode(o.id.0));
                    }
                }
            }
        }
    }
    panic!("no fixture output carries an R4");
}

#[tokio::test]
async fn register_search_finds_the_box_and_validates_its_path() {
    let (_d, app) = app();
    let (value, box_id) = fixture_r4();

    let (st, v) = get(&app, &format!("/v1/registers/R4/{value}/boxes?limit=500")).await;
    assert_eq!(st, StatusCode::OK);
    let ids: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&box_id.as_str()),
        "register index missed {box_id}"
    );
    // The register letter is case-insensitive.
    let (st, lower) = get(&app, &format!("/v1/registers/r4/{value}/boxes?limit=500")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(lower["items"], v["items"]);

    // The same value under a register no box holds it in is an empty page, not a 404 —
    // nothing asserts a register value ever existed.
    let (st, other) = get(&app, &format!("/v1/registers/R9/{value}/boxes")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(other["items"].as_array().unwrap().is_empty());
    let (st, none) = get(&app, "/v1/registers/R4/0e0400/boxes").await;
    assert_eq!(st, StatusCode::OK);
    assert!(none["items"].as_array().unwrap().is_empty());

    // R0..R3 are never indexed and are not addressable; nor is a bare digit or odd hex.
    for bad in ["R3", "R10", "RX", "4", "R"] {
        let (st, _) = get(&app, &format!("/v1/registers/{bad}/{value}/boxes")).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "reg {bad:?}");
    }
    for bad in ["abc", "zz", "0e04zz"] {
        let (st, body) = get(&app, &format!("/v1/registers/R4/{bad}/boxes")).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "value {bad:?}");
        assert_eq!(body["status"], 400);
    }
}

#[tokio::test]
async fn search_resolves_a_token_id_and_a_template_hash() {
    let (_d, sigusd_app) = app_sigusd();
    let (st, v) = get(&sigusd_app, &format!("/v1/search?q={SIGUSD}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["kind"], "token");
    assert_eq!(v["id"], SIGUSD);

    let (_d2, app2) = app();
    let tmpl = busiest_template();
    let (st, v) = get(&app2, &format!("/v1/search?q={tmpl}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["kind"], "template");
    assert_eq!(v["id"], tmpl);
}

/// `tx_count` is the store's own counter, so prove it against an independent walk of the
/// address's transaction page.
#[tokio::test]
async fn address_tx_count_matches_a_walk_of_its_txs() {
    let (_d, app) = app();
    let addr = coinbase_address();
    let (st, v) = get(&app, &format!("/v1/addresses/{addr}")).await;
    assert_eq!(st, StatusCode::OK);
    let claimed = v["tx_count"].as_u64().expect("tx_count is a number");

    let walked = walk(&app, &format!("/v1/addresses/{addr}/txs"), "id").await;
    let unique: HashSet<&String> = walked.iter().collect();
    assert_eq!(unique.len(), walked.len(), "tx walk repeated a tx");
    assert!(claimed > 0);
    assert_eq!(claimed, walked.len() as u64);
}

/// Box token lists carry the token's name and decimals when the store indexed its mint, and
/// `null` for a token minted before the store's seed height.
#[tokio::test]
async fn box_tokens_carry_names_when_the_mint_is_indexed() {
    let (_d, app) = app_sigusd();
    let b = fixture(453051);

    let mint_box = hex::encode(b.txs[1].outputs[0].id.0);
    let (st, v) = get(&app, &format!("/v1/boxes/{mint_box}")).await;
    assert_eq!(st, StatusCode::OK);
    let named = v["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == SIGUSD)
        .expect("the mint box holds SigUSD");
    assert_eq!(named["name"], "SigUSD");
    assert_eq!(named["decimals"], 2);
    assert_eq!(named["amount"], "10000000000001");

    // A box holding a token whose mint predates the seed height reports null, not "".
    let (unknown_box, unknown_token) = b
        .txs
        .iter()
        .flat_map(|t| t.outputs.iter())
        .find_map(|o| {
            o.tokens
                .iter()
                .find(|(id, _)| hex::encode(id) != SIGUSD)
                .map(|(id, _)| (hex::encode(o.id.0), hex::encode(id)))
        })
        .expect("a fixture box carries a pre-existing token");
    let (st, v) = get(&app, &format!("/v1/boxes/{unknown_box}")).await;
    assert_eq!(st, StatusCode::OK);
    let anon = v["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == unknown_token)
        .unwrap();
    assert!(
        anon["name"].is_null(),
        "unminted token must have a null name"
    );
    assert!(anon["decimals"].is_null());

    // The same enrichment reaches tx outputs, not just the standalone box route.
    let tx_id = hex::encode(b.txs[1].id.0);
    let (st, v) = get(&app, &format!("/v1/txs/{tx_id}")).await;
    assert_eq!(st, StatusCode::OK);
    let out_named = v["outputs"][0]["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == SIGUSD)
        .expect("the mint output holds SigUSD");
    assert_eq!(out_named["name"], "SigUSD");
}
