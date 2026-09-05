use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use std::collections::HashMap;
use std::sync::Arc;
use xp_source::{BlockSource, RustNode};
use xp_wire::decode_block;

fn fixture(h: u32) -> String {
    std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/blocks/{h}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

struct Fixtures {
    /// height -> raw block json
    by_height: HashMap<u32, String>,
    /// header id hex -> raw block json
    by_id: HashMap<String, String>,
    tip: u32,
}

async fn info(state: axum::extract::State<Arc<Fixtures>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "fullHeight": state.tip }))
}

async fn blocks_at(
    state: axum::extract::State<Arc<Fixtures>>,
    Path(height): Path<u32>,
) -> Json<Vec<String>> {
    match state.by_height.get(&height) {
        Some(json) => {
            let b = decode_block(json).unwrap();
            Json(vec![xp_types::hex32(&b.header.id.0)])
        }
        None => Json(vec![]),
    }
}

async fn block_by_id(
    state: axum::extract::State<Arc<Fixtures>>,
    Path(id): Path<String>,
) -> Response {
    match state.by_id.get(&id) {
        Some(json) => json.clone().into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn spawn_server() -> String {
    let heights = [1866000u32, 1866001, 1866002];
    let mut by_height = HashMap::new();
    let mut by_id = HashMap::new();
    for h in heights {
        let json = fixture(h);
        let b = decode_block(&json).unwrap();
        by_id.insert(xp_types::hex32(&b.header.id.0), json.clone());
        by_height.insert(h, json);
    }
    let state = Arc::new(Fixtures {
        by_height,
        by_id,
        tip: 1866002,
    });

    let app = Router::new()
        .route("/info", get(info))
        .route("/blocks/at/{height}", get(blocks_at))
        .route("/blocks/{id}", get(block_by_id))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn best_height_reads_full_height() {
    let base = spawn_server().await;
    let node = RustNode::new(&base);
    assert_eq!(node.best_height().await.unwrap(), 1866002);
}

#[tokio::test]
async fn header_id_at_matches_fixture() {
    let base = spawn_server().await;
    let node = RustNode::new(&base);

    let expected = decode_block(&fixture(1866001)).unwrap().header.id;
    let got = node.header_id_at(1866001).await.unwrap();
    assert_eq!(got, Some(expected.0));
}

#[tokio::test]
async fn header_id_at_unknown_height_is_none() {
    let base = spawn_server().await;
    let node = RustNode::new(&base);
    assert_eq!(node.header_id_at(1).await.unwrap(), None);
}

#[tokio::test]
async fn full_block_json_decodes_and_has_right_height() {
    let base = spawn_server().await;
    let node = RustNode::new(&base);

    let id = decode_block(&fixture(1866000)).unwrap().header.id;
    let body = node.full_block_json(&id.0).await.unwrap().unwrap();
    let decoded = decode_block(&body).unwrap();
    assert_eq!(decoded.header.height, 1866000);
}

#[tokio::test]
async fn full_block_json_unknown_id_is_none() {
    let base = spawn_server().await;
    let node = RustNode::new(&base);
    let unknown = [0xabu8; 32];
    assert_eq!(node.full_block_json(&unknown).await.unwrap(), None);
}

#[tokio::test]
async fn unreachable_base_yields_unavailable_or_http_error() {
    // Port 1 is a reserved low port with nothing listening; connection should be refused
    // (or time out) rather than hang. Either SourceError::Unavailable or SourceError::Http
    // is acceptable here — we document Unavailable as the expected mapping for a refused
    // connection, but don't over-assert on reqwest's exact error classification.
    let node = RustNode::new("http://127.0.0.1:1");
    let err = node.best_height().await.unwrap_err();
    match err {
        xp_source::SourceError::Unavailable | xp_source::SourceError::Http(_) => {}
        other => panic!("unexpected error variant: {other:?}"),
    }
}
