//! An orphan sibling applied at the tip used to wedge ingest forever: every later best-chain
//! block failed its parent check, and the fork check "agreed" because it asked the source the
//! same question that produced the orphan in the first place. These tests pin both halves of
//! the recovery — the fork check rolling the orphan back once the source names the best-chain
//! header, and a parent mismatch that persists becoming *visible* as a published stall.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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

/// The best chain: 1866000..=1866002, the fixtures verbatim, below a seed at 1865999.
fn best_chain() -> Vec<FakeBlock> {
    let a = fake_block(fixture_json(1866000));
    let seed_id = decode_block(&a.2).unwrap().header.parent_id.0;
    vec![
        (1865999, seed_id, String::new()),
        a,
        fake_block(fixture_json(1866001)),
        fake_block(fixture_json(1866002)),
    ]
}

/// The orphan sibling of 1866001: the same body under a different header id, so its parent is
/// still 1866000 (it applies cleanly on our tip) but nothing on the best chain builds on it.
fn orphan_1866001(best: &[FakeBlock]) -> FakeBlock {
    let real = xp_types::hex32(&id_at(best, 1866001));
    let mut bytes = xp_types::parse_hex32(&real).unwrap();
    bytes[0] ^= 0xFF;
    let orphan_id = xp_types::hex32(&bytes);
    let b = fake_block(fixture_json(1866001).replace(&real, &orphan_id));
    assert_ne!(b.1, id_at(best, 1866001));
    assert_eq!(
        decode_block(&b.2).unwrap().header.parent_id.0,
        id_at(best, 1866000),
        "the orphan must build on our tip, or it would never apply"
    );
    b
}

fn id_at(chain: &[FakeBlock], height: u32) -> Hash32 {
    chain.iter().find(|b| b.0 == height).unwrap().1
}

/// A node holding an orphan at 1866001. `header_id_at(1866001)` answers with the orphan while
/// `orphan_wins` is set; bodies are served for every block it knows, orphan included.
///
/// `offer_next` decides whether the *next* height is answered at all while the
/// orphan is being served: with it off, ingest simply has nothing above the orphan (the
/// benign case); with it on, the source offers best-chain 1866002, whose parent is the real
/// 1866001 — a parent mismatch that repeats forever, which is the case a stall must surface.
struct OrphanSource {
    best: Vec<FakeBlock>,
    orphan: FakeBlock,
    orphan_wins: Mutex<bool>,
    offer_next: bool,
    /// How many times the height *below* the orphan has been asked about. Nothing asks for it
    /// once it is indexed unless the fork check walks back below our tip, so it is the visible
    /// trace of that walk-back — and, with `offer_next`, the trigger for the blackout below.
    asked_below: AtomicU32,
    /// Set once ingest has asked for the successor's body, i.e. once the parent mismatch is
    /// under way. From then on, an ask for the height below the orphan can only be the fork
    /// check walking below our tip.
    wedged: AtomicBool,
    /// Set when that walk-back happens: past it the source stops announcing the orphan's
    /// height at all, so the rollback is followed by no successful apply. That is what makes
    /// "the stall survived the rollback" observable rather than a race against the orphan
    /// being re-applied (which would clear the stall legitimately, as progress).
    blackout: AtomicBool,
}

impl OrphanSource {
    fn new(offer_next: bool) -> OrphanSource {
        let best = best_chain();
        let orphan = orphan_1866001(&best);
        OrphanSource {
            best,
            orphan,
            orphan_wins: Mutex::new(true),
            offer_next,
            asked_below: AtomicU32::new(0),
            wedged: AtomicBool::new(false),
            blackout: AtomicBool::new(false),
        }
    }
    fn asked_below(&self) -> u32 {
        self.asked_below.load(Ordering::Relaxed)
    }
    /// The node settles on the best chain (what the fixed `header_id_at` reports).
    fn switch_to_best_chain(&self) {
        *self.orphan_wins.lock().unwrap() = false;
    }
}

#[async_trait::async_trait]
impl BlockSource for OrphanSource {
    fn name(&self) -> &str {
        "orphan"
    }
    async fn best_height(&self) -> Result<u32, SourceError> {
        Ok(1866002)
    }
    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError> {
        if height == 1866000 {
            self.asked_below.fetch_add(1, Ordering::Relaxed);
            if self.offer_next && self.wedged.load(Ordering::Relaxed) {
                self.blackout.store(true, Ordering::Relaxed);
            }
        }
        if *self.orphan_wins.lock().unwrap() {
            if height == 1866001 {
                if self.blackout.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                return Ok(Some(self.orphan.1));
            }
            if height == 1866002 && !self.offer_next {
                return Ok(None);
            }
        }
        Ok(self.best.iter().find(|b| b.0 == height).map(|b| b.1))
    }
    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError> {
        if id == &id_at(&self.best, 1866002) {
            self.wedged.store(true, Ordering::Relaxed);
        }
        if id == &self.orphan.1 {
            return Ok(Some(self.orphan.2.clone()));
        }
        Ok(self
            .best
            .iter()
            .find(|b| &b.1 == id)
            .map(|b| b.2.clone())
            .filter(|j| !j.is_empty()))
    }
    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        Ok("[]".into())
    }
}

fn initial_status() -> IngestStatus {
    IngestStatus {
        indexed: None,
        best: 0,
        mode: Mode::Tip,
        source: String::new(),
        halted: None,
        stalled: None,
    }
}

/// One block per batch, so the orphan is actually committed at the tip instead of failing
/// inside a bulk transaction alongside its successor.
fn one_at_a_time() -> IngestConfig {
    IngestConfig {
        poll_ms: 20,
        tip_lag_for_bulk: u32::MAX,
        ..Default::default()
    }
}

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

fn seeded_store(dir: &tempfile::TempDir) -> Arc<Store> {
    let store = Arc::new(Store::open(&dir.path().join("x.redb")).unwrap());
    let seed_id = id_at(&best_chain(), 1865999);
    store.seed_for_tests(1865999, seed_id).unwrap();
    store
}

/// The benign shape: an orphan gets indexed at the tip, then the source names the best-chain
/// header for that height. The fork check must notice and roll the orphan back — before this
/// fix the source kept naming the orphan, so it never did.
#[tokio::test(flavor = "multi_thread")]
async fn orphan_tip_is_rolled_back_once_the_source_names_the_best_chain() {
    let dir = tempfile::tempdir().unwrap();
    let store = seeded_store(&dir);
    let best = best_chain();
    let source = Arc::new(OrphanSource::new(false));
    let orphan_id = source.orphan.1;

    let (tx, mut rx) = watch::channel(initial_status());
    let shutdown = CancellationToken::new();
    let handle = tokio::spawn(run(
        store.clone(),
        source.clone(),
        one_at_a_time(),
        tx,
        shutdown.clone(),
    ));

    {
        let s = store.clone();
        wait_for(&mut rx, 10, "the orphan to be indexed", move || {
            s.header_id_at(1866001).unwrap() == Some(orphan_id)
        })
        .await;
    }

    source.switch_to_best_chain();
    {
        let s = store.clone();
        wait_for(&mut rx, 30, "the best chain at 1866002", move || {
            s.indexed_height().unwrap() == Some(1866002)
        })
        .await;
    }
    assert_eq!(
        store.header_id_at(1866001).unwrap(),
        Some(id_at(&best, 1866001)),
        "the orphan must have been rolled back and replaced"
    );
    assert_eq!(
        store.header_id_at(1866002).unwrap(),
        Some(id_at(&best, 1866002))
    );
    assert_eq!(rx.borrow().halted, None);

    shutdown.cancel();
    handle.await.unwrap().unwrap();
}

/// The wedged shape, and the one that stopped the mainnet sync: the source keeps naming the
/// orphan at the indexed height — so the fork check agrees — while offering a best-chain
/// successor whose parent is the header we do *not* have. Ingest then retries the same failing
/// apply forever. It must say so (`stalled`) rather than look like a healthy idle indexer, and
/// must recover on its own once the source stops naming the orphan.
#[tokio::test(flavor = "multi_thread")]
async fn persistent_parent_mismatch_publishes_a_stall_and_then_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let store = seeded_store(&dir);
    let best = best_chain();
    let source = Arc::new(OrphanSource::new(true));

    let (tx, mut rx) = watch::channel(initial_status());
    let shutdown = CancellationToken::new();
    let handle = tokio::spawn(run(
        store.clone(),
        source.clone(),
        one_at_a_time(),
        tx,
        shutdown.clone(),
    ));

    {
        let rx2 = rx.clone();
        wait_for(&mut rx, 20, "a stalled status", move || {
            rx2.borrow().stalled.is_some()
        })
        .await;
    }
    {
        let s = rx.borrow();
        let stalled = s.stalled.clone().unwrap();
        assert_eq!(stalled.height, 1866002, "the stall names the wanted height");
        assert!(
            stalled.reason.contains("parent mismatch"),
            "the stall must name the cause, got {:?}",
            stalled.reason
        );
        assert_eq!(s.halted, None, "a stall is not a halt");
        assert!(
            s.indexed <= Some(1866001),
            "indexed must not pass the mismatch"
        );
    }

    // The fork check, left alone, agrees at our tip — the source names the very orphan we
    // hold there — and so never looks lower. A stalled parent check must make it distrust the
    // tip and walk back regardless, which is what asking about 1866000 again proves.
    let before = source.asked_below();
    let since_before = rx.borrow().stalled.clone().unwrap().since_secs;
    {
        let src = source.clone();
        wait_for(
            &mut rx,
            30,
            "the fork check to walk below our tip",
            move || src.asked_below() > before,
        )
        .await;
    }

    // The rollback that walk-back triggers is an *attempt* to clear the wedge, not the end of
    // it: nothing has applied yet. So the stall must survive it, still naming the same height
    // and still counting from when the wedge started. Clearing it on rollback (as a
    // missing-body stall is cleared) would drop it here for good — after the walk-back the
    // source stops announcing 1866001, so no further mismatch can re-raise it.
    {
        let rx2 = rx.clone();
        wait_for(
            &mut rx,
            40,
            "the stall to keep ageing across the rollback",
            move || {
                let s = rx2.borrow();
                let stalled = s
                    .stalled
                    .clone()
                    .expect("the stall must survive the rollback");
                assert_eq!(stalled.height, 1866002, "the stall must not change height");
                assert!(
                    stalled.reason.contains("parent mismatch"),
                    "the stall must keep its cause, got {:?}",
                    stalled.reason
                );
                assert!(
                    stalled.since_secs >= since_before,
                    "since_secs went backwards: {since_before} then {}",
                    stalled.since_secs
                );
                stalled.since_secs > since_before
            },
        )
        .await;
    }

    source.switch_to_best_chain();
    {
        let s = store.clone();
        wait_for(&mut rx, 60, "the best chain at 1866002", move || {
            s.indexed_height().unwrap() == Some(1866002)
        })
        .await;
    }
    {
        let rx2 = rx.clone();
        wait_for(&mut rx, 20, "the stall to clear", move || {
            rx2.borrow().stalled.is_none()
        })
        .await;
    }
    assert_eq!(
        store.header_id_at(1866001).unwrap(),
        Some(id_at(&best, 1866001))
    );
    assert_eq!(rx.borrow().halted, None);

    shutdown.cancel();
    handle.await.unwrap().unwrap();
}
