use axum::{body::Body, http::Request, Router};
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{watch, Semaphore};
use tower::ServiceExt;
use xp_api::{ApiConfig, AppState, Counters};
use xp_ingest::{IngestStatus, Mode};
use xp_store::Store;

fn fixture(height: u32) -> xp_wire::DecodedBlock {
    xp_wire::decode_block(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/{height}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}
fn app(blocks: &[xp_wire::DecodedBlock]) -> (tempfile::TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("x.redb")).unwrap();
    store
        .seed_for_tests(1865999, blocks[0].header.parent_id.0)
        .unwrap();
    store.apply_batch(blocks, true).unwrap();
    let (_tx, status) = watch::channel(IngestStatus {
        source_observed_at_ms: None,
        source_error: None,
        indexed: Some(1866000),
        best: 1866000,
        mode: Mode::Tip,
        source: "test".into(),
        halted: None,
        stalled: None,
    });
    let cfg = ApiConfig {
        per_second: 0,
        ..Default::default()
    };
    let state = AppState {
        store: Arc::new(store),
        status,
        counters: Arc::new(Counters::default()),
        read_permits: Arc::new(Semaphore::new(4)),
    };
    (dir, xp_api::router(state, &cfg))
}
async fn get(app: &Router, uri: &str) -> Value {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn observed_fixture_separates_twelve_gross_nine_obligation_three_subsidy() {
    let b = fixture(1866000);
    let (_dir, router) = app(&[b]);
    let result = get(&router, "/v1/blocks/1866000/rewards").await;
    assert_eq!(result["basis"], "observed_eip27_reward_box");
    assert_eq!(result["gross_reward"], "12000000000");
    assert_eq!(result["reemission_obligation"], "9000000000");
    assert_eq!(result["miner_subsidy"], "3000000000");
    assert_eq!(result["reward_box_id"].as_str().unwrap().len(), 64);
    let by_id = get(
        &router,
        &format!(
            "/v1/blocks/{}/rewards",
            result["block_id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(by_id, result);
}

#[tokio::test]
async fn mismatched_miner_key_is_unsupported_not_zero_or_a_height_based_guess() {
    let mut b = fixture(1866000);
    b.header.miner_pk[1] ^= 1;
    let (_dir, router) = app(&[b]);
    let result = get(&router, "/v1/blocks/1866000/rewards").await;
    assert_eq!(result["basis"], "unsupported");
    assert!(result["miner_subsidy"].is_null());
    assert!(result["gross_reward"].is_null());
}

#[tokio::test]
async fn network_summary_has_same_reader_totals_explicit_scope_and_small_payload() {
    let blocks = [fixture(1866000), fixture(1866001), fixture(1866002)];
    let count: usize = blocks.iter().map(|b| b.txs.len()).sum();
    let (_dir, router) = app(&blocks);
    let result = get(&router, "/v1/network/summary").await;
    assert_eq!(result["scope"], "latest_indexed_blocks");
    assert_eq!(result["block_count"], 4);
    assert_eq!(result["requested_blocks"], 720);
    assert_eq!(result["transaction_count"], count);
    assert_eq!(result["to_height"], 1866002);
    assert_eq!(result["from_height"], 1865999);
    assert_eq!(result["partial_from"], 1865999);
    assert_eq!(result["recent_blocks"][0]["id"], result["anchor_id"]);
    assert_eq!(result["recent_blocks"].as_array().unwrap().len(), 4);
    assert_eq!(result["blocks_per_hour"].as_array().unwrap().len(), 24);
    assert!(serde_json::to_vec(&result).unwrap().len() < 6000);
}

#[tokio::test]
async fn oversized_headers_are_rejected_before_decode_in_both_overview_endpoints() {
    let mut block = fixture(1866000);
    block.header.raw_json = "x".repeat(2 * 1024 * 1024 + 1);
    let (_dir, router) = app(&[block]);
    for (uri, code) in [
        ("/v1/network/summary", "network_summary_limit"),
        ("/v1/blocks/1866000/rewards", "reward_evidence_limit"),
    ] {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["code"], code);
    }
}

#[tokio::test]
async fn network_summary_scans_only_the_latest_720_headers() {
    let template = fixture(1866000);
    let mut parent = template.header.parent_id;
    let mut blocks = Vec::new();
    for index in 0..725u32 {
        let mut header = template.header.clone();
        header.height += index;
        header.timestamp += u64::from(index) * 120_000;
        header.id.0[..4].copy_from_slice(&index.to_be_bytes());
        header.parent_id = parent;
        parent = header.id;
        blocks.push(xp_wire::DecodedBlock {
            header,
            txs: vec![],
            size: 100,
        });
    }
    let (_dir, router) = app(&blocks);
    let result = get(&router, "/v1/network/summary").await;
    assert_eq!(result["block_count"], 720);
    assert_eq!(result["to_height"], 1866724);
    assert_eq!(result["from_height"], 1866005);
    let preview = result["recent_blocks"].as_array().unwrap();
    assert_eq!(preview.len(), 10);
    assert_eq!(preview[0]["id"], result["anchor_id"]);
    for (index, block) in preview.iter().enumerate() {
        assert_eq!(block["height"], 1_866_724 - index as u32);
    }
    assert_eq!(
        result["blocks_per_hour"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap())
            .sum::<u64>(),
        720
    );
    assert!(serde_json::to_vec(&result).unwrap().len() < 6000);
}
