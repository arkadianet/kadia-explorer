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
