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

/// Even a single stored id cannot establish canonicality without `chainSlice`.
#[tokio::test]
async fn missing_chain_slice_with_single_id_is_a_capability_error() {
    assert_unsupported(vec![xp_types::hex32(&[0x11; 32])]).await;
}

#[tokio::test]
async fn missing_chain_slice_with_orphan_first_is_a_capability_error() {
    assert_unsupported(vec![
        xp_types::hex32(&[0x11; 32]),
        xp_types::hex32(&[0x22; 32]),
    ])
    .await;
}

async fn assert_unsupported(ids: Vec<String>) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let app = Router::new().route(
        "/blocks/at/{height}",
        get(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            let ids = ids.clone();
            async move { Json(ids) }
        }),
    );
    let base = serve(app).await;
    let err = RustNode::new(&base)
        .header_id_at(1866000)
        .await
        .unwrap_err();
    assert!(matches!(err, xp_source::SourceError::Capability(_)));
    assert!(err
        .to_string()
        .contains("upgrade this node or configure a primary"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unsupported_chain_slice_statuses_are_capability_errors() {
    for status in [
        axum::http::StatusCode::METHOD_NOT_ALLOWED,
        axum::http::StatusCode::NOT_IMPLEMENTED,
    ] {
        let base =
            serve(Router::new().route("/blocks/chainSlice", get(move || async move { status })))
                .await;
        assert!(matches!(
            RustNode::new(&base).header_id_at(42).await,
            Err(xp_source::SourceError::Capability(_))
        ));
    }
}

#[tokio::test]
async fn malformed_and_contradictory_chain_slice_are_decode_errors() {
    for body in [
        "not json".to_owned(),
        "{}".to_owned(),
        serde_json::json!([{"height": 42, "id": "bad"}]).to_string(),
        serde_json::json!([{"height": 42}]).to_string(),
        serde_json::json!([{"id": xp_types::hex32(&[1; 32])}]).to_string(),
        serde_json::json!([
            {"height": 42, "id": xp_types::hex32(&[1; 32])},
            {"height": 42, "id": xp_types::hex32(&[2; 32])}
        ])
        .to_string(),
    ] {
        let base = serve(Router::new().route(
            "/blocks/chainSlice",
            get(move || {
                let body = body.clone();
                async move { body }
            }),
        ))
        .await;
        assert!(matches!(
            RustNode::new(&base).header_id_at(42).await,
            Err(xp_source::SourceError::Decode(_))
        ));
    }
}

const PENDING_ROUTE: &str = "/transactions/unconfirmed/byTransactionId/{id}";

#[tokio::test]
async fn pending_lookup_uses_the_read_only_id_route() {
    let expected = [0x42; 32];
    let body = serde_json::json!({
        "id": xp_types::hex32(&expected), "inputs": [], "dataInputs": [], "outputs": []
    });
    let served = body.clone();
    let base = serve(Router::new().route(
        PENDING_ROUTE,
        get(move |Path(id): Path<String>| {
            assert_eq!(id, xp_types::hex32(&expected));
            let body = served.clone();
            async move { Json(body) }
        }),
    ))
    .await;
    let received = RustNode::new(&base)
        .unconfirmed_transaction_json(&expected)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&received).unwrap(),
        body
    );
}

#[tokio::test]
async fn pending_absence_requires_the_nodes_transaction_not_found_response() {
    for detail in [
        serde_json::Value::Null,
        serde_json::json!("unconfirmed transaction not found"),
    ] {
        let body = serde_json::json!({"error":404, "reason":"not-found", "detail":detail});
        let base = serve(Router::new().route(
            PENDING_ROUTE,
            get(move || {
                let body = body.clone();
                async move { (axum::http::StatusCode::NOT_FOUND, Json(body)) }
            }),
        ))
        .await;
        assert_eq!(
            RustNode::new(&base)
                .unconfirmed_transaction_json(&[0; 32])
                .await
                .unwrap(),
            None
        );
    }
}

#[tokio::test]
async fn pending_unsupported_and_proxy_404s_are_not_transaction_absence() {
    use axum::http::StatusCode;
    for (status, body) in [
        (StatusCode::NOT_FOUND, ""),
        (StatusCode::NOT_FOUND, "<h1>Not found</h1>"),
        (
            StatusCode::NOT_FOUND,
            r#"{"error":404,"reason":"not-found","detail":"route not found"}"#,
        ),
        (
            StatusCode::NOT_FOUND,
            r#"{"error":404,"reason":"not-found"}"#,
        ),
        (StatusCode::METHOD_NOT_ALLOWED, ""),
        (StatusCode::NOT_IMPLEMENTED, ""),
    ] {
        let base =
            serve(Router::new().route(PENDING_ROUTE, get(move || async move { (status, body) })))
                .await;
        assert!(matches!(
            RustNode::new(&base)
                .unconfirmed_transaction_json(&[0; 32])
                .await,
            Err(xp_source::SourceError::Capability(_))
        ));
    }
}

#[tokio::test]
async fn pending_outage_and_rate_limit_are_errors() {
    use axum::http::StatusCode;
    for status in [
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::FORBIDDEN,
    ] {
        let base =
            serve(Router::new().route(PENDING_ROUTE, get(move || async move { status }))).await;
        assert!(matches!(
            RustNode::new(&base)
                .unconfirmed_transaction_json(&[0; 32])
                .await,
            Err(xp_source::SourceError::Http(_))
        ));
    }
    assert!(matches!(
        RustNode::new("http://127.0.0.1:1")
            .unconfirmed_transaction_json(&[0; 32])
            .await,
        Err(xp_source::SourceError::Unavailable | xp_source::SourceError::Http(_))
    ));
}

#[tokio::test]
async fn pending_chunked_body_stops_at_two_mib() {
    use axum::body::{Body, Bytes};
    let base = serve(Router::new().route(
        PENDING_ROUTE,
        get(|| async {
            let chunks =
                (0..33).map(|_| Ok::<_, std::convert::Infallible>(Bytes::from(vec![b'x'; 65536])));
            Body::from_stream(futures::stream::iter(chunks))
        }),
    ))
    .await;
    let err = RustNode::new(&base)
        .unconfirmed_transaction_json(&[0; 32])
        .await
        .unwrap_err();
    assert!(matches!(err, xp_source::SourceError::Decode(_)));
    assert!(err.to_string().contains("2097152 bytes"));
}

#[tokio::test]
async fn bounded_block_reads_limit_decompressed_bytes_and_leave_ingest_unchanged() {
    // Gzip of exactly 1,024 ASCII x bytes. Its 29 wire bytes fit below the test limit.
    let compressed =
        hex::decode("1f8b080000000000000aaba81805a36014548c50000063f0d74800040000").unwrap();
    let base = serve(Router::new().route(
        "/blocks/{id}",
        get(move || {
            let compressed = compressed.clone();
            async move { ([(axum::http::header::CONTENT_ENCODING, "gzip")], compressed) }
        }),
    ))
    .await;
    let source = RustNode::new(&base);
    assert!(matches!(
        source.full_block_json_bounded(&[0; 32], 1000).await,
        Err(xp_source::SourceError::Decode(_))
    ));
    assert_eq!(
        source
            .full_block_json_bounded(&[0; 32], 1024)
            .await
            .unwrap(),
        Some("x".repeat(1024))
    );
    assert_eq!(
        source.full_block_json(&[0; 32]).await.unwrap(),
        Some("x".repeat(1024))
    );
}

#[tokio::test]
async fn optional_block_limit_does_not_cap_ingestion_at_sixteen_mib() {
    let size = 16 * 1024 * 1024 + 1;
    let base =
        serve(Router::new().route("/blocks/{id}", get(move || async move { "x".repeat(size) })))
            .await;
    let source = RustNode::new(&base);
    assert!(matches!(
        source
            .full_block_json_bounded(&[0; 32], 16 * 1024 * 1024)
            .await,
        Err(xp_source::SourceError::Decode(_))
    ));
    assert_eq!(
        source
            .full_block_json(&[0; 32])
            .await
            .unwrap()
            .unwrap()
            .len(),
        size
    );
}
