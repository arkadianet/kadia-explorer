//! Route-level tests: drive the real `axum::Router` returned by `xp_api::router` with
//! `tower::ServiceExt::oneshot` over a store holding the three block fixtures.

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::sync::Semaphore;
use tower::ServiceExt;
use xp_api::{ApiConfig, Counters};
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
    app_with(unlimited(), stall)
}

/// The config the non-rate-limit tests run under: limiting off (`per_second = 0`), since
/// several of them walk a cursor over far more than a default burst of requests and every
/// one of them arrives without `ConnectInfo`, i.e. under a single client key.
fn unlimited() -> ApiConfig {
    ApiConfig {
        per_second: 0,
        ..ApiConfig::default()
    }
}

/// Same again, under an explicit [`ApiConfig`] (rate-limit knobs).
fn app_with(cfg: ApiConfig, stall: Option<StalledInfo>) -> (tempfile::TempDir, Router) {
    let (dir, router, _state) = app_with_state(cfg, stall);
    (dir, router)
}

/// As [`app_with`], but also returns the [`xp_api::AppState`] so a test can reach into it
/// (e.g. to hold a read permit from outside the router, as a parked reader would).
fn app_with_state(
    cfg: ApiConfig,
    stall: Option<StalledInfo>,
) -> (tempfile::TempDir, Router, xp_api::AppState) {
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
        source_observed_at_ms: None,
        source_error: None,
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
        counters: Arc::new(Counters::default()),
        read_permits: Arc::new(Semaphore::new(cfg.max_inflight_reads as usize)),
    };
    let router = xp_api::router(state.clone(), &cfg);
    (dir, router, state)
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

/// GET as a given peer, optionally with `X-Forwarded-For`.
async fn get_from(
    app: &Router,
    path: &str,
    peer: &str,
    xff: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    get_from_lines(app, path, peer, xff.as_slice()).await
}

/// As [`get_from`], but sends one `X-Forwarded-For` header *line* per entry of `xff`.
async fn get_from_lines(
    app: &Router,
    path: &str,
    peer: &str,
    xff: &[&str],
) -> (StatusCode, HeaderMap, Value) {
    let peer: SocketAddr = format!("{peer}:4000").parse().unwrap();
    let mut builder = Request::builder().uri(path);
    for x in xff {
        builder = builder.header("x-forwarded-for", *x);
    }
    let mut req = builder.body(Body::empty()).unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, headers, v)
}

fn limited(per_second: u32, burst: u32, allow: &[&str]) -> ApiConfig {
    ApiConfig {
        per_second,
        burst,
        allowlist: xp_api::limit::Allowlist::parse(
            &allow.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap(),
        ..ApiConfig::default()
    }
}

#[tokio::test]
async fn rate_limit_returns_429_after_burst_with_retry_after() {
    let (_d, app) = app_with(limited(1, 2, &[]), None);
    for _ in 0..2 {
        let (st, _, _) = get_from(&app, "/v1/status", "198.51.100.1", None).await;
        assert_eq!(st, StatusCode::OK);
    }
    let (st, h, v) = get_from(&app, "/v1/status", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(h.get("retry-after").unwrap(), "1");
    assert_eq!(v["status"], 429);
    assert_eq!(v["title"], "Too Many Requests");
    // CORS still applies to a rejected request: a browser must be able to read the 429.
    assert!(h.contains_key("access-control-allow-origin"));
    // A different client is unaffected.
    let (st, _, _) = get_from(&app, "/v1/status", "198.51.100.2", None).await;
    assert_eq!(st, StatusCode::OK);
    // The counter moved.
    let (_, _, s) = get_from(&app, "/v1/status", "198.51.100.3", None).await;
    assert_eq!(s["rate_limited_total"], 1);
    assert_eq!(s["inflight_reads"], 0);
}

#[tokio::test]
async fn allowlisted_clients_are_never_limited() {
    let (_d, app) = app_with(limited(1, 1, &["198.51.100.0/24"]), None);
    for _ in 0..5 {
        let (st, _, _) = get_from(&app, "/v1/status", "198.51.100.7", None).await;
        assert_eq!(st, StatusCode::OK);
    }
}

#[tokio::test]
async fn forwarded_for_is_honoured_only_from_a_trusted_proxy() {
    let (_d, app) = app_with(limited(1, 1, &[]), None);
    // Behind the trusted proxy (127.0.0.1), two distinct forwarded clients each get one.
    assert_eq!(
        get_from(&app, "/v1/status", "127.0.0.1", Some("1.1.1.1"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        get_from(&app, "/v1/status", "127.0.0.1", Some("2.2.2.2"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        get_from(&app, "/v1/status", "127.0.0.1", Some("1.1.1.1"))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // From an untrusted peer the header is ignored: the peer itself is the key.
    assert_eq!(
        get_from(&app, "/v1/status", "203.0.113.5", Some("3.3.3.3"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        get_from(&app, "/v1/status", "203.0.113.5", Some("4.4.4.4"))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// A client that sends its own `X-Forwarded-For` gets a *second* header line appended by the
/// proxy; only the last entry across all lines is the proxy's word, so reading just the first
/// line would let the client pick its own rate-limit key.
#[tokio::test]
async fn every_forwarded_for_line_is_considered_not_just_the_first() {
    let (_d, app) = app_with(limited(1, 1, &[]), None);
    // Spoofed first line, proxy-appended second line: the key must be 1.1.1.1.
    assert_eq!(
        get_from_lines(&app, "/v1/status", "127.0.0.1", &["6.6.6.6", "1.1.1.1"])
            .await
            .0,
        StatusCode::OK
    );
    // A different *last* hop is a different client: still allowed.
    assert_eq!(
        get_from_lines(&app, "/v1/status", "127.0.0.1", &["6.6.6.6", "2.2.2.2"])
            .await
            .0,
        StatusCode::OK
    );
    // The same last hop again is the same client: limited.
    assert_eq!(
        get_from_lines(&app, "/v1/status", "127.0.0.1", &["6.6.6.6", "1.1.1.1"])
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn zero_rate_disables_limiting() {
    let (_d, app) = app_with(limited(0, 1, &[]), None);
    for _ in 0..10 {
        assert_eq!(
            get_from(&app, "/v1/status", "198.51.100.1", None).await.0,
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn requests_without_connect_info_are_keyed_as_unspecified_and_still_limited() {
    // `oneshot` without ConnectInfo (the existing `get` helper) must not panic.
    let (_d, app) = app_with(limited(1, 1, &[]), None);
    let (st, _) = get(&app, "/v1/status").await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = get(&app, "/v1/status").await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
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
        source_observed_at_ms: None,
        source_error: None,
        indexed: Some(indexed),
        best: indexed,
        mode: Mode::Tip,
        source: "test".into(),
        halted: None,
        stalled: None,
    });
    let cfg = unlimited();
    xp_api::router(
        xp_api::AppState {
            store: Arc::new(store),
            status: rx,
            counters: Arc::new(Counters::default()),
            read_permits: Arc::new(Semaphore::new(cfg.max_inflight_reads as usize)),
        },
        &cfg,
    )
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
        // Both holders own well over 0.01% of the circulating supply, so both take the
        // two-decimal band — and neither is the bare "0" reserved for an empty holder.
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
/// basis-point rendering rather than an integer percent — and a dust holder keeps enough
/// precision to stay visibly non-zero.
#[tokio::test]
async fn share_pct_renders_two_decimals_and_keeps_dust_visible() {
    assert_eq!(xp_api::dto::share_pct(1, 3), "33.33");
    assert_eq!(xp_api::dto::share_pct(1, 1), "100.00");
    assert_eq!(xp_api::dto::share_pct(1234, 10_000), "12.34");
    // The two-decimal band reaches down to exactly 0.01%.
    assert_eq!(xp_api::dto::share_pct(1, 10_000), "0.01");

    // Below 0.01% two decimals would say "0.00" — a holder of something rendered as a
    // holder of nothing. Up to four decimals, trailing zeros trimmed.
    assert_eq!(xp_api::dto::share_pct(7, 1_000_000), "0.0007");
    assert_eq!(xp_api::dto::share_pct(1, 100_000), "0.001");
    assert_eq!(xp_api::dto::share_pct(99, 1_000_000), "0.0099");
    // Smaller than four decimals can express: floored to the smallest non-zero rendering,
    // never to "0".
    assert_eq!(xp_api::dto::share_pct(1, 10_000_000_000_000), "0.0001");

    // Only an empty holder and a fully burned token render a bare "0" — the latter must
    // not divide by zero.
    assert_eq!(xp_api::dto::share_pct(0, 100), "0");
    assert_eq!(xp_api::dto::share_pct(5, 0), "0");
    // `amount * 1_000_000` overflows u64 here; the u128 path keeps it exact.
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

/// The burn path: the synthetic block spends the whole 10^13 SigUSD mint into outputs that
/// keep only 5 units, so `burned` is non-zero and `supply` is *not* `emission`. Without this
/// the `supply = emission - burned` rule is indistinguishable from `supply = emission`.
#[tokio::test]
async fn supply_is_emission_minus_burned_and_shares_use_it() {
    let (_d, app, _) = app_two_tokens();
    let (st, v) = get(&app, &format!("/v1/tokens/{SIGUSD}")).await;
    assert_eq!(st, StatusCode::OK);

    // The mint emitted 10_000_000_000_001 units; the spend rewrote only 3 + 2 into outputs.
    assert_eq!(v["emission"], "10000000000001");
    assert_eq!(v["burned"], "9999999999996");
    assert_eq!(v["supply"], "5");
    let emission: u64 = v["emission"].as_str().unwrap().parse().unwrap();
    let burned: u64 = v["burned"].as_str().unwrap().parse().unwrap();
    assert!(burned > 0, "the burn must actually be exercised");
    assert_ne!(
        v["supply"].as_str().unwrap(),
        v["emission"].as_str().unwrap(),
        "supply must differ from emission once units are burned"
    );
    assert_eq!(v["supply"], (emission - burned).to_string());
    // SigUSD declares no EIP-4 R7, so the raw tag is null and the kind falls back.
    assert!(v["token_type"].is_null());
    assert_eq!(v["kind"], "token");

    // Shares are of the *circulating* supply (5), not of the emission: 3/5 and 2/5. Against
    // emission both would round to "0.00", so this pins the denominator.
    let (st, h) = get(&app, &format!("/v1/tokens/{SIGUSD}/holders?limit=10")).await;
    assert_eq!(st, StatusCode::OK);
    let items = h["items"].as_array().unwrap();
    assert_eq!(items[0]["amount"], "3");
    assert_eq!(items[0]["share_pct"], "60.00");
    assert_eq!(items[1]["amount"], "2");
    assert_eq!(items[1]["share_pct"], "40.00");
}

/// `/v1/tokens/{id}/holders` has one fixed ordering, so it takes no `dir` — and, like every
/// other route, an unknown query parameter is ignored rather than rejected.
#[tokio::test]
async fn holders_and_tokens_ignore_unknown_query_params_alike() {
    let (_d, app, _) = app_two_tokens();

    let (st, plain) = get(&app, &format!("/v1/tokens/{SIGUSD}/holders?limit=10")).await;
    assert_eq!(st, StatusCode::OK);
    let (st, with_dir) = get(
        &app,
        &format!("/v1/tokens/{SIGUSD}/holders?limit=10&dir=asc"),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "holders has no dir to reject");
    assert_eq!(with_dir["items"], plain["items"], "dir must not reorder");

    // Same treatment on /v1/tokens, which also has no dir.
    let (st, tokens) = get(&app, "/v1/tokens?limit=10").await;
    assert_eq!(st, StatusCode::OK);
    let (st, tokens_dir) = get(&app, "/v1/tokens?limit=10&dir=asc").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(tokens_dir["items"], tokens["items"]);

    // A route that *does* take dir still honours it, so this is not blanket param-blindness.
    let (st, asc) = get(
        &app,
        &format!("/v1/tokens/{SIGUSD}/boxes?dir=asc&limit=500"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, desc) = get(
        &app,
        &format!("/v1/tokens/{SIGUSD}/boxes?dir=desc&limit=500"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let ids = |v: &Value| -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect()
    };
    let (mut a, d) = (ids(&asc), ids(&desc));
    a.reverse();
    assert_eq!(a, d, "dir=asc/desc must be exact reverses");
}

/// A contract (non-P2PK) tree encodes as a mainnet **P2S address**, not as `null`: the
/// `Option` on `example_address`/`address` covers a missing tree row, which the store's
/// invariants rule out, so neither field is null for anything reachable over the API.
#[tokio::test]
async fn contract_trees_render_as_p2s_addresses_not_null() {
    let (_d, app) = app();

    // Find a template whose example tree is not P2PK (mainnet P2PK addresses start with '9'
    // and are 51 chars; a P2S address encodes the whole script and is far longer).
    let mut checked = 0;
    for h in [1866000u32, 1866001, 1866002] {
        for tx in &fixture(h).txs {
            for o in &tx.outputs {
                let Ok(t) = xp_wire::template_hash_of(&o.tree_bytes) else {
                    continue;
                };
                let (st, v) = get(&app, &format!("/v1/templates/{}", hex::encode(t))).await;
                assert_eq!(st, StatusCode::OK);
                let addr = v["example_address"]
                    .as_str()
                    .expect("example_address is never null for an indexed tree");
                if !addr.starts_with('9') {
                    assert!(
                        addr.len() > 51,
                        "a P2S address encodes the script, so it is long: {addr}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0, "no contract template in the fixture set");

    // The same holds for a token holder sitting on a contract tree.
    let (_d2, tokens_app, _) = app_two_tokens();
    let (st, h) = get(
        &tokens_app,
        &format!("/v1/tokens/{SIGUSD}/holders?limit=10"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let holders = h["items"].as_array().unwrap();
    let contract = holders
        .iter()
        .find(|it| !it["address"].as_str().unwrap().starts_with('9'))
        .expect("SigUSD sits on a contract tree in the fixture");
    assert!(contract["address"].is_string(), "never null, always P2S");
    assert!(contract["address"].as_str().unwrap().len() > 51);
    for it in holders {
        assert!(
            !it["address"].is_null(),
            "every indexed holder has an address"
        );
    }
}

#[tokio::test]
async fn address_txs_are_summaries_without_resolved_boxes() {
    let (_d, app) = app();
    let addr = coinbase_address();
    let (st, v) = get(&app, &format!("/v1/addresses/{addr}/txs?limit=5")).await;
    assert_eq!(st, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let first = &items[0];
    for key in [
        "id",
        "height",
        "index",
        "timestamp",
        "size",
        "fee",
        "input_count",
        "data_input_count",
        "output_count",
    ] {
        assert!(first.get(key).is_some(), "missing {key}");
    }
    assert!(
        first.get("inputs").is_none(),
        "summaries must not resolve inputs"
    );
    assert!(
        first.get("outputs").is_none(),
        "summaries must not resolve outputs"
    );
    assert!(first["fee"].is_string());
    // Cross-check one row against the full tx endpoint.
    let id = first["id"].as_str().unwrap();
    let (_, full) = get(&app, &format!("/v1/txs/{id}")).await;
    assert_eq!(full["height"], first["height"]);
    assert_eq!(full["fee"], first["fee"]);
    assert_eq!(
        full["inputs"].as_array().unwrap().len() as u64,
        first["input_count"].as_u64().unwrap()
    );
    assert_eq!(
        full["outputs"].as_array().unwrap().len() as u64,
        first["output_count"].as_u64().unwrap()
    );
}

#[tokio::test]
async fn reads_beyond_the_permit_budget_fail_fast_with_503() {
    let cfg = ApiConfig {
        max_inflight_reads: 1,
        per_second: 0,
        ..ApiConfig::default()
    };
    let (_d, app, state) = app_with_state(cfg, None);
    // Occupy the single permit from outside the router, exactly as a parked reader would.
    let held = state.read_permits.clone().try_acquire_owned().unwrap();
    let (st, h, v) = get_from(&app, "/v1/blocks?limit=1", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(h.get("retry-after").unwrap(), "1");
    assert_eq!(v["title"], "Service Unavailable");
    drop(held);
    let (st, _, _) = get_from(&app, "/v1/blocks?limit=1", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::OK);
    // /v1/status does not take a permit (it reads the watch channel, not the store).
    let (st, _, s) = get_from(&app, "/v1/status", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["inflight_reads"], 0);
}

#[tokio::test]
async fn ipv6_clients_share_a_bucket_per_64_but_allowlist_matches_the_full_address() {
    let (_d, app) = app_with(limited(1, 1, &["2001:db8:1:2::9/128"]), None);
    // Two addresses in one /64 share the single-token bucket.
    assert_eq!(
        get_from(&app, "/v1/status", "[2001:db8:1:2::1]", None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        get_from(&app, "/v1/status", "[2001:db8:1:2::2]", None)
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // A different /64 gets its own bucket.
    assert_eq!(
        get_from(&app, "/v1/status", "[2001:db8:1:3::1]", None)
            .await
            .0,
        StatusCode::OK
    );
    // The /128 allowlist entry still matches its exact client inside the exhausted /64.
    for _ in 0..3 {
        assert_eq!(
            get_from(&app, "/v1/status", "[2001:db8:1:2::9]", None)
                .await
                .0,
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn search_shared_mint_id_returns_box_and_token() {
    let (_d, app, id) = app_two_tokens();
    assert_eq!(get(&app, &format!("/v1/boxes/{id}")).await.0, 200);
    assert_eq!(get(&app, &format!("/v1/tokens/{id}")).await.0, 200);
    let (status, result) = get(&app, &format!("/v1/search?q={id}")).await;
    assert_eq!(status, 200);
    assert_eq!(result["kind"], "box");
    assert_eq!(result["id"], id);
    assert_eq!(
        result["matches"],
        serde_json::json!([
            {"kind": "box", "id": id}, {"kind": "token", "id": id}
        ])
    );
}

#[tokio::test]
async fn upcoming_rent_reports_truncation_and_exact_cap() {
    let (_d, app) = app();
    let (_, all) = get(&app, "/v1/rent/upcoming?blocks=2000000&limit=500").await;
    let count = all["items"].as_array().unwrap().len();
    assert!(count > 1 && count < 500);
    assert_eq!(all["complete"], true);
    let (_, capped) = get(&app, "/v1/rent/upcoming?blocks=2000000&limit=1").await;
    assert_eq!(capped["complete"], false);
    let (_, exact) = get(
        &app,
        &format!("/v1/rent/upcoming?blocks=2000000&limit={count}"),
    )
    .await;
    assert_eq!(exact["complete"], true);
}

#[tokio::test]
async fn upcoming_501_boxes_explicitly_marks_500_as_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("rent.redb")).unwrap();
    let mut block = fixture(1866000);
    store
        .seed_for_tests(1865999, block.header.parent_id.0)
        .unwrap();
    block.txs.truncate(1);
    let output = block.txs[0].outputs[0].clone();
    block.txs[0].outputs = (0u32..501)
        .map(|i| {
            let mut b = output.clone();
            b.id.0[..4].copy_from_slice(&i.to_be_bytes());
            b
        })
        .collect();
    store.apply_batch(&[block], true).unwrap();
    let app = router_over(store, 1866000);
    let (status, page) = get(&app, "/v1/rent/upcoming?blocks=2000000&limit=500").await;
    assert_eq!(status, 200);
    assert_eq!(page["items"].as_array().unwrap().len(), 500);
    assert_eq!(page["complete"], false);
}

#[tokio::test]
async fn supply_is_derived_from_the_emission_contract_not_a_schedule() {
    // The fixture store is seeded above genesis, so it has no emission box and must say so
    // rather than invent a number.
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/supply").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["genesis_total_nano"], "97739925000000000");
    assert_eq!(v["complete"], false);
    assert!(v["emitted_nano"].is_null(), "no genesis ⇒ no honest figure");
    assert!(v["emission_remaining_nano"].is_null());
    // The constant itself is the sum of the three chain-spec genesis boxes.
    assert_eq!(
        xp_types::GENESIS_TOTAL_NANO,
        93_409_132_500_000_000 + 4_330_791_500_000_000 + 1_000_000_000
    );
}

fn mainnet_genesis() -> Vec<xp_wire::DecodedBox> {
    xp_wire::decode_genesis_boxes(include_str!("../../../tests/fixtures/genesis.json")).unwrap()
}

fn genesis_app(boxes: &[xp_wire::DecodedBox]) -> (tempfile::TempDir, Router, Arc<Store>) {
    let (dir, _, mut state) = app_with_state(unlimited(), None);
    let store = Arc::new(Store::open(&dir.path().join("genesis.redb")).unwrap());
    store.seed_genesis(boxes).unwrap();
    state.store = store.clone();
    (dir, xp_api::router(state, &unlimited()), store)
}

#[tokio::test]
async fn supply_defines_gross_reserves_without_claiming_circulation() {
    let (_d, app, store) = genesis_app(&mainnet_genesis());
    let before = store.fingerprint().unwrap();
    let (st, v) = get(&app, "/v1/supply").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["complete"], true);
    assert_eq!(
        v["definition"],
        "genesis_allocation_minus_original_emission_reserve"
    );
    assert_eq!(v["outside_emission_nano"], "4330792500000000");
    assert!(v["circulating_nano"].is_null());
    assert_eq!(store.fingerprint().unwrap(), before);
}

#[tokio::test]
async fn supply_does_not_apply_mainnet_constants_to_other_genesis() {
    let mut boxes = mainnet_genesis();
    boxes[0].id = xp_types::BoxId([0xab; 32]);
    let (_d, app, _) = genesis_app(&boxes);
    let (st, v) = get(&app, "/v1/supply").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["complete"], false);
    assert!(v["emitted_nano"].is_null());
}

#[tokio::test]
async fn history_genesis_unseen_and_invalid_selectors() {
    let boxes = mainnet_genesis();
    let address = xp_wire::tree_info(&boxes[0].tree_bytes).unwrap().address;
    let (_d, app, store) = genesis_app(&boxes);
    let before = store.fingerprint().unwrap();
    let path = format!("/v1/addresses/{address}");
    let (st, v) = get(&app, &format!("{path}/balance/at?height=0")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["balance"]["nano"], boxes[0].value.to_string());
    assert_eq!(v["at"]["height"], 0);
    assert!(v["at"]["block_id"].is_null());
    let (st, p) = get(&app, &format!("{path}/boxes/at?height=0")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(p["items"][0]["inclusion_height"], 0);
    assert_eq!(p["items"][0]["nano"], v["balance"]["nano"]);
    for query in [
        "",
        "height=-1",
        "height=4294967296",
        "height=0&timestamp=1",
        "height=1",
        "height=0&limit=0",
    ] {
        assert_eq!(
            get(&app, &format!("{path}/balance/at?{query}")).await.0,
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
    assert_eq!(
        get(&app, "/v1/addresses/garbage/balance/at?height=0")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let unseen = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address;
    let (st, v) = get(&app, &format!("/v1/addresses/{unseen}/balance/at?height=0")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["balance"]["nano"], "0");
    assert_eq!(v["balance"]["tokens"], serde_json::json!([]));
    assert_eq!(store.fingerprint().unwrap(), before);
}

#[tokio::test]
async fn history_rejects_partial_store_and_legacy_network_stats_stays_absent() {
    let (_d, app) = app();
    let address = xp_wire::tree_info(&mainnet_genesis()[0].tree_bytes)
        .unwrap()
        .address;
    let (st, v) = get(
        &app,
        &format!("/v1/addresses/{address}/balance/at?height=1866000"),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["code"], "history_unavailable");
    assert_eq!(
        get(&app, "/v1/network/stats").await.0,
        StatusCode::NOT_FOUND
    );
}

fn history_id(n: u32) -> [u8; 32] {
    let mut id = [0x7e; 32];
    id[..4].copy_from_slice(&n.to_be_bytes());
    id
}

fn history_output(n: u32, value: u64, tokens: Vec<([u8; 32], u64)>) -> xp_wire::DecodedBox {
    let mut out = mainnet_genesis()[0].clone();
    out.id = xp_types::BoxId(history_id(n));
    out.value = value;
    out.tree_bytes = vec![0, 8, 0xd3];
    out.tree_hash = xp_wire::tree_hash(&out.tree_bytes);
    out.creation_height = 0; // deliberately older than inclusion
    out.tokens = tokens;
    out
}

fn history_tx(
    n: u32,
    inputs: Vec<xp_types::BoxId>,
    mut outputs: Vec<xp_wire::DecodedBox>,
) -> xp_wire::DecodedTx {
    let id = xp_types::TxId(history_id(n));
    for (i, out) in outputs.iter_mut().enumerate() {
        out.tx_id = id;
        out.index = i as u16;
    }
    xp_wire::DecodedTx {
        id,
        inputs,
        data_inputs: vec![],
        outputs,
        size: 100,
    }
}

fn history_block(
    h: u32,
    id: u32,
    parent: [u8; 32],
    txs: Vec<xp_wire::DecodedTx>,
) -> xp_wire::DecodedBlock {
    let mut block = fixture(1866000);
    block.header.height = h;
    block.header.id = xp_types::HeaderId(history_id(id));
    block.header.parent_id = xp_types::HeaderId(parent);
    block.txs = txs;
    block
}

async fn history_pages(app: &Router, address: &str, h: u32, limit: usize) -> Vec<Value> {
    let mut cursor = None;
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let suffix = cursor
            .as_ref()
            .map(|c| format!("&cursor={c}"))
            .unwrap_or_default();
        let (st, page) = get(
            app,
            &format!("/v1/addresses/{address}/boxes/at?height={h}&limit={limit}{suffix}"),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{page}");
        items.extend(page["items"].as_array().unwrap().clone());
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if let Some(ref c) = cursor {
            assert!(seen.insert(c.clone()), "history cursor did not advance");
        } else {
            break;
        }
    }
    items
}

#[tokio::test]
async fn history_matches_utxo_oracle_at_every_height_and_after_forks() {
    use std::collections::BTreeMap;
    let genesis = mainnet_genesis();
    let (_d, app, store) = genesis_app(&genesis);
    let address = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address;
    let token = genesis[2].id.0;
    let one = history_block(
        1,
        100,
        [0; 32],
        vec![history_tx(
            200,
            vec![genesis[2].id],
            vec![
                history_output(1, 4_000_000_000_000_000, vec![(token, 100)]),
                history_output(2, 1000, vec![]),
            ],
        )],
    );
    let two = history_block(
        2,
        101,
        one.header.id.0,
        vec![
            history_tx(
                201,
                vec![
                    xp_types::BoxId(history_id(1)),
                    xp_types::BoxId(history_id(2)),
                ],
                vec![
                    history_output(3, 3_000_000_000_000_000, vec![(token, 80)]),
                    history_output(4, 1000, vec![]),
                ],
            ),
            history_tx(
                202,
                vec![xp_types::BoxId(history_id(3))],
                vec![history_output(5, 3_000_000_000_000_000, vec![(token, 60)])],
            ),
        ],
    );
    let three = history_block(
        3,
        102,
        two.header.id.0,
        vec![history_tx(
            203,
            vec![xp_types::BoxId(history_id(5))],
            vec![],
        )],
    );
    let mut utxos = BTreeMap::new();
    let mut expected = vec![utxos.clone()];
    for block in [&one, &two, &three] {
        for tx in &block.txs {
            for input in &tx.inputs {
                utxos.remove(&input.0);
            }
            for out in &tx.outputs {
                utxos.insert(out.id.0, out.clone());
            }
        }
        expected.push(utxos.clone());
    }
    store
        .apply_batch(&[one.clone(), two.clone()], true)
        .unwrap();
    let before_three = store.fingerprint().unwrap();
    let (_, page) = get(
        &app,
        &format!("/v1/addresses/{address}/boxes/at?height=2&limit=1"),
    )
    .await;
    let cursor = page["next_cursor"].as_str().unwrap().to_owned();
    store
        .apply_batch(std::slice::from_ref(&three), true)
        .unwrap();
    assert_eq!(
        get(
            &app,
            &format!("/v1/addresses/{address}/boxes/at?height=2&cursor={cursor}")
        )
        .await
        .0,
        StatusCode::OK,
        "append preserves anchor"
    );
    for (h, oracle) in expected.iter().enumerate() {
        let (st, v) = get(
            &app,
            &format!("/v1/addresses/{address}/balance/at?height={h}"),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(
            v["balance"]["nano"],
            oracle
                .values()
                .map(|o| u128::from(o.value))
                .sum::<u128>()
                .to_string()
        );
        assert_eq!(v["balance"]["box_count"], oracle.len());
        let total = oracle
            .values()
            .flat_map(|o| o.tokens.iter())
            .map(|(_, n)| u128::from(*n))
            .sum::<u128>();
        let tokens = if total == 0 {
            serde_json::json!([])
        } else {
            serde_json::json!([{ "token_id": hex::encode(token), "amount": total.to_string() }])
        };
        assert_eq!(v["balance"]["tokens"], tokens);
        let items = history_pages(&app, &address, h as u32, 1).await;
        let ids: HashSet<_> = items
            .iter()
            .map(|i| i["box_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids, oracle.keys().map(hex::encode).collect());
        assert_eq!(items.len(), oracle.len());
    }
    store.rollback_to(2).unwrap();
    assert_eq!(
        store.fingerprint().unwrap(),
        before_three,
        "apply/rollback logical bytes unchanged"
    );
    store.rollback_to(1).unwrap();
    assert_eq!(
        get(
            &app,
            &format!("/v1/addresses/{address}/boxes/at?height=2&cursor={cursor}")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let fork = history_block(
        2,
        110,
        one.header.id.0,
        vec![history_tx(
            210,
            vec![xp_types::BoxId(history_id(1))],
            vec![history_output(6, 999, vec![(token, 20)])],
        )],
    );
    let before_fork = store.fingerprint().unwrap();
    store
        .apply_batch(std::slice::from_ref(&fork), true)
        .unwrap();
    assert_eq!(
        get(
            &app,
            &format!("/v1/addresses/{address}/boxes/at?height=2&cursor={cursor}")
        )
        .await
        .0,
        StatusCode::CONFLICT,
        "reused gidx cannot join forks"
    );
    let (_d2, fresh_app, fresh) = genesis_app(&genesis);
    fresh.apply_batch(&[one, fork], true).unwrap();
    for h in 0..=2 {
        let url = format!("/v1/addresses/{address}/balance/at?height={h}");
        assert_eq!(get(&app, &url).await.1, get(&fresh_app, &url).await.1);
        assert_eq!(
            history_pages(&app, &address, h, 1).await,
            history_pages(&fresh_app, &address, h, 1).await
        );
    }
    store.rollback_to(1).unwrap();
    assert_eq!(store.fingerprint().unwrap(), before_fork);
}

#[tokio::test]
async fn history_walks_ten_thousand_candidates_and_sparse_empty_pages() {
    let genesis = mainnet_genesis();
    let (_d, app, store) = genesis_app(&genesis);
    let address = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address;
    let outputs: Vec<_> = (1..=10_001)
        .map(|i| history_output(i, 1000, vec![]))
        .collect();
    let inputs = outputs.iter().take(10_000).map(|o| o.id).collect();
    let one = history_block(
        1,
        100_000,
        [0; 32],
        vec![history_tx(200_000, vec![genesis[2].id], outputs)],
    );
    let two = history_block(
        2,
        100_001,
        one.header.id.0,
        vec![history_tx(200_001, inputs, vec![])],
    );
    let three = history_block(3, 100_002, two.header.id.0, vec![]);
    store.apply_batch(&[one, two], true).unwrap();
    let fp = store.fingerprint().unwrap();
    store.apply_batch(&[three], true).unwrap();
    let path = format!("/v1/addresses/{address}");
    let (st, balance) = get(&app, &format!("{path}/balance/at?height=1")).await;
    assert_eq!(st, StatusCode::OK, "{balance}");
    assert_eq!(balance["balance"]["box_count"], 10_001);
    assert_eq!(balance["balance"]["nano"], "10001000");
    let start = std::time::Instant::now();
    let items = history_pages(&app, &address, 1, 500).await;
    eprintln!("10,001 historical boxes paged in {:?}", start.elapsed());
    assert_eq!(items.len(), 10_001);
    assert_eq!(
        items
            .iter()
            .map(|b| b["box_id"].as_str().unwrap())
            .collect::<HashSet<_>>()
            .len(),
        items.len()
    );
    let (st, sparse) = get(&app, &format!("{path}/boxes/at?height=2")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(sparse["items"].as_array().unwrap().is_empty());
    assert!(sparse["next_cursor"].is_string());
    let items = history_pages(&app, &address, 2, 500).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["box_id"], hex::encode(history_id(10_001)));
    store.rollback_to(2).unwrap();
    assert_eq!(store.fingerprint().unwrap(), fp);
}

#[tokio::test]
async fn history_spent_candidates_do_not_exhaust_scalar_budget() {
    let genesis = mainnet_genesis();
    let (_d, app, store) = genesis_app(&genesis);
    let address = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address;
    let outputs: Vec<_> = (1..=36_140)
        .map(|i| history_output(i, 1000, vec![]))
        .collect();
    let inputs = outputs.iter().take(36_137).map(|o| o.id).collect();
    let one = history_block(
        1,
        100_000,
        [0; 32],
        vec![history_tx(200_000, vec![genesis[2].id], outputs)],
    );
    let two = history_block(
        2,
        100_001,
        one.header.id.0,
        vec![history_tx(200_001, inputs, vec![])],
    );
    let three = history_block(3, 100_002, two.header.id.0, vec![]);
    store.apply_batch(&[one, two, three], true).unwrap();
    let (status, balance) = get(
        &app,
        &format!("/v1/addresses/{address}/balance/at?height=2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{balance}");
    assert_eq!(balance["balance"]["nano"], "3000");
    assert_eq!(balance["balance"]["box_count"], 3);
    assert_eq!(history_pages(&app, &address, 2, 2).await.len(), 3);
    assert_eq!(history_pages(&app, &address, 3, 2).await.len(), 3);
}

#[tokio::test]
async fn history_future_birth_before_live_candidate_does_not_end_walk() {
    use redb::ReadableTable;
    use xp_store::{
        keys::k_u32,
        rows::{HeaderRow, TxRow},
        tables::{HEADERS, META, META_INDEXED_HEIGHT, TXS},
    };
    // gidx is an iteration key, not a license for the handler to discard the tail.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reordered.redb");
    {
        let local = Store::open(&path).unwrap();
        local.seed_genesis(&mainnet_genesis()).unwrap();
        let one = history_block(
            1,
            100_000,
            [0; 32],
            vec![history_tx(
                200_000,
                vec![mainnet_genesis()[2].id],
                (1..=1001)
                    .map(|i| history_output(i, 1000, vec![]))
                    .collect(),
            )],
        );
        let two = history_block(
            2,
            100_001,
            one.header.id.0,
            vec![history_tx(
                200_001,
                vec![],
                vec![history_output(1002, 2000, vec![])],
            )],
        );
        local.apply_batch(&[one, two], true).unwrap();
    }
    {
        let db = redb::Database::create(&path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut rows = tx.open_table(TXS).unwrap();
            for (id, height) in [(200_000, 2), (200_001, 1)] {
                let key = history_id(id);
                let mut row =
                    TxRow::decode(rows.get(key.as_slice()).unwrap().unwrap().value()).unwrap();
                row.height = height;
                rows.insert(key.as_slice(), row.encode().as_slice())
                    .unwrap();
            }
            let mut headers = tx.open_table(HEADERS).unwrap();
            let row = HeaderRow::decode(headers.get(k_u32(2).as_slice()).unwrap().unwrap().value())
                .unwrap();
            headers
                .insert(k_u32(3).as_slice(), row.encode().as_slice())
                .unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(META_INDEXED_HEIGHT, k_u32(3).as_slice())
                .unwrap();
        }
        tx.commit().unwrap();
    }
    let (_, _, mut state) = app_with_state(unlimited(), None);
    state.store = Arc::new(Store::open(&path).unwrap());
    let app = xp_api::router(state, &unlimited());
    let address = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address;
    let (status, first) = get(
        &app,
        &format!("/v1/addresses/{address}/boxes/at?height=1&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(first["items"].as_array().unwrap().is_empty());
    assert!(first["next_cursor"].is_string());
    let items = history_pages(&app, &address, 1, 1).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["box_id"], hex::encode(history_id(1002)));
    let (status, balance) = get(
        &app,
        &format!("/v1/addresses/{address}/balance/at?height=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(balance["balance"]["nano"], "2000");
}

/// Export public data with scripts/capture-history-fixture.py, then run with
/// HISTORY_HTTP_FIXTURE=/tmp/history-mainnet.json cargo test -p xp-api
/// --test routes history_http_snapshot_replay -- --ignored --nocapture.
/// Only a fresh temporary database is written; the serving database is never opened.
#[tokio::test]
#[ignore = "requires an explicit public HTTP snapshot export"]
async fn history_http_snapshot_replay() {
    use xp_store::{
        keys::{k_hash_gidx, k_u32, k_u64},
        rows::{BoxRow, TxRow},
        tables::*,
    };
    fn hash(v: &Value) -> [u8; 32] {
        hex::decode(v.as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap()
    }
    let input = std::env::var("HISTORY_HTTP_FIXTURE").expect("set HISTORY_HTTP_FIXTURE");
    let data: Value = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let tip = data["tip"].as_u64().unwrap() as u32;
    let (_dir, app) = edited_genesis_app(|tx| {
        let mut gidx = 3u64;
        for address in data["addresses"].as_array().unwrap() {
            for b in address["boxes"].as_array().unwrap() {
                let id = hash(&b["id"]);
                let row = BoxRow {
                    gidx,
                    value: b["value"].as_str().unwrap().parse().unwrap(),
                    tree_hash: hash(&b["tree_hash"]),
                    creation_height: b["creation_height"].as_u64().unwrap() as u32,
                    tx_id: hash(&b["tx_id"]),
                    index: b["index"].as_u64().unwrap() as u16,
                    size: b["size"].as_u64().unwrap() as u32,
                    tokens: b["tokens"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| {
                            (
                                hash(&t["id"]),
                                t["amount"].as_str().unwrap().parse().unwrap(),
                            )
                        })
                        .collect(),
                    registers_json: b["registers"].to_string(),
                    spent: b["spent_height"]
                        .as_u64()
                        .map(|h| (hash(&b["spent_by"]), h as u32)),
                };
                tx.open_table(BOXES)
                    .unwrap()
                    .insert(id.as_slice(), row.encode().as_slice())
                    .unwrap();
                tx.open_table(BOX_BY_GIDX)
                    .unwrap()
                    .insert(k_u64(gidx).as_slice(), id.as_slice())
                    .unwrap();
                let key = k_hash_gidx(&row.tree_hash, gidx);
                tx.open_table(TREE_BOXES)
                    .unwrap()
                    .insert(key.as_slice(), &[][..])
                    .unwrap();
                if row.spent.is_none() {
                    tx.open_table(TREE_UNSPENT)
                        .unwrap()
                        .insert(key.as_slice(), &[][..])
                        .unwrap();
                }
                let creator = TxRow {
                    height: b["inclusion_height"].as_u64().unwrap() as u32,
                    index: 0,
                    gidx,
                    first_out_gidx: gidx,
                    timestamp: 0,
                    size: 0,
                    fee: 0,
                    inputs: vec![],
                    data_inputs: vec![],
                    output_count: 1,
                };
                tx.open_table(TXS)
                    .unwrap()
                    .insert(row.tx_id.as_slice(), creator.encode().as_slice())
                    .unwrap();
                gidx += 1;
            }
        }
        // Only the anchor headers are needed by these read routes. Their identities
        // are fixture identities, not a claim to reproduce an entire mainnet store.
        for h in [1_500_000, 1_600_000, 1_800_000, tip] {
            let header = xp_store::rows::HeaderRow {
                id: history_id(h),
                parent_id: [0; 32],
                timestamp: 0,
                difficulty: 0,
                miner_pk: [0; 33],
                tx_count: 0,
                first_tx_gidx: 0,
                size: 0,
                fees: 0,
                reward: 0,
                version: 1,
                raw_json: "{}".into(),
            };
            tx.open_table(HEADERS)
                .unwrap()
                .insert(k_u32(h).as_slice(), header.encode().as_slice())
                .unwrap();
        }
        tx.open_table(META)
            .unwrap()
            .insert(META_INDEXED_HEIGHT, k_u32(tip).as_slice())
            .unwrap();
    });
    for address in data["addresses"].as_array().unwrap() {
        let addr = address["address"].as_str().unwrap();
        let rows = address["boxes"].as_array().unwrap();
        for h in if address["full"] == true {
            vec![1_500_000, 1_600_000, 1_800_000, tip]
        } else {
            vec![tip]
        } {
            let expected: HashSet<_> = if h == tip {
                address["live"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap())
                    .collect()
            } else {
                rows.iter()
                    .filter(|b| {
                        b["inclusion_height"].as_u64().unwrap() <= h as u64
                            && b["spent_height"].as_u64().is_none_or(|s| s > h as u64)
                    })
                    .map(|b| b["id"].as_str().unwrap())
                    .collect()
            };
            let started = std::time::Instant::now();
            let items = history_pages(&app, addr, h, 500).await;
            let actual: HashSet<_> = items
                .iter()
                .map(|b| b["box_id"].as_str().unwrap())
                .collect();
            assert_eq!(actual.len(), items.len(), "duplicate boxes");
            assert_eq!(actual, expected, "membership at {h} for {addr}");
            let nano: u128 = rows
                .iter()
                .filter(|b| expected.contains(b["id"].as_str().unwrap()))
                .map(|b| b["value"].as_str().unwrap().parse::<u128>().unwrap())
                .sum();
            let paged_nano: u128 = items
                .iter()
                .map(|b| b["nano"].as_str().unwrap().parse::<u128>().unwrap())
                .sum();
            assert_eq!(paged_nano, nano);
            let mut expected_tokens = std::collections::BTreeMap::<String, u128>::new();
            for b in rows
                .iter()
                .filter(|b| expected.contains(b["id"].as_str().unwrap()))
            {
                for t in b["tokens"].as_array().unwrap() {
                    *expected_tokens
                        .entry(t["id"].as_str().unwrap().into())
                        .or_default() += t["amount"].as_str().unwrap().parse::<u128>().unwrap();
                }
            }
            if h != tip {
                let (status, balance) =
                    get(&app, &format!("/v1/addresses/{addr}/balance/at?height={h}")).await;
                assert_eq!(status, StatusCode::OK, "{balance}");
                assert_eq!(balance["balance"]["nano"], nano.to_string());
                assert_eq!(balance["balance"]["box_count"], expected.len());
                let actual_tokens: std::collections::BTreeMap<String, u128> = balance["balance"]
                    ["tokens"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| {
                        (
                            t["token_id"].as_str().unwrap().into(),
                            t["amount"].as_str().unwrap().parse().unwrap(),
                        )
                    })
                    .collect();
                assert_eq!(actual_tokens, expected_tokens);
            }
            eprintln!(
                "{addr} height={h} candidates={} boxes={} nano={nano} elapsed={:?}",
                rows.len(),
                items.len(),
                started.elapsed()
            );
        }
    }
}

/// Corrupt only a newly created fixture file, while no Store owns it.
fn edited_genesis_app(edit: impl FnOnce(&redb::WriteTransaction)) -> (tempfile::TempDir, Router) {
    let (dir, _, mut state) = app_with_state(unlimited(), None);
    let path = dir.path().join("edited.redb");
    {
        let store = Store::open(&path).unwrap();
        store.seed_genesis(&mainnet_genesis()).unwrap();
    }
    {
        let db = redb::Database::create(&path).unwrap();
        let tx = db.begin_write().unwrap();
        edit(&tx);
        tx.commit().unwrap();
    }
    state.store = Arc::new(Store::open(&path).unwrap());
    (dir, xp_api::router(state, &unlimited()))
}

#[tokio::test]
async fn supply_missing_balance_and_impossible_total_are_errors_but_zero_is_valid() {
    use redb::ReadableTable;
    use xp_store::{rows::BalanceRow, tables::TREE_BALANCE};
    let tree = mainnet_genesis()[0].tree_hash.0;
    for amount in [None, Some(xp_types::GENESIS_TOTAL_NANO + 1), Some(0)] {
        let (_d, app) = edited_genesis_app(|tx| {
            let mut table = tx.open_table(TREE_BALANCE).unwrap();
            let mut balance =
                BalanceRow::decode(table.get(tree.as_slice()).unwrap().unwrap().value()).unwrap();
            match amount {
                None => {
                    table.remove(tree.as_slice()).unwrap();
                }
                Some(nano) => {
                    balance.nano = nano;
                    table
                        .insert(tree.as_slice(), balance.encode().as_slice())
                        .unwrap();
                }
            }
        });
        let (st, v) = get(&app, "/v1/supply").await;
        if amount == Some(0) {
            assert_eq!(st, StatusCode::OK);
            assert_eq!(
                v["outside_emission_nano"],
                xp_types::GENESIS_TOTAL_NANO.to_string()
            );
        } else {
            assert_eq!(st, StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
}

#[tokio::test]
async fn historical_tip_response_size_is_bounded_and_problem_headers_are_typed() {
    use redb::ReadableTable;
    use xp_store::{rows::BalanceRow, tables::TREE_BALANCE};
    let genesis = mainnet_genesis();
    let tree = genesis[0].tree_hash.0;
    let address = xp_wire::tree_info(&genesis[0].tree_bytes).unwrap().address;
    let (_d, app) = edited_genesis_app(|tx| {
        let mut table = tx.open_table(TREE_BALANCE).unwrap();
        let mut balance =
            BalanceRow::decode(table.get(tree.as_slice()).unwrap().unwrap().value()).unwrap();
        balance.tokens = (0..30_000).map(|n| (history_id(n), 1)).collect();
        table
            .insert(tree.as_slice(), balance.encode().as_slice())
            .unwrap();
    });
    let (st, headers, v) = get_from(
        &app,
        &format!("/v1/addresses/{address}/balance/at?height=0"),
        "127.0.0.1",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["code"], "history_response_limit");
    assert_eq!(headers["content-type"], "application/problem+json");
}

#[tokio::test]
async fn rent_reports_the_signed_consensus_fee_not_just_nominal_rent() {
    // Consensus computes the storage fee as a wrapping i32 multiply, so a box of 1,718 bytes
    // or more has a NEGATIVE fee and cannot be rent-claimed at any age — while the nominal
    // `min(size × 1_250_000, value)` still looks like a healthy opportunity. The rent DTO must
    // carry both so the pages cannot advertise rent nobody can take.
    use xp_types::rent::{consensus_storage_fee, is_rent_claimable, rent_due};
    assert!(is_rent_claimable(1_717));
    assert!(!is_rent_claimable(1_718));
    assert_eq!(consensus_storage_fee(2_008), -1_784_967_296);
    // Nominal rent on that same box looks collectible; that is the trap.
    assert_eq!(rent_due(2_008, 10_000_000_000), 2_510_000_000);

    // And the wire shape exposes it on a real fixture box.
    let (_d, app) = app();
    let (st, v) = get(&app, "/v1/rent/eligible?limit=1").await;
    assert_eq!(st, StatusCode::OK);
    if let Some(item) = v["items"].as_array().and_then(|a| a.first()) {
        let rent = &item["box"]["rent"];
        assert!(rent["consensus_fee_nano"].is_string(), "{rent}");
        assert!(rent["collectible"].is_boolean(), "{rent}");
        let fee: i64 = rent["consensus_fee_nano"]
            .as_str()
            .unwrap()
            .parse()
            .expect("decimal i64");
        assert_eq!(rent["collectible"].as_bool().unwrap(), fee > 0);
    }
}

/// Each body request waits for the test, so assertions inspect published status between
/// attempts without depending on scheduling or wall-clock sleeps.
struct FailingBodies {
    tip_id: xp_types::Hash32,
    requests: tokio::sync::mpsc::UnboundedSender<
        tokio::sync::oneshot::Sender<Result<Option<String>, xp_source::SourceError>>,
    >,
}

#[async_trait::async_trait]
impl xp_source::BlockSource for FailingBodies {
    fn name(&self) -> &str {
        "failing-bodies"
    }

    async fn best_height(&self) -> Result<u32, xp_source::SourceError> {
        Ok(1866003)
    }

    async fn header_id_at(
        &self,
        _height: u32,
    ) -> Result<Option<xp_types::Hash32>, xp_source::SourceError> {
        Ok(Some(self.tip_id))
    }

    async fn full_block_json(
        &self,
        _id: &xp_types::Hash32,
    ) -> Result<Option<String>, xp_source::SourceError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.requests.send(tx).unwrap();
        rx.await.unwrap()
    }

    async fn genesis_boxes_json(&self) -> Result<String, xp_source::SourceError> {
        Ok("[]".into())
    }
}

#[tokio::test]
async fn status_repeated_body_errors_reports_failure_and_recovers() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let (_dir, _app, mut state) = app_with_state(unlimited(), None);
        let (tx, mut rx) = watch::channel(state.status.borrow().clone());
        state.status = rx.clone();
        let app = xp_api::router(state.clone(), &unlimited());
        let (requests, mut bodies) = tokio::sync::mpsc::unbounded_channel();
        let source = Arc::new(FailingBodies {
            tip_id: state.store.header_id_at(1866002).unwrap().unwrap(),
            requests,
        });
        let shutdown = tokio_util::sync::CancellationToken::new();
        let handle = tokio::spawn(xp_ingest::run(
            state.store.clone(),
            source,
            xp_ingest::IngestConfig {
                poll_ms: 1,
                ..Default::default()
            },
            tx,
            shutdown.clone(),
        ));

        let mut request = bodies.recv().await.unwrap();
        for attempt in 1..=4 {
            request
                .send(Err(xp_source::SourceError::Http("body HTTP 503".into())))
                .unwrap();
            request = bodies.recv().await.unwrap();
            // The next attempt's /info has already succeeded, but must not clear the error.
            let (code, status) = get(&app, "/v1/status").await;
            assert_eq!(code, StatusCode::OK);
            assert!(status["source_observed_at_ms"].is_number());
            assert_eq!(status["indexed"], 1866002);
            assert!(status["stalled"].is_null());
            if attempt < 3 {
                assert!(status["source_error"].is_null());
            } else {
                assert!(status["source_error"]
                    .as_str()
                    .unwrap()
                    .contains("body HTTP 503"));
            }
        }

        // A successful response withholding the body is a stall, not a fetch error.
        request.send(Ok(None)).unwrap();
        while rx.borrow().stalled.is_none() {
            rx.changed().await.unwrap();
        }
        let (_, status) = get(&app, "/v1/status").await;
        assert!(status["source_error"].is_null());
        assert_eq!(status["stalled"]["height"], 1866003);
        shutdown.cancel();
        handle.await.unwrap().unwrap();
    })
    .await
    .expect("ingest did not publish status");
}

// M2 integrity fixtures: build complete synthetic history, close its only owner,
// mutate one reference, close redb, then hand the file to the router's only Store.
fn integrity_blocks() -> Vec<xp_wire::DecodedBlock> {
    let token = mainnet_genesis()[0].id.0;
    let mut first = history_output(101, 100, vec![(token, 70)]);
    first.registers_json = r#"{"R4":"0402"}"#.into(); // valid non-EIP-4 name
    let second = history_output(102, 200, vec![(token, 30)]);
    let b1 = history_block(
        1,
        201,
        [0; 32],
        vec![history_tx(
            301,
            vec![mainnet_genesis()[0].id],
            vec![first, second],
        )],
    );
    let b2 = history_block(
        2,
        202,
        b1.header.id.0,
        vec![history_tx(
            302,
            vec![xp_types::BoxId(history_id(101))],
            vec![history_output(103, 90, vec![(token, 60)])],
        )],
    );
    vec![b1, b2]
}

fn integrity_store(
    partial: bool,
    edit: impl FnOnce(&redb::WriteTransaction),
) -> (tempfile::TempDir, Store) {
    integrity_store_at(partial, 2, edit)
}

fn integrity_store_at(
    partial: bool,
    height: usize,
    edit: impl FnOnce(&redb::WriteTransaction),
) -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("integrity.redb");
    {
        let store = Store::open(&path).unwrap();
        if partial {
            store.seed_for_tests(0, [0; 32]).unwrap();
        } else {
            store.seed_genesis(&mainnet_genesis()).unwrap();
        }
        store
            .apply_batch(&integrity_blocks()[..height], true)
            .unwrap();
    }
    {
        let db = redb::Database::create(&path).unwrap();
        let tx = db.begin_write().unwrap();
        edit(&tx);
        tx.commit().unwrap();
    }
    let store = Store::open(&path).unwrap();
    (dir, store)
}

async fn assert_integrity_routes(app: &Router, paths: &[String]) {
    for path in paths {
        let (status, value) = get(app, path).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{path}: {value}");
        assert_eq!(value["code"], "integrity_error", "{path}: {value}");
        assert!(
            value.get("items").is_none(),
            "must not return a shortened list"
        );
        assert!(
            value.get("balance").is_none(),
            "must not return a partial total"
        );
    }
    // The same damaged store must still distinguish unrelated requested ids.
    let unknown = hex::encode([0xfa; 32]);
    for path in [
        format!("/v1/boxes/{unknown}"),
        format!("/v1/txs/{unknown}"),
        format!("/v1/tokens/{unknown}"),
        format!("/v1/templates/{unknown}"),
        format!("/v1/blocks/{unknown}"),
        "/v1/blocks/999".into(),
        "/v1/addresses/unknown".into(),
    ] {
        assert_eq!(get(app, &path).await.0, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn integrity_required_rows_fail_full_store_lists_details_and_totals() {
    use xp_store::{keys::*, tables::*};
    let tree = xp_wire::tree_hash(&[0, 8, 0xd3]).0;
    let address = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address;
    let template = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().template_hash;
    let token = mainnet_genesis()[0].id.0;
    let tx_detail = format!("/v1/txs/{}", hex::encode(history_id(301)));
    let box_detail = format!("/v1/boxes/{}", hex::encode(history_id(102)));
    let addr = format!("/v1/addresses/{address}");
    let token_path = format!("/v1/tokens/{}", hex::encode(token));
    let template_path = format!("/v1/templates/{}", hex::encode(template));
    let expansion = vec![
        "/v1/txs".into(),
        tx_detail.clone(),
        "/v1/blocks/1/txs".into(),
    ];
    let box_pages = vec![
        format!("{addr}/boxes"),
        format!("{addr}/boxes?unspent=true"),
        format!("{token_path}/boxes"),
        format!("{token_path}/boxes?unspent=true"),
        format!("{template_path}/boxes"),
        format!("{template_path}/boxes?unspent=true"),
        format!("{addr}/rent"),
    ];
    let mut tree_paths = expansion.clone();
    tree_paths.extend([
        box_detail.clone(),
        "/v1/richlist".into(),
        format!("{token_path}/holders"),
        template_path.clone(),
        "/v1/rent/upcoming?blocks=2000000".into(),
    ]);
    let mut box_paths = expansion.clone();
    box_paths.extend(box_pages.clone());
    box_paths.push("/v1/rent/upcoming?blocks=2000000".into());
    box_paths.push(format!("{addr}/balance/at?height=1"));
    box_paths.push(format!("{addr}/boxes/at?height=1"));
    // Each case gets a fresh, independently damaged temporary file.
    let cases = vec![
        (
            META,
            META_NEXT_TX_GIDX.to_vec(),
            vec!["/v1/txs".into(), "/v1/txs?dir=asc".into()],
        ),
        (
            HEADERS,
            k_u32(1).to_vec(),
            vec![
                "/v1/blocks".into(),
                "/v1/blocks/1".into(),
                format!("/v1/blocks/{}", hex::encode(history_id(201))),
                "/v1/blocks/1/txs".into(),
            ],
        ),
        (
            TXS,
            history_id(301).to_vec(),
            vec![
                "/v1/txs".into(),
                "/v1/blocks/1/txs".into(),
                format!("{addr}/txs"),
                format!("{addr}/balance/at?height=1"),
                format!("{addr}/boxes/at?height=1"),
            ],
        ),
        (
            TX_BY_GIDX,
            k_u64(0).to_vec(),
            vec![
                "/v1/txs".into(),
                "/v1/blocks/1/txs".into(),
                format!("{addr}/txs"),
            ],
        ),
        (
            BOX_BY_GIDX,
            k_u64(4).to_vec(),
            box_paths
                .clone()
                .into_iter()
                .filter(|p| !p.starts_with("/v1/rent/"))
                .collect(),
        ),
        (BOXES, history_id(102).to_vec(), box_paths),
        (
            BOX_BY_GIDX,
            k_u64(3).to_vec(),
            vec![
                "/v1/registers/R4/0402/boxes".into(),
                format!("{addr}/boxes"),
                format!("{token_path}/boxes"),
                format!("{template_path}/boxes"),
                tx_detail.clone(),
                "/v1/txs".into(),
            ],
        ),
        (ERGO_TREES, tree.to_vec(), tree_paths),
        (BOXES, mainnet_genesis()[0].id.0.to_vec(), expansion.clone()),
        (TOKENS, token.to_vec(), {
            let mut paths = expansion.clone();
            paths.extend([
                box_detail.clone(),
                addr.clone(),
                "/v1/tokens".into(),
                "/v1/tokens?sort=holders".into(),
            ]);
            paths.extend([format!("{addr}/boxes"), format!("{template_path}/boxes")]);
            paths
        }),
        (
            TREE_BALANCE,
            tree.to_vec(),
            vec![addr.clone(), "/v1/richlist".into()],
        ),
        (
            TOKEN_HOLDER_AMT,
            k_token_tree(&token, &tree).to_vec(),
            vec![format!("{token_path}/holders")],
        ),
        (
            META,
            META_EMISSION_TREE_HASH.to_vec(),
            vec!["/v1/supply".into()],
        ),
        (
            TREE_BALANCE,
            mainnet_genesis()[0].tree_hash.0.to_vec(),
            vec!["/v1/supply".into()],
        ),
    ];
    for (table, key, paths) in cases {
        let (_dir, store) = integrity_store(false, |tx| {
            assert!(tx
                .open_table(table)
                .unwrap()
                .remove(key.as_slice())
                .unwrap()
                .is_some());
        });
        assert_integrity_routes(&router_over(store, 2), &paths).await;
    }
}

#[tokio::test]
async fn integrity_invalid_register_json_fails_expansion_but_valid_null_is_preserved() {
    use redb::ReadableTable;
    use xp_store::{rows::BoxRow, tables::BOXES};
    for json in ["{broken", "null"] {
        let (_dir, store) = integrity_store(false, |tx| {
            let mut boxes = tx.open_table(BOXES).unwrap();
            let id = history_id(102);
            let mut row =
                BoxRow::decode(boxes.get(id.as_slice()).unwrap().unwrap().value()).unwrap();
            row.registers_json = json.into();
            boxes
                .insert(id.as_slice(), row.encode().as_slice())
                .unwrap();
        });
        let app = router_over(store, 2);
        let detail = format!("/v1/boxes/{}", hex::encode(history_id(102)));
        if json == "null" {
            let (status, value) = get(&app, &detail).await;
            assert_eq!(status, StatusCode::OK);
            assert!(value["registers"].is_null());
        } else {
            assert_integrity_routes(
                &app,
                &[
                    detail,
                    "/v1/txs".into(),
                    "/v1/blocks/1/txs".into(),
                    format!("/v1/txs/{}", hex::encode(history_id(301))),
                ],
            )
            .await;
        }
    }
}

#[tokio::test]
async fn integrity_partial_preseed_inputs_and_mints_are_explicitly_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("preseed.redb")).unwrap();
    let blocks = integrity_blocks();
    // Start after the mint and the spent box were created: no corruption injection.
    store.seed_for_tests(1, blocks[0].header.id.0).unwrap();
    store.apply_batch(&blocks[1..], true).unwrap();
    let app = router_over(store, 2);
    let (status, headers, value) = get_from(
        &app,
        &format!("/v1/txs/{}", hex::encode(history_id(302))),
        "127.0.0.1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-explorer-completeness"], "incomplete");
    assert!(value["inputs"][0]["box"].is_null());
    assert!(value["outputs"][0]["tokens"][0]["name"].is_null());
    assert_eq!(value["outputs"][0]["tokens"][0]["amount"], "60");
    let address = xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address;
    let (status, headers, balance) =
        get_from(&app, &format!("/v1/addresses/{address}"), "127.0.0.1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-explorer-completeness"], "incomplete");
    assert_eq!(balance["balance"]["nano"], "90");
    assert_eq!(balance["balance"]["tokens"][0]["amount"], "60");
    for path in [
        "/v1/txs",
        "/v1/richlist",
        "/v1/rent/upcoming?blocks=2000000",
    ] {
        let (status, headers, _) = get_from(&app, path, "127.0.0.1", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["x-explorer-completeness"], "incomplete");
    }
}

#[tokio::test]
async fn integrity_partial_in_range_dangling_indexes_still_fail() {
    use xp_store::{keys::*, tables::*};
    for (table, key, paths) in [
        (
            BOX_BY_GIDX,
            k_u64(1).to_vec(),
            vec![
                "/v1/txs".into(),
                "/v1/blocks/1/txs".into(),
                format!("/v1/txs/{}", hex::encode(history_id(301))),
            ],
        ),
        (
            TOKENS,
            mainnet_genesis()[0].id.0.to_vec(),
            vec!["/v1/tokens".into(), "/v1/tokens?sort=holders".into()],
        ),
        (
            HEADERS,
            k_u32(1).to_vec(),
            vec!["/v1/blocks".into(), "/v1/blocks/1".into()],
        ),
    ] {
        let (_dir, store) = integrity_store(true, |tx| {
            assert!(tx
                .open_table(table)
                .unwrap()
                .remove(key.as_slice())
                .unwrap()
                .is_some());
        });
        assert_integrity_routes(&router_over(store, 2), &paths).await;
    }
}

#[tokio::test]
async fn integrity_full_store_optional_eip4_fields_and_unknown_ids_stay_valid() {
    let (_dir, store) = integrity_store(false, |_| {});
    let app = router_over(store, 2);
    let (status, headers, value) = get_from(
        &app,
        &format!("/v1/boxes/{}", hex::encode(history_id(102))),
        "127.0.0.1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-explorer-completeness"], "complete");
    assert_eq!(value["tokens"][0]["name"], "");
    assert!(value["tokens"][0]["decimals"].is_null());
    // Empty paths still exercise every unrelated unknown-id assertion.
    assert_integrity_routes(&app, &[]).await;
}

#[test]
fn integrity_rollback_missing_spent_box_is_atomic_in_full_and_partial_stores() {
    use xp_store::{tables::BOXES, StoreError};
    for partial in [false, true] {
        let (_dir, store) = integrity_store(partial, |tx| {
            assert!(tx
                .open_table(BOXES)
                .unwrap()
                .remove(history_id(101).as_slice())
                .unwrap()
                .is_some());
        });
        let before = store.fingerprint().unwrap();
        assert!(matches!(
            store.rollback_to(1),
            Err(StoreError::Corrupt("undo: spent box missing"))
        ));
        assert_eq!(store.fingerprint().unwrap(), before);
        assert_eq!(store.indexed_height().unwrap(), Some(2));
    }
}

#[test]
fn integrity_required_apply_and_rollback_references_abort_atomically() {
    use xp_store::{keys::*, tables::*, StoreError};
    let tree = xp_wire::tree_hash(&[0, 8, 0xd3]).0;
    let token = mainnet_genesis()[0].id.0;
    // Apply the second block only after independently damaging one of its required
    // inputs. Full-store mint absence must not silently skip burns or counters.
    for (table, key) in [
        (META, META_NEXT_BOX_GIDX.to_vec()),
        (META, META_NEXT_TX_GIDX.to_vec()),
        (BOXES, history_id(101).to_vec()),
        (ERGO_TREES, tree.to_vec()),
        (TOKENS, token.to_vec()),
        (HEADERS, k_u32(1).to_vec()),
    ] {
        let (_dir, store) = integrity_store_at(false, 1, |tx| {
            assert!(tx
                .open_table(table)
                .unwrap()
                .remove(key.as_slice())
                .unwrap()
                .is_some());
        });
        let before = store.fingerprint().unwrap();
        assert!(matches!(
            store.apply_batch(&integrity_blocks()[1..], true),
            Err(StoreError::Corrupt(_))
        ));
        assert_eq!(store.fingerprint().unwrap(), before);
        assert_eq!(store.indexed_height().unwrap(), Some(1));
    }
    // These are missing indexed rows named by the existing rollback journal, not
    // changes to UNDO encoding, retention or its verification model.
    for (table, key) in [
        (BOXES, history_id(103).to_vec()),
        (ERGO_TREES, tree.to_vec()),
        (TXS, history_id(302).to_vec()),
        (HEADERS, k_u32(2).to_vec()),
        (TREE_BALANCE, tree.to_vec()),
        (TOKENS, token.to_vec()),
    ] {
        let (_dir, store) = integrity_store(false, |tx| {
            assert!(tx
                .open_table(table)
                .unwrap()
                .remove(key.as_slice())
                .unwrap()
                .is_some());
        });
        let before = store.fingerprint().unwrap();
        assert!(matches!(store.rollback_to(1), Err(StoreError::Corrupt(_))));
        assert_eq!(store.fingerprint().unwrap(), before);
        assert_eq!(store.indexed_height().unwrap(), Some(2));
    }
}

#[tokio::test]
async fn integrity_rent_eligible_does_not_skip_a_missing_box() {
    use xp_store::tables::BOXES;
    let (_dir, store) = integrity_store(false, |tx| {
        // Keep canonical coverage contiguous while moving only this fixture's
        // maturity entries below its tip. No production file is opened.
        let mut rent = tx.open_table(xp_store::tables::RENT_MATURES).unwrap();
        use redb::ReadableTable;
        let entries: Vec<_> = rent
            .iter()
            .unwrap()
            .map(|entry| {
                let (key, value) = entry.unwrap();
                (key.value().to_vec(), value.value().to_vec())
            })
            .collect();
        for (key, value) in entries {
            let gidx = xp_store::keys::gidx_of_composite(&key).unwrap();
            rent.remove(key.as_slice()).unwrap();
            rent.insert(xp_store::keys::k_rent(1, gidx).as_slice(), value.as_slice())
                .unwrap();
        }
        tx.open_table(BOXES)
            .unwrap()
            .remove(history_id(102).as_slice())
            .unwrap();
    });
    assert_integrity_routes(&router_over(store, 2), &["/v1/rent/eligible".into()]).await;
}

#[tokio::test]
async fn summary_routes_match_all_fixture_fields_without_enrichment() {
    use xp_store::{read::Dir, Reader};
    let (_dir, app, state) = app_with_state(unlimited(), None);
    let rd = Reader::new(&state.store).unwrap();
    let rows = rd.txs_by_gidx(None, 500, Dir::Asc).unwrap().items;
    let expected: Vec<Value> = rows
        .iter()
        .map(|(id, row)| {
            serde_json::json!({
                "id": hex::encode(id), "height": row.height, "index": row.index,
                "timestamp": row.timestamp, "size": row.size, "fee": row.fee.to_string(),
                "input_count": row.inputs.len(), "data_input_count": row.data_inputs.len(),
                "output_count": row.output_count,
            })
        })
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(state.store.lookup_counts(), [0; 3]);
    for dir in ["asc", "desc"] {
        let mut routes = vec![("/v1/tx-summaries".to_string(), expected.clone())];
        for height in 1866000..=1866002 {
            let block: Vec<_> = expected
                .iter()
                .filter(|row| row["height"] == height)
                .cloned()
                .collect();
            routes.push((format!("/v1/blocks/{height}/tx-summaries"), block.clone()));
            let id = hex::encode(rd.header_at(height).unwrap().unwrap().id);
            routes.push((format!("/v1/blocks/{id}/tx-summaries"), block));
        }
        for (route, mut wanted) in routes {
            if dir == "desc" {
                wanted.reverse();
            }
            let mut actual = Vec::new();
            let mut cursor = None;
            for _ in 0..=wanted.len() {
                let path = format!(
                    "{route}?limit=1&dir={dir}{}",
                    cursor
                        .as_ref()
                        .map(|c| format!("&cursor={c}"))
                        .unwrap_or_default()
                );
                let (status, value) = get(&app, &path).await;
                assert_eq!(status, StatusCode::OK, "{path}: {value}");
                actual.extend(value["items"].as_array().unwrap().iter().cloned());
                let next = value["next_cursor"].as_str().map(str::to_owned);
                if next.is_none() {
                    break;
                }
                assert_ne!(next, cursor, "exclusive cursor must advance");
                cursor = next;
            }
            assert_eq!(actual, wanted, "{route} {dir}");
            assert_eq!(state.store.lookup_counts(), [0; 3], "{route} enriched rows");
        }
    }
    for route in ["/v1/tx-summaries", "/v1/blocks/1866000/tx-summaries"] {
        for query in ["limit=0", "limit=no", "cursor=no", "dir=no"] {
            assert_eq!(
                get(&app, &format!("{route}?{query}")).await.0,
                StatusCode::BAD_REQUEST
            );
        }
        for query in ["cursor=18446744073709551615&dir=asc", "cursor=0&dir=desc"] {
            assert_eq!(
                get(&app, &format!("{route}?{query}")).await.1["items"],
                serde_json::json!([])
            );
        }
    }
    assert_eq!(
        get(&app, "/v1/blocks/999/tx-summaries").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(state.store.lookup_counts(), [0; 3]);

    // Compare the new implementation's complete legacy JSON with the original DTO path,
    // and demonstrate that the same per-store counters detect actual enrichment.
    let tip = rd.indexed_height().unwrap();
    let emission = rd.emission_tree_hash().unwrap();
    let mut full = rows
        .iter()
        .map(|(id, row)| xp_api::dto::tx_dto(&rd, id, row, tip, emission.as_ref()).unwrap())
        .collect::<Vec<_>>();
    xp_api::dto::enrich_txs(&rd, full.iter_mut()).unwrap();
    let full = serde_json::to_value(full).unwrap();
    assert_eq!(
        get(&app, "/v1/txs?dir=asc&limit=500").await.1["items"],
        full
    );
    for (summary, tx) in expected.iter().zip(full.as_array().unwrap()) {
        for key in ["id", "height", "index", "timestamp", "size", "fee"] {
            assert_eq!(summary[key], tx[key]);
        }
        for (count, field) in [
            ("input_count", "inputs"),
            ("data_input_count", "data_inputs"),
            ("output_count", "outputs"),
        ] {
            assert_eq!(summary[count], tx[field].as_array().unwrap().len());
        }
        assert_eq!(
            get(
                &app,
                &format!("/v1/txs/{}", summary["id"].as_str().unwrap())
            )
            .await
            .1,
            *tx
        );
    }
    for height in 1866000..=1866002 {
        let wanted: Vec<_> = full
            .as_array()
            .unwrap()
            .iter()
            .filter(|tx| tx["height"] == height)
            .cloned()
            .collect();
        assert_eq!(
            get(&app, &format!("/v1/blocks/{height}/txs")).await.1,
            serde_json::json!(wanted)
        );
    }
    let counts = state.store.lookup_counts();
    assert!(
        counts[0] > 0 && counts[1] > 0 && counts[2] > 0,
        "positive control: {counts:?}"
    );
}

#[tokio::test]
async fn legacy_expansion_rejects_one_oversized_transaction_without_partial_arrays() {
    use redb::ReadableTable;
    use xp_store::{rows::TxRow, tables::TXS};
    let (_dir, store) = integrity_store(false, |tx| {
        let mut table = tx.open_table(TXS).unwrap();
        let id = history_id(302);
        let mut row = TxRow::decode(table.get(id.as_slice()).unwrap().unwrap().value()).unwrap();
        row.inputs = vec![[0xff; 32]; 10_001];
        table
            .insert(id.as_slice(), row.encode().as_slice())
            .unwrap();
    });
    let app = router_over(store, 2);
    for path in [
        format!("/v1/txs/{}", hex::encode(history_id(302))),
        "/v1/txs?dir=asc".into(),
        "/v1/blocks/2/txs".into(),
    ] {
        let (status, headers, body) = get_from(&app, &path, "127.0.0.1", None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {body}");
        assert_eq!(headers["content-type"], "application/problem+json");
        assert_eq!(body["code"], "expansion_work_limit");
        assert!(!body.is_array() && body.get("items").is_none());
    }
    assert_eq!(
        get(&app, "/v1/blocks/2/tx-summaries").await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn summary_count_overflow_is_integrity_error_for_both_input_counts() {
    use redb::ReadableTable;
    use xp_store::{rows::TxRow, tables::TXS};
    for data_inputs in [false, true] {
        let (_dir, store) = integrity_store(false, |tx| {
            let mut table = tx.open_table(TXS).unwrap();
            let id = history_id(302);
            let mut row =
                TxRow::decode(table.get(id.as_slice()).unwrap().unwrap().value()).unwrap();
            let ids = vec![[0xff; 32]; usize::from(u16::MAX) + 1];
            if data_inputs {
                row.data_inputs = ids;
            } else {
                row.inputs = ids;
            }
            table
                .insert(id.as_slice(), row.encode().as_slice())
                .unwrap();
        });
        let rd = xp_store::Reader::new(&store).unwrap();
        let tree = integrity_blocks()[1].txs[0].outputs[0].tree_hash.0;
        let address = rd.tree_row(&tree).unwrap().unwrap().address;
        drop(rd);
        let app = router_over(store, 2);
        let address_path = format!("/v1/addresses/{address}/txs");
        for path in [
            "/v1/tx-summaries",
            "/v1/blocks/2/tx-summaries",
            &address_path,
        ] {
            let (status, body) = get(&app, path).await;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body["code"], "integrity_error");
        }
    }
}

#[tokio::test]
async fn legacy_block_budget_is_cumulative_and_never_returns_a_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("cumulative.redb")).unwrap();
    store.seed_genesis(&mainnet_genesis()).unwrap();
    let mut txs: Vec<_> = integrity_blocks()
        .into_iter()
        .flat_map(|block| block.txs)
        .collect();
    for tx in &mut txs {
        tx.data_inputs = vec![mainnet_genesis()[0].id; 5_000];
    }
    store
        .apply_batch(&[history_block(1, 201, [0; 32], txs)], true)
        .unwrap();
    let app = router_over(store, 1);
    for id in [301, 302] {
        assert_eq!(
            get(&app, &format!("/v1/txs/{}", hex::encode(history_id(id))))
                .await
                .0,
            StatusCode::OK
        );
    }
    for route in ["/v1/blocks/1/txs", "/v1/txs?dir=asc"] {
        let (status, value) = get(&app, route).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{route}: {value}");
        assert_eq!(value["code"], "expansion_work_limit");
        assert!(!value.is_array() && value.get("items").is_none());
    }
    assert_eq!(
        get(&app, "/v1/blocks/1/tx-summaries").await.1["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn legacy_admission_rejects_oversized_enrichment_rows_before_decode() {
    use redb::ReadableTable;
    use xp_store::{
        rows::{BoxRow, TokenRow, TreeRow},
        tables::*,
    };
    for kind in ["registers", "tree", "token"] {
        let (_dir, store) = integrity_store(false, |tx| {
            let huge = "x".repeat(2 * 1024 * 1024);
            match kind {
                "registers" => {
                    let mut table = tx.open_table(BOXES).unwrap();
                    let id = history_id(103);
                    let mut row =
                        BoxRow::decode(table.get(id.as_slice()).unwrap().unwrap().value()).unwrap();
                    row.registers_json = serde_json::json!({"R4": huge}).to_string();
                    table
                        .insert(id.as_slice(), row.encode().as_slice())
                        .unwrap();
                }
                "tree" => {
                    let mut table = tx.open_table(ERGO_TREES).unwrap();
                    let id = xp_wire::tree_hash(&[0, 8, 0xd3]).0;
                    let mut row =
                        TreeRow::decode(table.get(id.as_slice()).unwrap().unwrap().value())
                            .unwrap();
                    row.tree_bytes = huge.into_bytes();
                    table
                        .insert(id.as_slice(), row.encode().as_slice())
                        .unwrap();
                }
                "token" => {
                    let mut table = tx.open_table(TOKENS).unwrap();
                    let id = mainnet_genesis()[0].id.0;
                    let mut row =
                        TokenRow::decode(table.get(id.as_slice()).unwrap().unwrap().value())
                            .unwrap();
                    row.name = huge;
                    table
                        .insert(id.as_slice(), row.encode().as_slice())
                        .unwrap();
                }
                _ => unreachable!(),
            }
        });
        let app = router_over(store, 2);
        for route in [
            format!("/v1/txs/{}", hex::encode(history_id(302))),
            "/v1/blocks/2/txs".into(),
        ] {
            let (status, value) = get(&app, &route).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{kind}: {value}");
            assert_eq!(value["code"], "expansion_decode_limit");
        }
        assert_eq!(
            get(&app, "/v1/blocks/2/tx-summaries").await.0,
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn summary_block_empty_missing_range_and_exact_count_boundary() {
    use redb::ReadableTable;
    use xp_store::{
        keys::k_u32,
        rows::{HeaderRow, TxRow},
        tables::{HEADERS, TXS},
    };
    for case in ["empty", "missing", "max_count"] {
        let (_dir, store) = integrity_store(false, |tx| {
            if case == "max_count" {
                let mut table = tx.open_table(TXS).unwrap();
                let id = history_id(302);
                let mut row =
                    TxRow::decode(table.get(id.as_slice()).unwrap().unwrap().value()).unwrap();
                row.inputs = vec![[0; 32]; usize::from(u16::MAX)];
                row.data_inputs = vec![[0; 32]; usize::from(u16::MAX)];
                table
                    .insert(id.as_slice(), row.encode().as_slice())
                    .unwrap();
            } else {
                let mut table = tx.open_table(HEADERS).unwrap();
                let key = k_u32(2);
                let mut row =
                    HeaderRow::decode(table.get(key.as_slice()).unwrap().unwrap().value()).unwrap();
                row.tx_count = if case == "empty" { 0 } else { 2 };
                table
                    .insert(key.as_slice(), row.encode().as_slice())
                    .unwrap();
            }
        });
        let app = router_over(store, 2);
        let (status, value) = get(&app, "/v1/blocks/2/tx-summaries?dir=asc").await;
        match case {
            "empty" => {
                assert_eq!(status, StatusCode::OK);
                // M5 adds explicit best-effort metadata; retain exact whole-response
                // equality, the empty items/cursor and the unchanged legacy array check.
                assert_eq!(
                    value,
                    serde_json::json!({
                        "items": [], "next_cursor": null,
                        "consistency": "best_effort",
                        "observed_anchor": { "height": 2, "block_id": hex::encode(history_id(202)) },
                        "anchor": null, "next_snapshot": null
                    })
                );
                assert_eq!(get(&app, "/v1/blocks/2/txs").await.1, serde_json::json!([]));
            }
            "missing" => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(value["code"], "integrity_error");
            }
            "max_count" => {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(value["items"][0]["input_count"], u16::MAX);
                assert_eq!(value["items"][0]["data_input_count"], u16::MAX);
            }
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn register_capacity_exposes_exact_committed_count_with_read_admission() {
    let (_dir, app, state) = app_with_state(unlimited(), None);
    let (status, body) = get(&app, "/v1/register-capacity").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["entries"],
        state.store.register_index_entries().unwrap().to_string()
    );
    assert_eq!(body["ceiling"], serde_json::Value::Null);
    let _permits = state
        .read_permits
        .clone()
        .acquire_many_owned(32)
        .await
        .unwrap();
    assert_eq!(
        get(&app, "/v1/register-capacity").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn register_capacity_exposes_configured_ceiling_as_exact_decimal() {
    let (_dir, _app, mut state) = app_with_state(unlimited(), None);
    let dir = tempfile::tempdir().unwrap();
    state.store = Arc::new(
        Store::open_with_register_index_ceiling(&dir.path().join("x.redb"), Some(u64::MAX))
            .unwrap(),
    );
    let app = xp_api::router(state, &unlimited());
    let (status, body) = get(&app, "/v1/register-capacity").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"], "0");
    assert_eq!(body["ceiling"], u64::MAX.to_string());
}

// M5: every ordinary route/order uses the real router and disposable stores.
const M5_BASE: u32 = 1_100_000;

struct M5Fixture {
    dir: tempfile::TempDir,
    store: Arc<Store>,
    app: Router,
    blocks: Vec<xp_wire::DecodedBlock>,
    trees: Vec<Vec<u8>>,
    paths: Vec<(String, bool)>, // immutable membership
}

fn m5_router(store: Arc<Store>) -> Router {
    let (_, status) = watch::channel(IngestStatus {
        source_observed_at_ms: None,
        source_error: None,
        indexed: store.indexed_height().unwrap(),
        best: M5_BASE + 6,
        mode: Mode::Tip,
        source: "test".into(),
        halted: None,
        stalled: None,
    });
    let cfg = unlimited();
    xp_api::router(
        xp_api::AppState {
            store,
            status,
            counters: Arc::new(Counters::default()),
            read_permits: Arc::new(Semaphore::new(32)),
        },
        &cfg,
    )
}

fn m5_fixture() -> M5Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("m5.redb")).unwrap());
    store.seed_for_tests(M5_BASE, history_id(9000)).unwrap();
    let mut trees = vec![vec![0, 8, 0xd3]];
    for tx in fixture(1866000).txs {
        for out in tx.outputs {
            if !trees.contains(&out.tree_bytes) && xp_wire::tree_info(&out.tree_bytes).is_ok() {
                trees.push(out.tree_bytes);
            }
            if trees.len() == 6 {
                break;
            }
        }
        if trees.len() == 6 {
            break;
        }
    }
    assert_eq!(trees.len(), 6);
    let mut txs = Vec::new();
    for token in 0..6 {
        let outputs = trees
            .iter()
            .enumerate()
            .map(|(i, tree)| {
                let mut out = history_output(
                    10000 + token * 10 + i as u32,
                    if token == 0 {
                        (6 - i) as u64 * 1_000_000
                    } else {
                        100
                    },
                    vec![(history_id(20000 + token), (6 - i) as u64 * 100)],
                );
                out.tree_bytes = tree.clone();
                out.tree_hash = xp_wire::tree_hash(tree);
                out.registers_json = r#"{"R4":"0402"}"#.into();
                out
            })
            .collect();
        txs.push(history_tx(
            30000 + token,
            vec![xp_types::BoxId(history_id(20000 + token))],
            outputs,
        ));
    }
    // Insert nonmatching transactions between address-summary members. The final
    // sparse suffix is nonempty globally but contains no further tree[0] members.
    let mut sparse_txs = Vec::new();
    for (i, tx) in txs.into_iter().enumerate() {
        sparse_txs.push(tx);
        let mut out = history_output(50000 + i as u32, 10, vec![]);
        out.tree_bytes = trees[1].clone();
        out.tree_hash = xp_wire::tree_hash(&out.tree_bytes);
        out.registers_json = "{}".into();
        sparse_txs.push(history_tx(51000 + i as u32, vec![], vec![out]));
    }
    let txs = sparse_txs;
    let mut blocks = vec![history_block(M5_BASE + 1, 9001, history_id(9000), txs)];
    for h in 2..=6 {
        blocks.push(history_block(
            M5_BASE + h,
            9000 + h,
            history_id(8999 + h),
            vec![],
        ));
    }
    store.apply_batch(&blocks, true).unwrap();
    let info = xp_wire::tree_info(&trees[0]).unwrap();
    let token = hex::encode(history_id(20000));
    let mut paths = vec![
        ("/v1/blocks?".into(), true),
        ("/v1/tokens?sort=newest&".into(), false),
        ("/v1/tokens?sort=holders&".into(), false),
        (format!("/v1/tokens/{token}/holders?"), false),
        ("/v1/richlist?".into(), false),
        ("/v1/rent/eligible?".into(), false),
    ];
    for dir in ["asc", "desc"] {
        for (path, immutable) in [
            ("/v1/tx-summaries".into(), true),
            (format!("/v1/blocks/{}/tx-summaries", M5_BASE + 1), true),
            (format!("/v1/addresses/{}/txs", info.address), true),
            ("/v1/txs".into(), false),
            ("/v1/registers/R4/0402/boxes".into(), false),
        ] {
            paths.push((format!("{path}?dir={dir}&"), immutable));
        }
        for path in [
            format!("/v1/addresses/{}/boxes", info.address),
            format!("/v1/tokens/{token}/boxes"),
            format!("/v1/templates/{}/boxes", hex::encode(info.template_hash)),
        ] {
            for unspent in [false, true] {
                paths.push((format!("{path}?dir={dir}&unspent={unspent}&"), false));
            }
        }
    }
    let app = m5_router(store.clone());
    M5Fixture {
        dir,
        store,
        app,
        blocks,
        trees,
        paths,
    }
}

fn m5_next(path: &str, page: &Value, limit: usize) -> Option<String> {
    page["next_cursor"].as_str().map(|cursor| {
        let token = page["next_snapshot"]
            .as_str()
            .expect("strict page must pair its cursor with a snapshot");
        format!("{path}consistency=strict&limit={limit}&cursor={cursor}&snapshot={token}")
    })
}

async fn m5_walk(
    app: &Router,
    path: &str,
    limit: usize,
    first: Option<Value>,
) -> (Vec<Value>, usize) {
    let mut uri = format!("{path}consistency=strict&limit={limit}");
    let mut first = first;
    let mut items = vec![];
    let mut pages = 0;
    loop {
        let page = match first.take() {
            Some(page) => page,
            None => {
                let (status, page) = get(app, &uri).await;
                assert_eq!(status, StatusCode::OK, "{uri}: {page}");
                page
            }
        };
        pages += 1;
        assert!(pages < 100, "nonterminating walk: {path}");
        assert_eq!(page["consistency"], "strict", "{path}");
        let rows = page["items"].as_array().unwrap();
        assert!(rows.len() <= limit);
        items.extend(rows.clone());
        match m5_next(path, &page, limit) {
            Some(next) => {
                assert_eq!(rows.len(), limit, "exact limit: {path}");
                uri = next;
            }
            None => {
                assert!(page["next_snapshot"].is_null());
                break;
            }
        }
    }
    let unique: HashSet<_> = items.iter().map(Value::to_string).collect();
    assert_eq!(unique.len(), items.len(), "duplicate items: {path}");
    (items, pages)
}

#[tokio::test]
async fn m5_every_family_three_pages_both_directions_exact_limit_last_and_restart() {
    let mut f = m5_fixture();
    let mut saved = vec![];
    for (path, _) in &f.paths {
        let (expected, _) = m5_walk(&f.app, path, 500, None).await;
        let (actual, pages) = m5_walk(&f.app, path, 2, None).await;
        assert!(pages >= 3, "too few pages: {path}");
        assert_eq!(actual, expected, "page identity: {path}");
        let (status, first) = get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
        assert_eq!(status, StatusCode::OK);
        saved.push((path.clone(), first, expected));
    }
    // Close every Store/Router owner and reopen the database. No retained Reader.
    drop(f.app);
    drop(f.store);
    f.store = Arc::new(Store::open(&f.dir.path().join("m5.redb")).unwrap());
    f.app = m5_router(f.store.clone());
    for (path, first, expected) in saved {
        let (actual, pages) = m5_walk(&f.app, &path, 2, Some(first)).await;
        assert!(pages >= 3);
        assert_eq!(actual, expected, "restart: {path}");
    }
}

fn m5_reorder(f: &M5Fixture, id: u32) -> xp_wire::DecodedBlock {
    let inputs = f.blocks[0].txs[0].outputs.iter().map(|o| o.id).collect();
    let outputs = f
        .trees
        .iter()
        .enumerate()
        .map(|(i, tree)| {
            let mut out = history_output(
                40000 + i as u32,
                (i + 1) as u64 * 1_000_000,
                vec![(history_id(20000), (i + 1) as u64 * 100)],
            );
            out.tree_bytes = tree.clone();
            out.tree_hash = xp_wire::tree_hash(tree);
            out.registers_json = r#"{"R4":"0402"}"#.into();
            out
        })
        .collect();
    history_block(
        M5_BASE + 7,
        id,
        history_id(9006),
        vec![history_tx(id + 100, inputs, outputs)],
    )
}

async fn m5_conflict(app: &Router, uri: &str) {
    let (status, body) = get(app, uri).await;
    assert_eq!(status, StatusCode::CONFLICT, "STRICT SNAPSHOT REGRESSION: returning a page can silently mix rankings or fork membership: {uri}: {body}");
    assert_eq!(body["code"], "snapshot_changed");
    assert!(
        body.get("items").is_none(),
        "409 must not carry a plausible page"
    );
}

#[tokio::test]
async fn m5_apply_reorders_holders_and_richlist_current_pages_must_409_immutable_stays_identical() {
    let f = m5_fixture();
    let mut saved = vec![];
    for (path, immutable) in &f.paths {
        let (_, first) = get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
        let (expected, _) = m5_walk(&f.app, path, 2, Some(first.clone())).await;
        saved.push((path, immutable, first, expected));
    }
    f.store.apply_batch(&[m5_reorder(&f, 9007)], true).unwrap();
    for (path, immutable, first, expected) in saved {
        if *immutable {
            let uri = m5_next(path, &first, 2).unwrap();
            let (_, next) = get(&f.app, &uri).await;
            assert_eq!(next["anchor"], first["anchor"]);
            assert_ne!(next["observed_anchor"], first["observed_anchor"]);
            let (actual, _) = m5_walk(&f.app, path, 2, Some(first)).await;
            assert_eq!(
                actual, expected,
                "append introduced new items or duplicates: {path}"
            );
        } else {
            m5_conflict(&f.app, &m5_next(path, &first, 2).unwrap()).await;
            let (status, restarted) =
                get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
            assert_eq!(status, StatusCode::OK, "restart: {path}: {restarted}");
            if path.contains("/holders?") || path.contains("/richlist?") {
                assert_ne!(
                    restarted["items"][0]["tree_hash"], first["items"][0]["tree_hash"],
                    "fixture must ACTUALLY reorder ranking keys: {path}"
                );
            }
        }
    }
}

#[tokio::test]
async fn m5_replaced_anchor_and_forked_gidx_reuse_explicitly_invalidate_every_family() {
    let f = m5_fixture();
    let mut saved = vec![];
    for (path, _) in &f.paths {
        let (_, first) = get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
        saved.push(m5_next(path, &first, 2).unwrap());
    }
    // Replace the anchor at the identical height. All routes must reject its id.
    f.store.rollback_to(M5_BASE + 5).unwrap();
    for uri in &saved {
        m5_conflict(&f.app, uri).await;
    }
    f.store
        .apply_batch(
            &[history_block(M5_BASE + 6, 9906, history_id(9005), vec![])],
            true,
        )
        .unwrap();
    for uri in &saved {
        m5_conflict(&f.app, uri).await;
    }
    // Allocate transaction/box gidx, save pages, roll back and reuse the exact gidx.
    let mut append = m5_reorder(&f, 9907);
    append.header.parent_id = xp_types::HeaderId(history_id(9906));
    f.store.apply_batch(&[append.clone()], true).unwrap();
    let mut fork_pages = vec![];
    for (path, _) in &f.paths {
        let (_, first) = get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
        fork_pages.push(m5_next(path, &first, 2).unwrap());
    }
    let old_gidx = xp_store::Reader::new(&f.store)
        .unwrap()
        .tx_by_id(&append.txs[0].id.0)
        .unwrap()
        .unwrap()
        .gidx;
    f.store.rollback_to(M5_BASE + 6).unwrap();
    let mut fork = append.clone();
    fork.header.id = xp_types::HeaderId(history_id(9997));
    fork.txs[0].id = xp_types::TxId(history_id(9998));
    for out in &mut fork.txs[0].outputs {
        out.tx_id = xp_types::TxId(history_id(9998));
        out.id.0[31] ^= 1;
    }
    f.store.apply_batch(&[fork.clone()], true).unwrap();
    let rd = xp_store::Reader::new(&f.store).unwrap();
    assert_eq!(
        rd.tx_by_id(&fork.txs[0].id.0).unwrap().unwrap().gidx,
        old_gidx
    );
    assert!(rd.tx_by_id(&append.txs[0].id.0).unwrap().is_none());
    for uri in &fork_pages {
        m5_conflict(&f.app, uri).await;
    }
}

#[tokio::test]
async fn m5_http_binding_rejection_legacy_metadata_and_sparse_terminal_page() {
    let f = m5_fixture();
    for (path, _) in &f.paths {
        let (_, first) = get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
        let (_, second) = get(&f.app, &m5_next(path, &first, 2).unwrap()).await;
        for (token_page, cursor_page) in [(&first, &second), (&second, &first)] {
            let uri = format!(
                "{path}consistency=strict&limit=2&snapshot={}&cursor={}",
                token_page["next_snapshot"].as_str().unwrap(),
                cursor_page["next_cursor"].as_str().unwrap()
            );
            assert_eq!(
                get(&f.app, &uri).await.0,
                StatusCode::BAD_REQUEST,
                "cursor/token cross-pair: {path}"
            );
        }
        let cursor = first["next_cursor"].as_str().unwrap();
        for extra in [
            "consistency=strict&".to_string(),
            "consistency=unknown&".into(),
            format!("snapshot={}&", first["next_snapshot"].as_str().unwrap()),
        ] {
            assert_eq!(
                get(&f.app, &format!("{path}{extra}cursor={cursor}"))
                    .await
                    .0,
                StatusCode::BAD_REQUEST,
                "{path}: {extra}"
            );
        }
        assert_eq!(
            get(
                &f.app,
                &format!(
                    "{path}consistency=strict&snapshot={}",
                    first["next_snapshot"].as_str().unwrap()
                )
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let (status, legacy) = get(&f.app, &format!("{path}cursor={cursor}&limit=2")).await;
        assert_eq!(status, StatusCode::OK, "legacy: {path}: {legacy}");
        assert_eq!(legacy["items"], second["items"]);
        assert_eq!(legacy["consistency"], "best_effort");
        assert_eq!(legacy["observed_anchor"], first["observed_anchor"]);
        assert!(legacy["anchor"].is_null());
        assert!(legacy["next_snapshot"].is_null());
        if path.contains("dir=asc") || path.contains("dir=desc") {
            let changed = if path.contains("dir=asc") {
                path.replace("dir=asc", "dir=desc")
            } else {
                path.replace("dir=desc", "dir=asc")
            };
            assert_eq!(
                get(&f.app, &m5_next(&changed, &first, 2).unwrap()).await.0,
                StatusCode::BAD_REQUEST,
                "direction: {path}"
            );
        }
        if path.contains("unspent=") {
            let changed = if path.contains("unspent=true") {
                path.replace("unspent=true", "unspent=false")
            } else {
                path.replace("unspent=false", "unspent=true")
            };
            assert_eq!(
                get(&f.app, &m5_next(&changed, &first, 2).unwrap()).await.0,
                StatusCode::BAD_REQUEST,
                "unspent: {path}"
            );
        }
    }
    let token0 = hex::encode(history_id(20000));
    let token1 = hex::encode(history_id(20001));
    let a0 = xp_wire::tree_info(&f.trees[0]).unwrap();
    let a1 = xp_wire::tree_info(&f.trees[1]).unwrap();
    for (a, b) in [
        (
            format!("/v1/tokens/{token0}/holders?"),
            format!("/v1/tokens/{token1}/holders?"),
        ),
        (
            format!("/v1/tokens/{token0}/boxes?"),
            format!("/v1/tokens/{token1}/boxes?"),
        ),
        (
            format!("/v1/addresses/{}/boxes?", a0.address),
            format!("/v1/addresses/{}/boxes?", a1.address),
        ),
        (
            format!("/v1/addresses/{}/txs?", a0.address),
            format!("/v1/addresses/{}/txs?", a1.address),
        ),
        (
            format!("/v1/templates/{}/boxes?", hex::encode(a0.template_hash)),
            format!("/v1/templates/{}/boxes?", hex::encode(a1.template_hash)),
        ),
        ("/v1/tx-summaries?".into(), "/v1/txs?".into()),
    ] {
        for (source, target) in [(&a, &b), (&b, &a)] {
            let (_, first) = get(&f.app, &format!("{source}consistency=strict&limit=2")).await;
            assert_eq!(
                get(&f.app, &m5_next(target, &first, 2).unwrap()).await.0,
                StatusCode::BAD_REQUEST,
                "entity/route binding: {source} -> {target}"
            );
        }
    }
    for (source, targets) in [
        (
            "/v1/registers/R4/0402/boxes?".to_string(),
            vec![
                "/v1/registers/R5/0402/boxes?".into(),
                "/v1/registers/R4/0404/boxes?".into(),
            ],
        ),
        (
            format!("/v1/blocks/{}/tx-summaries?", M5_BASE + 1),
            vec![format!("/v1/blocks/{}/tx-summaries?", M5_BASE + 2)],
        ),
        (
            "/v1/tokens?sort=newest&".into(),
            vec!["/v1/tokens?sort=holders&".into()],
        ),
        (
            "/v1/tokens?sort=holders&".into(),
            vec!["/v1/tokens?sort=newest&".into()],
        ),
    ] {
        let (_, first) = get(&f.app, &format!("{source}consistency=strict&limit=2")).await;
        for target in targets {
            assert_eq!(
                get(&f.app, &m5_next(&target, &first, 2).unwrap()).await.0,
                StatusCode::BAD_REQUEST,
                "filter: {source} -> {target}"
            );
        }
    }
    for (source, missing) in [
        (
            format!("/v1/addresses/{}/txs?", a0.address),
            "/v1/addresses/unknown/txs?".to_string(),
        ),
        (
            format!("/v1/addresses/{}/boxes?", a0.address),
            "/v1/addresses/unknown/boxes?".to_string(),
        ),
        (
            format!("/v1/blocks/{}/tx-summaries?", M5_BASE + 1),
            "/v1/blocks/4294967295/tx-summaries?".to_string(),
        ),
    ] {
        let (_, first) = get(&f.app, &format!("{source}consistency=strict&limit=2")).await;
        assert_eq!(
            get(&f.app, &m5_next(&missing, &first, 2).unwrap()).await.0,
            StatusCode::BAD_REQUEST,
            "changed selector: {missing}"
        );
        assert_eq!(
            get(&f.app, &format!("{missing}consistency=strict")).await.0,
            StatusCode::NOT_FOUND,
            "unknown initial selector keeps its 404: {missing}"
        );
    }
    // Block height and canonical id normalize to the same binding.
    let path = format!("/v1/blocks/{}/tx-summaries?", M5_BASE + 1);
    let (_, first) = get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
    let id_path = format!("/v1/blocks/{}/tx-summaries?", hex::encode(history_id(9001)));
    assert_eq!(
        get(&f.app, &m5_next(&id_path, &first, 2).unwrap()).await.0,
        StatusCode::OK
    );
    // Six sparse address members end at gidx 10, below allocation end 12.
    let path = format!("/v1/addresses/{}/txs?dir=asc&", a0.address);
    let (_, first) = get(&f.app, &format!("{path}consistency=strict&limit=6")).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 6);
    let next = m5_next(&path, &first, 6).unwrap();
    f.store.apply_batch(&[m5_reorder(&f, 9007)], true).unwrap();
    let (status, empty) = get(&f.app, &next).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["items"], serde_json::json!([]));
    assert!(empty["next_cursor"].is_null());
    assert!(empty["next_snapshot"].is_null());
    assert_eq!(empty["anchor"], first["anchor"]);
}

#[tokio::test]
async fn m5_reorg_removing_selected_entities_and_rebinding_block_height_is_409() {
    let f = m5_fixture();
    f.store.rollback_to(M5_BASE + 1).unwrap();
    let mut continuations = vec![];
    for (path, _) in &f.paths {
        if path.starts_with("/v1/blocks?") {
            continue;
        } // only two headers at this tip
        let (_, first) = get(&f.app, &format!("{path}consistency=strict&limit=2")).await;
        continuations.push(m5_next(path, &first, 2).unwrap());
    }
    let id_path = format!("/v1/blocks/{}/tx-summaries?", hex::encode(history_id(9001)));
    let (_, first) = get(&f.app, &format!("{id_path}consistency=strict&limit=2")).await;
    continuations.push(m5_next(&id_path, &first, 2).unwrap());
    f.store.rollback_to(M5_BASE).unwrap();
    for uri in &continuations {
        m5_conflict(&f.app, uri).await;
    }
    let mut replacement = f.blocks[0].clone();
    replacement.header.id = xp_types::HeaderId(history_id(9991));
    f.store.apply_batch(&[replacement], true).unwrap();
    for uri in &continuations {
        m5_conflict(&f.app, uri).await;
    }
}

#[tokio::test]
async fn m5_empty_genesis_and_headerless_partial_snapshots_do_not_invent_anchors() {
    for kind in ["empty", "genesis", "partial", "corrupt_partial"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unavailable.redb");
        {
            let store = Store::open(&path).unwrap();
            if kind == "genesis" {
                store.seed_genesis(&mainnet_genesis()).unwrap();
            }
        }
        if kind == "partial" || kind == "corrupt_partial" {
            use xp_store::{
                keys::k_u32,
                tables::{META, META_INDEXED_HEIGHT, META_NEXT_TX_GIDX, META_PARTIAL_FROM},
            };
            let db = redb::Database::create(&path).unwrap();
            let tx = db.begin_write().unwrap();
            {
                let mut meta = tx.open_table(META).unwrap();
                meta.insert(META_INDEXED_HEIGHT, k_u32(20).as_slice())
                    .unwrap();
                meta.insert(
                    META_PARTIAL_FROM,
                    k_u32(if kind == "partial" { 21 } else { 20 }).as_slice(),
                )
                .unwrap();
                meta.insert(META_NEXT_TX_GIDX, xp_store::keys::k_u64(0).as_slice())
                    .unwrap();
            }
            tx.commit().unwrap();
        }
        let app = m5_router(Arc::new(Store::open(&path).unwrap()));
        for path in [
            "/v1/blocks",
            "/v1/tx-summaries",
            "/v1/txs",
            "/v1/tokens",
            "/v1/richlist",
            "/v1/registers/R4/0402/boxes",
            "/v1/rent/eligible",
        ] {
            let (status, page) = get(&app, &format!("{path}?consistency=strict&limit=1")).await;
            // A missing in-range header remains corruption (M2), never an empty
            // snapshot. Only metadata below partial_from permits a headerless seed.
            if kind == "corrupt_partial" {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(page["code"], "integrity_error");
                assert!(page.get("anchor").is_none());
                continue;
            }
            assert_eq!(status, StatusCode::OK, "{kind}: {path}: {page}");
            assert_eq!(page["consistency"], "strict");
            for key in ["anchor", "observed_anchor", "next_snapshot"] {
                assert!(page[key].is_null(), "{kind}: {path}: {page}");
            }
        }
    }
}

// A pre-M5 consumer knows only these fields; serde must ignore all strict additions.
#[tokio::test]
async fn m5_pre_m5_client_parses_legacy_pages_and_strict_amounts_and_ids() {
    #[derive(serde::Deserialize)]
    struct LegacyPage {
        items: Vec<LegacyRichItem>,
        next_cursor: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct LegacyRichItem {
        address: Option<String>,
        tree_hash: String,
        nano: String,
    }
    let f = m5_fixture();
    for strict in [false, true] {
        let query = if strict { "&consistency=strict" } else { "" };
        let (status, body) = get(&f.app, &format!("/v1/richlist?limit=2{query}")).await;
        assert_eq!(status, StatusCode::OK);
        let page: LegacyPage = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(page.items.len(), 2);
        for item in &page.items {
            assert!(item.address.is_some());
            assert_eq!(item.tree_hash.len(), 64);
            assert!(item
                .tree_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
            assert!(item.nano.bytes().all(|b| b.is_ascii_digit()));
            assert!(item.nano.parse::<u64>().unwrap() > 0);
        }
        let cursor = page.next_cursor.unwrap();
        let suffix = if strict {
            format!(
                "&consistency=strict&snapshot={}",
                body["next_snapshot"].as_str().unwrap()
            )
        } else {
            String::new()
        };
        // This is the unchanged pre-M5 request shape in the legacy iteration.
        let (status, next) = get(
            &f.app,
            &format!("/v1/richlist?limit=2&cursor={cursor}{suffix}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let next: LegacyPage = serde_json::from_value(next).unwrap();
        assert_eq!(next.items.len(), 2);
        assert_ne!(page.items[0].tree_hash, next.items[0].tree_hash);
    }
}

#[tokio::test]
async fn m5_strict_expanded_wire_preserves_decimal_amounts_and_hex_ids() {
    let f = m5_fixture();
    let (status, body) = get(&f.app, "/v1/txs?dir=asc&limit=1&consistency=strict").await;
    assert_eq!(status, StatusCode::OK);
    let tx = &body["items"][0];
    let hex_id = |value: &serde_json::Value| {
        let value = value.as_str().expect("id must remain a JSON string");
        assert_eq!(value.len(), 64);
        assert!(value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    };
    let decimal = |value: &serde_json::Value| {
        let value = value.as_str().expect("amount must remain a JSON string");
        assert!(!value.is_empty());
        assert!(value.bytes().all(|b| b.is_ascii_digit()));
        value.parse::<u64>().unwrap();
    };
    hex_id(&tx["id"]);
    decimal(&tx["fee"]);
    let output = &tx["outputs"][0];
    hex_id(&output["id"]);
    hex_id(&output["tx_id"]);
    decimal(&output["value"]);
    hex_id(&output["tokens"][0]["id"]);
    decimal(&output["tokens"][0]["amount"]);
}
