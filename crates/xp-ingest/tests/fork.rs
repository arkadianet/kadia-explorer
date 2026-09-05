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

/// `(height, header id, raw json)` for one block of a fake chain.
type FakeBlock = (u32, Hash32, String);

fn fake_block(json: String) -> FakeBlock {
    let b = decode_block(&json).expect("fixture decodes");
    (b.header.height, b.header.id.0, json)
}

/// A hex id distinct from `id`: the same 32 bytes with the first one XOR-ed with 0xFF.
fn fork_id(id: &str) -> String {
    let mut bytes = xp_types::parse_hex32(id).unwrap();
    bytes[0] ^= 0xFF;
    xp_types::hex32(&bytes)
}

/// Chain A: the three fixtures verbatim.
fn chain_a() -> Vec<FakeBlock> {
    vec![
        fake_block(fixture_json(1866000)),
        fake_block(fixture_json(1866001)),
        fake_block(fixture_json(1866002)),
    ]
}

/// Chain B: 1866000 verbatim, then 1866001'/1866002' — the same bodies with fresh header ids.
/// Every occurrence of the old id is rewritten, which covers `header.id` on the block itself
/// and `header.parentId` (plus the `headerId` back-references) on its successor.
fn chain_b() -> Vec<FakeBlock> {
    let a = chain_a();
    let id1 = xp_types::hex32(&a[1].1);
    let id2 = xp_types::hex32(&a[2].1);
    let (id1b, id2b) = (fork_id(&id1), fork_id(&id2));
    let b1 = fixture_json(1866001).replace(&id1, &id1b);
    let b2 = fixture_json(1866002)
        .replace(&id1, &id1b)
        .replace(&id2, &id2b);
    let out = vec![a[0].clone(), fake_block(b1), fake_block(b2)];
    assert_eq!(xp_types::hex32(&out[1].1), id1b);
    assert_eq!(xp_types::hex32(&out[2].1), id2b);
    assert_ne!(out[1].1, a[1].1);
    assert_ne!(out[2].1, a[2].1);
    out
}

/// The header id of the chain entry at `height`.
fn id_at(chain: &[FakeBlock], height: u32) -> Hash32 {
    chain.iter().find(|b| b.0 == height).unwrap().1
}

/// In-memory `BlockSource` whose chain can be swapped out from under a running ingest loop.
struct FakeSource {
    chain: Mutex<Vec<FakeBlock>>,
    /// When set, `header_id_at` answers every height with a distinct synthetic id that can
    /// never match the store — simulating a fork deeper than the rollback window.
    always_fork: bool,
    best: Mutex<u32>,
}

impl FakeSource {
    fn new(chain: Vec<FakeBlock>) -> FakeSource {
        let best = chain.iter().map(|b| b.0).max().unwrap_or(0);
        FakeSource {
            chain: Mutex::new(chain),
            always_fork: false,
            best: Mutex::new(best),
        }
    }
    fn forking(best: u32) -> FakeSource {
        FakeSource {
            chain: Mutex::new(Vec::new()),
            always_fork: true,
            best: Mutex::new(best),
        }
    }
    fn set_chain(&self, chain: Vec<FakeBlock>) {
        *self.best.lock().unwrap() = chain.iter().map(|b| b.0).max().unwrap_or(0);
        *self.chain.lock().unwrap() = chain;
    }
}

#[async_trait::async_trait]
impl BlockSource for FakeSource {
    fn name(&self) -> &str {
        "fake"
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Ok(*self.best.lock().unwrap())
    }
    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError> {
        if self.always_fork {
            let mut id = [0u8; 32];
            id[..4].copy_from_slice(&height.to_be_bytes());
            id[4] = 0xAB;
            return Ok(Some(id));
        }
        Ok(self
            .chain
            .lock()
            .unwrap()
            .iter()
            .find(|b| b.0 == height)
            .map(|b| b.1))
    }
    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError> {
        Ok(self
            .chain
            .lock()
            .unwrap()
            .iter()
            .find(|b| &b.1 == id)
            .map(|b| b.2.clone()))
    }
    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        Ok("[]".into())
    }
}

/// A source that can never answer anything: every call is a transient failure. Used to observe
/// the status ingest publishes before it has done any work.
struct DeadSource;

#[async_trait::async_trait]
impl BlockSource for DeadSource {
    fn name(&self) -> &str {
        "dead"
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Err(SourceError::Unavailable)
    }
    async fn header_id_at(&self, _height: u32) -> Result<Option<Hash32>, SourceError> {
        Err(SourceError::Unavailable)
    }
    async fn full_block_json(&self, _id: &Hash32) -> Result<Option<String>, SourceError> {
        Err(SourceError::Unavailable)
    }
    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        Ok("[]".into())
    }
}

/// A source that reports a tip but cannot serve anything else, so ingest gets as far as the
/// fetch step — the path that used to publish `Some(0)` for an empty store — before failing.
struct TipOnlySource;

#[async_trait::async_trait]
impl BlockSource for TipOnlySource {
    fn name(&self) -> &str {
        "dead"
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Ok(10)
    }
    async fn header_id_at(&self, _height: u32) -> Result<Option<Hash32>, SourceError> {
        Err(SourceError::Unavailable)
    }
    async fn full_block_json(&self, _id: &Hash32) -> Result<Option<String>, SourceError> {
        Err(SourceError::Unavailable)
    }
    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        Ok("[]".into())
    }
}

/// Runs ingest against `source` just long enough to observe its first published status.
async fn first_published_status(store: Arc<Store>, source: Arc<dyn BlockSource>) -> IngestStatus {
    let (tx, mut rx) = watch::channel(initial_status());
    let shutdown = CancellationToken::new();
    let handle = tokio::spawn(run(store, source, test_cfg(), tx, shutdown.clone()));
    // `initial_status()` uses an empty source name, so the first status carrying "dead" is the
    // first one ingest itself published.
    let status = timeout(Duration::from_secs(10), async {
        loop {
            {
                let s = rx.borrow_and_update();
                if s.source == "dead" {
                    return s.clone();
                }
            }
            rx.changed().await.expect("run dropped the status sender");
        }
    })
    .await
    .expect("no status published");
    shutdown.cancel();
    handle.await.unwrap().unwrap();
    status
}

/// An empty store has indexed nothing, and must say so — `Some(0)` would claim it holds the
/// genesis block.
#[tokio::test]
async fn empty_store_publishes_indexed_none() {
    // Both a source that fails immediately and one that gets ingest as far as fetching:
    // neither may turn "nothing indexed" into height 0.
    let sources: Vec<Arc<dyn BlockSource>> = vec![Arc::new(DeadSource), Arc::new(TipOnlySource)];
    for source in sources {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&dir.path().join("x.redb")).unwrap());
        let status = first_published_status(store, source).await;
        assert_eq!(status.indexed, None, "empty store must not report a height");
        assert_eq!(status.halted, None);
    }
}

#[tokio::test]
async fn seeded_store_publishes_its_indexed_height() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("x.redb")).unwrap());
    store.seed_for_tests(1865999, [3u8; 32]).unwrap();
    let status = first_published_status(store, Arc::new(DeadSource)).await;
    assert_eq!(status.indexed, Some(1865999));
    assert_eq!(status.halted, None);
}

fn test_cfg() -> IngestConfig {
    IngestConfig {
        poll_ms: 20,
        tip_lag_for_bulk: 1,
        ..Default::default()
    }
}

fn initial_status() -> IngestStatus {
    IngestStatus {
        indexed: None,
        best: 0,
        mode: Mode::Tip,
        source: String::new(),
        halted: None,
    }
}

/// Waits (up to 10 s, driven by status notifications rather than fixed sleeps) for `f`.
async fn wait_for(rx: &mut watch::Receiver<IngestStatus>, what: &str, mut f: impl FnMut() -> bool) {
    let waited = timeout(Duration::from_secs(10), async {
        loop {
            if f() {
                return;
            }
            if rx.changed().await.is_err() {
                // Sender gone (run() returned): give the condition one last chance.
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    })
    .await;
    assert!(waited.is_ok(), "timed out waiting for {what}");
}

#[tokio::test]
async fn reorg_rolls_back_and_reapplies_the_new_chain() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("x.redb")).unwrap());
    let mut a = chain_a();
    let mut b = chain_b();
    let seed_id = decode_block(&a[0].2).unwrap().header.parent_id.0;
    store.seed_for_tests(1865999, seed_id).unwrap();
    // The source must also know the seeded ancestor: a real node always holds the block below
    // our tip, and answering `None` there means "source is behind", which ingest waits out.
    // No body is ever requested for it, since ingest only fetches above its own tip.
    a.insert(0, (1865999, seed_id, String::new()));
    b.insert(0, (1865999, seed_id, String::new()));

    let source = Arc::new(FakeSource::new(a.clone()));
    let (tx, mut rx) = watch::channel(initial_status());
    let shutdown = CancellationToken::new();
    let handle = tokio::spawn(run(
        store.clone(),
        source.clone(),
        test_cfg(),
        tx,
        shutdown.clone(),
    ));

    {
        let s = store.clone();
        wait_for(&mut rx, "chain A tip", move || {
            s.indexed_height().unwrap() == Some(1866002)
        })
        .await;
    }
    assert_eq!(
        store.header_id_at(1866002).unwrap(),
        Some(id_at(&a, 1866002))
    );

    source.set_chain(b.clone());
    {
        let s = store.clone();
        let want = id_at(&b, 1866002);
        wait_for(&mut rx, "chain B tip", move || {
            s.header_id_at(1866002).unwrap() == Some(want)
        })
        .await;
    }
    assert_eq!(store.indexed_height().unwrap(), Some(1866002));
    assert_eq!(
        store.header_id_at(1866001).unwrap(),
        Some(id_at(&b, 1866001))
    );
    assert_eq!(rx.borrow().halted, None);

    shutdown.cancel();
    let out = timeout(Duration::from_secs(10), handle)
        .await
        .unwrap()
        .unwrap();
    assert!(out.is_ok(), "run returned {out:?}");
}

#[tokio::test]
async fn fork_deeper_than_rollback_window_halts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("x.redb")).unwrap());
    store.seed_for_tests(1865999, [7u8; 32]).unwrap();

    let source = Arc::new(FakeSource::forking(1866002));
    let (tx, rx) = watch::channel(initial_status());
    let shutdown = CancellationToken::new();
    let err = timeout(
        Duration::from_secs(10),
        run(store, source, test_cfg(), tx, shutdown),
    )
    .await
    .expect("run should return promptly")
    .expect_err("run should fail on an unresolvable fork");
    assert!(
        err.to_string().contains("reindex required"),
        "unexpected error: {err}"
    );
    assert!(rx.borrow().halted.is_some(), "halted status not published");
}
