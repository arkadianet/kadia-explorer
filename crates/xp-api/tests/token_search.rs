//! Exercise discovery through the real router and canonical apply/rollback paths.
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
use xp_store::Store;
use xp_types::{BoxId, HeaderId, TxId};

fn block(names: &[&str], height: u32, parent: [u8; 32]) -> xp_wire::DecodedBlock {
    let mut b =
        xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/453051.json")).unwrap();
    let mint = b.txs[1].clone();
    b.header.height = height;
    b.header.id = HeaderId([height as u8; 32]);
    b.header.parent_id = HeaderId(parent);
    b.txs = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            assert!(name.len() < 128);
            let id = [i as u8 + 1; 32];
            let mut tx = mint.clone();
            tx.id = TxId([i as u8 + 40; 32]);
            tx.inputs = vec![BoxId(id)];
            tx.data_inputs.clear();
            tx.outputs.truncate(1);
            let out = &mut tx.outputs[0];
            out.id = BoxId([i as u8 + 80; 32]);
            out.tx_id = tx.id;
            out.creation_height = height;
            out.tokens = vec![(id, 10_000_000_000_001)];
            out.registers_json =
                json!({"R4": format!("0e{:02x}{}", name.len(), hex::encode(name)), "R6":"0e0132"})
                    .to_string();
            tx
        })
        .collect();
    b
}

fn app(ready: bool) -> (tempfile::TempDir, Arc<Store>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("names.redb")).unwrap());
    store.seed_for_tests(100, [100; 32]).unwrap();
    store
        .apply_batch(
            &[block(
                &["SigUSD", " SIGusd ", "SigUSD Plus", "123", ""],
                101,
                [100; 32],
            )],
            true,
        )
        .unwrap();
    // Simulate an existing schema-v2 database without the auxiliary readiness marker.
    drop(store);
    {
        let db = redb::Database::open(dir.path().join("names.redb")).unwrap();
        let txn = db.begin_write().unwrap();
        txn.delete_table(xp_store::token_search::TOKEN_NAME_META)
            .unwrap();
        txn.commit().unwrap();
    }
    let store = Arc::new(Store::open(&dir.path().join("names.redb")).unwrap());
    if ready {
        prepare(&store);
    }
    let (_, status) = watch::channel(IngestStatus {
        source_observed_at_ms: None,
        source_error: None,
        indexed: Some(101),
        best: 101,
        mode: Mode::Tip,
        source: "fixture".into(),
        halted: None,
        stalled: None,
    });
    let state = AppState {
        store: store.clone(),
        status,
        counters: Arc::new(Counters::default()),
        read_permits: Arc::new(Semaphore::new(8)),
    };
    (
        dir,
        store,
        xp_api::router(
            state,
            &ApiConfig {
                per_second: 0,
                ..Default::default()
            },
        ),
    )
}

fn prepare(store: &Store) {
    for _ in 0..20 {
        if store.backfill_token_names_batch(2).unwrap().ready {
            return;
        }
    }
    panic!("backfill did not converge");
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn normalization_duplicates_prefix_exact_and_identity_evidence() {
    let (_dir, _store, app) = app(true);
    let (status, page) = get(&app, "/v1/tokens/search?q=%20SIGusd%20&match=exact").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["items"][0]["name"], "SigUSD");
    assert_eq!(page["items"][1]["name"], " SIGusd ");
    assert_ne!(page["items"][0]["id"], page["items"][1]["id"]);
    assert_eq!(page["items"][0]["emission"], "10000000000001");
    assert_eq!(page["items"][0]["mint_height"], 101);
    assert_eq!(page["search"]["normalized_query"], "sigusd");
    assert_eq!(page["search"]["coverage"], "partial");
    assert_eq!(page["search"]["partial_from"], 100);
    assert_eq!(page["search"]["indexed_names"], 4);
    assert_eq!(page["search"]["total_tokens"], 5);
    assert_eq!(page["search"]["unindexed_tokens"], 1);
    let (_, prefix) = get(&app, "/v1/tokens/search?q=sig").await;
    assert_eq!(prefix["items"].as_array().unwrap().len(), 3);
    let (_, numeric) = get(&app, "/v1/tokens/search?q=123&match=exact").await;
    assert_eq!(numeric["items"][0]["name"], "123");
    let (_, empty) = get(&app, "/v1/tokens/search?q=absent").await;
    assert_eq!(empty["items"], json!([]));
}

#[tokio::test]
async fn preparing_is_retryable_and_other_routes_remain_available() {
    let (_dir, store, app) = app(false);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/tokens/search?q=sig")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()["retry-after"], "1");
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "token_search_preparing");
    assert_eq!(get(&app, "/v1/tokens").await.0, StatusCode::OK);
    prepare(&store);
    assert_eq!(get(&app, "/v1/tokens/search?q=sig").await.0, StatusCode::OK);
}

#[tokio::test]
async fn strict_continuations_bind_query_mode_cursor_and_canonical_tip() {
    let (_dir, store, app) = app(true);
    let (_, first) = get(&app, "/v1/tokens/search?q=sig&limit=1&consistency=strict").await;
    let cursor = first["next_cursor"].as_str().unwrap();
    let snapshot = first["next_snapshot"].as_str().unwrap();
    let tail = format!("&limit=1&consistency=strict&cursor={cursor}&snapshot={snapshot}");
    let (status, second) = get(&app, &format!("/v1/tokens/search?q=SIG{tail}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    for prefix in ["q=sig&match=exact", "q=other"] {
        assert_eq!(
            get(&app, &format!("/v1/tokens/search?{prefix}{tail}"))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        get(
            &app,
            &format!("/v1/tokens/search?q=sig&consistency=strict&cursor={cursor}")
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let wrong_cursor = tail.replace(cursor, &hex::encode([3; 32]));
    assert_eq!(
        get(&app, &format!("/v1/tokens/search?q=sig{wrong_cursor}"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    store
        .apply_batch(&[block(&[], 102, [101; 32])], true)
        .unwrap();
    let (status, changed) = get(&app, &format!("/v1/tokens/search?q=sig{tail}")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(changed["code"], "snapshot_changed");
}

#[tokio::test]
async fn invalid_queries_fail_without_scanning() {
    let (_dir, _store, app) = app(true);
    for query in [
        "q=",
        "q=%20%09",
        "q=x%00",
        "q=sig&match=fuzzy",
        "q=sig&limit=0",
        "q=sig&cursor=no",
        "q=sig&consistency=snapshot",
    ] {
        assert_eq!(
            get(&app, &format!("/v1/tokens/search?{query}")).await.0,
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
    assert_eq!(
        get(&app, &format!("/v1/tokens/search?q={}", "x".repeat(513)))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}
