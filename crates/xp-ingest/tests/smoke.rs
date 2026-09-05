//! Ignored end-to-end smoke test against a locally running Ergo node.
//!
//! Run with: `cargo test -p xp-ingest -- --ignored smoke_local_node --nocapture`

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use xp_ingest::{run, IngestConfig, IngestStatus, Mode};
use xp_source::{BlockSource, RustNode, SourceError};
use xp_store::Store;
use xp_types::Hash32;

const NODE: &str = "http://127.0.0.1:9063";
const TARGET: u32 = 2000;

/// A real node, but pretending the chain ends at `cap`, so the run terminates at a known
/// height instead of chasing the live tip.
struct Capped {
    inner: RustNode,
    cap: u32,
}

#[async_trait::async_trait]
impl BlockSource for Capped {
    fn name(&self) -> &str {
        self.inner.name()
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Ok(self.inner.best_height().await?.min(self.cap))
    }
    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError> {
        if height > self.cap {
            return Ok(None);
        }
        self.inner.header_id_at(height).await
    }
    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError> {
        self.inner.full_block_json(id).await
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a local Ergo node on 127.0.0.1:9063"]
async fn smoke_local_node() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("smoke.redb")).unwrap());
    let source: Arc<dyn BlockSource> = Arc::new(Capped {
        inner: RustNode::new(NODE),
        cap: TARGET,
    });
    assert!(
        source.best_height().await.is_ok(),
        "no Ergo node reachable at {NODE}"
    );

    let (tx, mut rx) = watch::channel(IngestStatus {
        indexed: None,
        best: 0,
        mode: Mode::Tip,
        source: String::new(),
        halted: None,
    });
    let shutdown = CancellationToken::new();
    let cfg = IngestConfig {
        bulk_batch: 100,
        ..Default::default()
    };
    let started = Instant::now();
    let handle = tokio::spawn(run(store.clone(), source, cfg, tx, shutdown.clone()));

    timeout(Duration::from_secs(600), async {
        while rx.borrow().indexed != Some(TARGET) {
            assert_eq!(rx.borrow().halted, None, "ingest halted");
            if rx.changed().await.is_err() {
                break;
            }
        }
    })
    .await
    .expect("did not reach target height in 600 s");
    let elapsed = started.elapsed();

    shutdown.cancel();
    handle.await.unwrap().unwrap();

    assert_eq!(store.indexed_height().unwrap(), Some(TARGET));
    // Height 1 spends chain-spec genesis boxes the store never saw; applying it at all proves
    // that path did not error.
    assert!(store.header_id_at(1).unwrap().is_some());
    println!(
        "smoke: {TARGET} blocks in {:.2}s = {:.0} blocks/s",
        elapsed.as_secs_f64(),
        TARGET as f64 / elapsed.as_secs_f64()
    );
}
