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
