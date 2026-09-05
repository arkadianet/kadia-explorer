//! `Fallback` covers the one failure the primary node actually shows: `/blocks/{id}` 404s for
//! a block whose header the same node happily announces. Everything else — the tip, the
//! height→id mapping, the genesis boxes — must keep coming from the primary alone, so the
//! chain the index follows is still the primary's chain.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use std::collections::HashMap;
use std::sync::Arc;
use xp_source::{BlockSource, Fallback, RustNode};
use xp_wire::decode_block;

const HEIGHTS: [u32; 3] = [1866000, 1866001, 1866002];
/// The height whose body the primary refuses to serve.
const HOLE: u32 = 1866001;

fn fixture(h: u32) -> String {
    std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/blocks/{h}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn id_of(h: u32) -> String {
    xp_types::hex32(&decode_block(&fixture(h)).unwrap().header.id.0)
}

struct Node {
    by_height: HashMap<u32, String>,
    by_id: HashMap<String, String>,
    tip: u32,
    genesis: &'static str,
}

async fn info(State(s): State<Arc<Node>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "fullHeight": s.tip }))
}

async fn blocks_at(State(s): State<Arc<Node>>, Path(height): Path<u32>) -> Json<Vec<String>> {
    match s.by_height.get(&height) {
        Some(id) => Json(vec![id.clone()]),
        None => Json(vec![]),
    }
}

async fn block_by_id(State(s): State<Arc<Node>>, Path(id): Path<String>) -> Response {
    match s.by_id.get(&id) {
        Some(json) => json.clone().into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn genesis(State(s): State<Arc<Node>>) -> Response {
    s.genesis.into_response()
}

async fn spawn(node: Node) -> String {
    let app = Router::new()
        .route("/info", get(info))
        .route("/blocks/at/{height}", get(blocks_at))
        .route("/blocks/{id}", get(block_by_id))
        .route("/utxo/genesis", get(genesis))
        .with_state(Arc::new(node));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Primary: knows every header, serves every body except `HOLE`'s.
/// Fallback: serves only `HOLE`'s body, and lies about the tip and the genesis boxes so any
/// delegation leak shows up as a wrong value rather than an accidentally-correct one.
async fn pair() -> (String, String) {
    let mut p = Node {
        by_height: HashMap::new(),
        by_id: HashMap::new(),
        tip: 1866002,
        genesis: "[\"primary\"]",
    };
    for h in HEIGHTS {
        p.by_height.insert(h, id_of(h));
        if h != HOLE {
            p.by_id.insert(id_of(h), fixture(h));
        }
    }
    let f = Node {
        by_height: HashMap::new(),
        by_id: HashMap::from([(id_of(HOLE), fixture(HOLE))]),
        tip: 9_999_999,
        genesis: "[\"fallback\"]",
    };
    (spawn(p).await, spawn(f).await)
}

fn fallback_source(primary_url: &str, fallback_url: &str) -> Fallback {
    let primary: Arc<dyn BlockSource> = Arc::new(RustNode::new(primary_url));
    Fallback::new(primary, RustNode::new(fallback_url))
}

#[tokio::test]
async fn body_missing_on_the_primary_comes_from_the_fallback() {
    let (p, f) = pair().await;
    let src = fallback_source(&p, &f);

    let hole_id = xp_types::parse_hex32(&id_of(HOLE)).unwrap();
    let body = src
        .full_block_json(&hole_id)
        .await
        .unwrap()
        .expect("fallback serves the body the primary 404s");
    let decoded = decode_block(&body).unwrap();
    assert_eq!(decoded.header.height, HOLE);
    // Fetched *by id*, so the body can only be the block the primary announced.
    assert_eq!(decoded.header.id.0, hole_id);
}

#[tokio::test]
async fn bodies_the_primary_has_are_not_routed_to_the_fallback() {
    let (p, f) = pair().await;
    let src = fallback_source(&p, &f);
    let id = xp_types::parse_hex32(&id_of(1866000)).unwrap();
    let body = src.full_block_json(&id).await.unwrap().unwrap();
    assert_eq!(decode_block(&body).unwrap().header.height, 1866000);
}

#[tokio::test]
async fn unknown_to_both_is_none() {
    let (p, f) = pair().await;
    let src = fallback_source(&p, &f);
    assert_eq!(src.full_block_json(&[0xcd; 32]).await.unwrap(), None);
}

#[tokio::test]
async fn tip_headers_and_genesis_come_from_the_primary_only() {
    let (p, f) = pair().await;
    let src = fallback_source(&p, &f);

    assert_eq!(src.best_height().await.unwrap(), 1866002);
    assert_eq!(
        src.header_id_at(HOLE).await.unwrap(),
        Some(xp_types::parse_hex32(&id_of(HOLE)).unwrap())
    );
    // The fallback knows no headers; asking it would have produced `Some(...)` from its own
    // (empty) map, i.e. `None` — this asserts the primary answered.
    assert_eq!(src.genesis_boxes_json().await.unwrap(), "[\"primary\"]");
}

#[tokio::test]
async fn name_marks_the_fallback() {
    let (p, f) = pair().await;
    let src = fallback_source(&p, &f);
    assert_eq!(src.name(), format!("{p} (+fallback)"));
}

/// A primary that is not merely missing the body but unreachable still gets covered: the
/// fallback answers, so a transient primary outage does not stall ingest either.
#[tokio::test]
async fn primary_error_also_falls_back() {
    let (_p, f) = pair().await;
    let src = fallback_source("http://127.0.0.1:1", &f);
    let hole_id = xp_types::parse_hex32(&id_of(HOLE)).unwrap();
    let body = src.full_block_json(&hole_id).await.unwrap().unwrap();
    assert_eq!(decode_block(&body).unwrap().header.height, HOLE);
}
