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

/// `/blocks/chainSlice` — the best-chain headers in `(from, to]`, except that `from == to`
/// returns the single header at that height (verified against a live node).
async fn chain_slice(
    state: axum::extract::State<Arc<Fixtures>>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Json<Vec<serde_json::Value>> {
    let to: u32 = q.get("toHeight").and_then(|v| v.parse().ok()).unwrap_or(0);
    // A real node clamps a range that runs past its tip rather than answering empty, so a
    // request above the tip comes back with the *tip's* header. `header_id_at` must reject
    // that on height rather than hand back an id from the wrong height.
    let at = to.min(state.tip);
    match state.by_height.get(&at) {
        Some(json) => {
            let v: serde_json::Value = serde_json::from_str(json).unwrap();
            Json(vec![v.get("header").unwrap().clone()])
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

/// Binds an ephemeral port, serves `app` on it, and returns its base URL.
async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
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
        .route("/blocks/chainSlice", get(chain_slice))
        .route("/blocks/{id}", get(block_by_id))
        .with_state(state);
    serve(app).await
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
    // Above the node's tip the mock clamps to the tip header, as a real node does. Answering
    // with that id would make ingest fetch the tip's body for a height it does not belong to.
    assert_eq!(node.header_id_at(1866007).await.unwrap(), None);
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

/// A node holding competing blocks lists several ids at `/blocks/at/{h}`, and the orphan can
/// come first. `chainSlice` names the best-chain header for the height, so that is what the
/// source must return — taking `/blocks/at`'s first id applied an orphan and wedged the sync.
#[tokio::test]
async fn header_id_at_prefers_the_chain_slice_header_over_blocks_at() {
    let orphan = [0x11u8; 32];
    let best = [0x22u8; 32];
    let ids = vec![xp_types::hex32(&orphan), xp_types::hex32(&best)];
    let slice = serde_json::json!([{ "height": 1789057, "id": xp_types::hex32(&best) }]);
    let app = Router::new()
        .route(
            "/blocks/at/{height}",
            get(move || {
                let ids = ids.clone();
                async move { Json(ids) }
            }),
        )
        .route(
            "/blocks/chainSlice",
            get(move || {
                let slice = slice.clone();
                async move { Json(slice) }
            }),
        );
    let base = serve(app).await;

    let node = RustNode::new(&base);
    assert_eq!(node.header_id_at(1789057).await.unwrap(), Some(best));
}

/// `chainSlice` above a node's own tip answers with an empty array — no header, not an error.
#[tokio::test]
async fn header_id_at_empty_chain_slice_is_none() {
    let app = Router::new().route(
        "/blocks/chainSlice",
        get(|| async { Json(Vec::<serde_json::Value>::new()) }),
    );
    let base = serve(app).await;
    assert_eq!(RustNode::new(&base).header_id_at(9).await.unwrap(), None);
}

/// A `chainSlice` that answers with headers but none at the height asked for (a node whose
/// range semantics differ) is "no header here", not a wrong id from a neighbouring height.
#[tokio::test]
async fn header_id_at_ignores_chain_slice_headers_at_other_heights() {
    let slice = serde_json::json!([{ "height": 41, "id": xp_types::hex32(&[0x33u8; 32]) }]);
    let app = Router::new().route(
        "/blocks/chainSlice",
        get(move || {
            let slice = slice.clone();
            async move { Json(slice) }
        }),
    );
    let base = serve(app).await;
    assert_eq!(RustNode::new(&base).header_id_at(42).await.unwrap(), None);
}

/// A `chainSlice` that fails for any reason other than "no such endpoint" must surface as a
/// transient error. Degrading to `/blocks/at`'s first id on, say, a 503 would silently
/// reintroduce the orphan bug on a node that does have the endpoint.
#[tokio::test]
async fn header_id_at_chain_slice_server_error_is_an_error_not_a_fallback() {
    let ids = vec![xp_types::hex32(&[0x11u8; 32])];
    let app = Router::new()
        .route(
            "/blocks/at/{height}",
            get(move || {
                let ids = ids.clone();
                async move { Json(ids) }
            }),
        )
        .route(
            "/blocks/chainSlice",
            get(|| async { axum::http::StatusCode::SERVICE_UNAVAILABLE }),
        );
    let base = serve(app).await;

    let err = RustNode::new(&base)
        .header_id_at(1866000)
        .await
        .unwrap_err();
    match err {
        xp_source::SourceError::Http(msg) => assert!(msg.contains("503"), "got {msg}"),
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// Older nodes have no `chainSlice` endpoint at all. Then — and only then — the source falls
/// back to `/blocks/at/{h}` and its first-id convention.
#[tokio::test]
async fn header_id_at_falls_back_to_blocks_at_when_chain_slice_is_missing() {
    let first = [0x11u8; 32];
    let second = [0x22u8; 32];
    let ids = vec![xp_types::hex32(&first), xp_types::hex32(&second)];
    // No `/blocks/chainSlice` route: the router answers 404, as an older node would.
    let app = Router::new().route(
        "/blocks/at/{height}",
        get(move || {
            let ids = ids.clone();
            async move { Json(ids) }
        }),
    );
    let base = serve(app).await;

    let node = RustNode::new(&base);
    assert_eq!(node.header_id_at(1866000).await.unwrap(), Some(first));
}
