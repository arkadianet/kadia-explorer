//! A source that announces a header at a height but will not serve its body used to freeze
//! ingest silently: `fetch_range` stopped at the hole, applied nothing, and the loop span on
//! `poll_ms` forever while `/v1/status` showed nothing unusual. These tests pin the visible
//! symptom instead: the published status carries `stalled`, and it clears once the body shows
//! up.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use xp_ingest::{run, IngestConfig, IngestStatus, Mode};
use xp_source::{BlockSource, SourceError};
use xp_store::Store;
use xp_types::Hash32;
use xp_wire::decode_block;

fn fixture_json(h: u32) -> String {
    std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/blocks/{h}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// `(height, header id, raw json)`.
type FakeBlock = (u32, Hash32, String);

fn fake_block(json: String) -> FakeBlock {
    let b = decode_block(&json).expect("fixture decodes");
    (b.header.height, b.header.id.0, json)
}

/// A source that serves headers for the whole chain but refuses the body of whichever height
/// is currently withheld — exactly the primary node's 404-on-`/blocks/{id}` behaviour.
struct HoleSource {
    chain: Vec<FakeBlock>,
    withheld: Mutex<Option<u32>>,
}

impl HoleSource {
    fn new(chain: Vec<FakeBlock>, withheld: u32) -> HoleSource {
        HoleSource {
            chain,
            withheld: Mutex::new(Some(withheld)),
        }
    }
    fn serve_everything(&self) {
        *self.withheld.lock().unwrap() = None;
    }
}

#[async_trait::async_trait]
impl BlockSource for HoleSource {
    fn name(&self) -> &str {
        "hole"
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Ok(self.chain.iter().map(|b| b.0).max().unwrap_or(0))
    }
    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError> {
        Ok(self.chain.iter().find(|b| b.0 == height).map(|b| b.1))
    }
    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError> {
        let entry = self.chain.iter().find(|b| &b.1 == id);
        match entry {
            Some(b) if *self.withheld.lock().unwrap() == Some(b.0) => Ok(None),
            Some(b) => Ok(Some(b.2.clone())),
            None => Ok(None),
        }
    }
    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        Ok("[]".into())
    }
}

fn initial_status() -> IngestStatus {
    IngestStatus {
        source_observed_at_ms: None,
        source_error: None,
        indexed: None,
        best: 0,
        mode: Mode::Tip,
        source: String::new(),
        halted: None,
        stalled: None,
    }
}

/// Waits (up to `secs`, driven by status notifications) for `f`.
async fn wait_for(
    rx: &mut watch::Receiver<IngestStatus>,
    secs: u64,
    what: &str,
    mut f: impl FnMut() -> bool,
) {
    let waited = timeout(Duration::from_secs(secs), async {
        loop {
            if f() {
                return;
            }
            if rx.changed().await.is_err() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    })
    .await;
    assert!(waited.is_ok(), "timed out waiting for {what}");
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_body_publishes_stalled_and_clears_when_it_appears() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("x.redb")).unwrap());
    let mut chain = vec![
        fake_block(fixture_json(1866000)),
        fake_block(fixture_json(1866001)),
        fake_block(fixture_json(1866002)),
    ];
    let seed_id = decode_block(&chain[0].2).unwrap().header.parent_id.0;
    store.seed_for_tests(1865999, seed_id).unwrap();
    // The source knows the seeded ancestor too (a real node holds the block below our tip);
    // its body is never requested, since ingest only fetches above its own tip.
    chain.insert(0, (1865999, seed_id, String::new()));

    let source = Arc::new(HoleSource::new(chain, 1866000));
    let (tx, mut rx) = watch::channel(initial_status());
    let shutdown = CancellationToken::new();
    let cfg = IngestConfig {
        poll_ms: 20,
        tip_lag_for_bulk: 1,
        ..Default::default()
    };
    let handle = tokio::spawn(run(
        store.clone(),
        source.clone(),
        cfg,
        tx,
        shutdown.clone(),
    ));

    {
        let rx2 = rx.clone();
        wait_for(&mut rx, 10, "stalled status", move || {
            rx2.borrow().stalled.is_some()
        })
        .await;
    }
    {
        let s = rx.borrow();
        let stalled = s.stalled.clone().unwrap();
        assert_eq!(stalled.height, 1866000, "stall names the withheld height");
        assert!(
            !stalled.reason.is_empty(),
            "stall carries a human-readable reason"
        );
        assert_eq!(s.halted, None, "a stall is not a halt");
        assert_eq!(s.indexed, Some(1865999), "indexed must not advance");
    }
    // Still stuck a moment later: the stall persists rather than being a one-off blip.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(store.indexed_height().unwrap(), Some(1865999));
    assert!(rx.borrow().stalled.is_some());

    source.serve_everything();
    {
        let s = store.clone();
        wait_for(
            &mut rx,
            30,
            "chain tip after the hole is filled",
            move || s.indexed_height().unwrap() == Some(1866002),
        )
        .await;
    }
    {
        let rx2 = rx.clone();
        wait_for(&mut rx, 10, "stall to clear", move || {
            rx2.borrow().stalled.is_none()
        })
        .await;
    }
    assert_eq!(rx.borrow().halted, None);

    shutdown.cancel();
    handle.await.unwrap().unwrap();
}
