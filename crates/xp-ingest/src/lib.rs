//! The ingest pipeline: fetch → fork-check → apply.
//!
//! [`run`] drives a [`Store`] towards a [`BlockSource`]'s best chain forever, until the
//! supplied [`CancellationToken`] is cancelled or an unrecoverable condition (a fork deeper
//! than [`ROLLBACK_WINDOW`], a corrupt store, an undecodable block) forces it to halt.
//! Progress is published on a [`watch`] channel so a server can serve it as a health page.

use futures::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use xp_source::{BlockSource, SourceError};
use xp_store::{Store, StoreError, ROLLBACK_WINDOW};
use xp_types::Hash32;
use xp_wire::{decode_block, DecodedBlock};

#[derive(Clone, Debug)]
pub struct IngestConfig {
    /// How long to wait before re-polling when there is nothing to do (or after a transient
    /// source error).
    pub poll_ms: u64,
    /// Blocks fetched and applied per transaction while in [`Mode::Bulk`].
    pub bulk_batch: usize,
    /// In-flight block fetches while in [`Mode::Bulk`].
    pub bulk_concurrency: usize,
    /// While in [`Mode::Bulk`], force a durable (fsync-ed) commit whenever the batch crosses a
    /// multiple of this many heights; intermediate batches commit without fsync, so a crash
    /// costs at most `durable_every` blocks of re-sync.
    pub durable_every: u32,
    /// Lag (best − indexed) above which [`Mode::Bulk`] is used instead of [`Mode::Tip`].
    pub tip_lag_for_bulk: u32,
}

impl Default for IngestConfig {
    fn default() -> IngestConfig {
        IngestConfig {
            poll_ms: 500,
            bulk_batch: 64,
            bulk_concurrency: 8,
            durable_every: 256,
            tip_lag_for_bulk: 64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Bulk,
    Tip,
}

#[derive(Clone, Debug)]
pub struct IngestStatus {
    pub source_observed_at_ms: Option<u64>,
    pub source_error: Option<String>,
    pub indexed: Option<u32>,
    pub best: u32,
    pub mode: Mode,
    pub source: String,
    /// `Some(reason)` once ingest has given up; [`run`] returns an error immediately after
    /// publishing it.
    pub halted: Option<String>,
    /// `Some` while ingest is making no progress for a reason it expects to resolve itself:
    /// a source that announces a header at a height but will not serve its body, or a parent
    /// check that keeps failing at the same height (an orphan sibling indexed at our tip).
    /// Unlike [`IngestStatus::halted`], ingest keeps retrying (see [`STALL_BACKOFF`]), so this
    /// is the only outward sign that `indexed` has stopped moving.
    pub stalled: Option<StalledInfo>,
}

/// Why, where and for how long ingest has been stuck. Published on [`IngestStatus::stalled`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StalledInfo {
    /// The height ingest cannot get past: the one whose body the source will not serve, or
    /// the one whose parent check keeps failing.
    pub height: u32,
    /// Seconds since the stall started (since the first failed attempt at `height`).
    pub since_secs: u64,
    /// Human-readable cause, safe to show in a UI.
    pub reason: String,
}

/// How many times in a row the same height may fail its parent check before ingest treats it
/// as a wedge rather than a mid-batch chain move: it publishes a stall and forces the fork
/// check below its own tip. Small, because the benign case (the source's chain moved under us
/// during a batch) resolves on the very next iteration.
const PARENT_MISMATCH_LIMIT: u32 = 5;

/// Poll interval used instead of `poll_ms` while stalled. A stall is a source-side hole that
/// no amount of re-asking fixes quickly, so backing off keeps ingest from hammering the node
/// (twice a second, at the default `poll_ms`) for as long as it lasts.
pub const STALL_BACKOFF: Duration = Duration::from_secs(5);
/// How often a persisting stall is re-logged. The start is always logged.
const STALL_WARN_EVERY: Duration = Duration::from_secs(60);

/// What put ingest in a stall. Not published — [`StalledInfo::reason`] is what a UI shows —
/// but the run loop has to tell the two apart: a stall raised by a persistent parent mismatch
/// outlives the rollback taken to clear it, while a missing-body stall is about a height the
/// rollback moved away from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StallOrigin {
    /// The source announced a header at the height but would not serve its body.
    MissingBody,
    /// The same height failed its parent check [`PARENT_MISMATCH_LIMIT`] times over.
    ParentMismatch,
}

/// Live stall bookkeeping. [`StalledInfo`] is derived from it on every publish so `since_secs`
/// keeps counting up while nothing else changes.
struct Stall {
    height: u32,
    since: Instant,
    last_warn: Instant,
    reason: String,
    origin: StallOrigin,
}

impl Stall {
    fn info(&self) -> StalledInfo {
        StalledInfo {
            height: self.height,
            since_secs: self.since.elapsed().as_secs(),
            reason: self.reason.clone(),
        }
    }
}

/// Records that ingest is stuck at `height` for `reason`, starting a stall or refreshing the
/// one already running. Both stall sites go through here so that "start, keep `since` ticking,
/// re-warn at most every [`STALL_WARN_EVERY`]" has exactly one implementation.
///
/// A stall at a different height, or from a different `origin`, replaces the current one:
/// `since_secs` measures how long *this* wedge has lasted, not how long ingest has been
/// unhappy in general.
fn note_stall(stall: &mut Option<Stall>, height: u32, origin: StallOrigin, reason: String) {
    match stall {
        Some(st) if st.height == height && st.origin == origin => {
            if st.last_warn.elapsed() >= STALL_WARN_EVERY {
                st.last_warn = Instant::now();
                warn!(
                    height,
                    stalled_secs = st.since.elapsed().as_secs(),
                    "ingest still stalled: {reason}"
                );
            }
        }
        _ => {
            warn!(height, "ingest stalled: {reason}");
            let now = Instant::now();
            *stall = Some(Stall {
                height,
                since: now,
                last_warn: now,
                reason,
                origin,
            });
        }
    }
}

/// What the per-iteration fork check concluded.
enum ForkCheck {
    /// Store and source agree at the store's tip (or the store is empty): carry on.
    Agreed,
    /// The store's tip is on a dead branch; roll back to this height first.
    RollbackTo(u32),
    /// The source can't answer for a height the store has — it is behind us, or on a shorter
    /// chain. Wait rather than destroying indexed state.
    Wait,
    /// The common ancestor is further back than [`ROLLBACK_WINDOW`]: unrecoverable.
    TooDeep,
    /// A transient source error; wait and retry.
    SourceError(SourceError),
    /// The store itself failed to answer. Never a chain disagreement: hard failure.
    StoreError(StoreError),
}

/// Drives `store` towards `source`'s best chain until `shutdown` is cancelled.
///
/// Returns `Ok(())` on shutdown. Returns `Err` only for conditions that cannot be retried
/// out of: a fork deeper than [`ROLLBACK_WINDOW`], an undecodable block body, or a store
/// error other than [`StoreError::ParentMismatch`]. Source errors are always transient.
pub async fn run(
    store: Arc<Store>,
    source: Arc<dyn BlockSource>,
    cfg: IngestConfig,
    status: watch::Sender<IngestStatus>,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let source_name = source.name().to_string();
    let poll = Duration::from_millis(cfg.poll_ms);
    let mut best = 0u32;
    let mut mode = Mode::Tip;

    // `Some` for as long as the source withholds a body we need; see [`Stall`].
    let mut stall: Option<Stall> = None;
    // `Some((height, times))` while the same height keeps failing its parent check. Reset by
    // anything that changes the situation: an applied batch, or a rollback.
    let mut parent_mismatch: Option<(u32, u32)> = None;

    let publish = |indexed: Option<u32>,
                   best: u32,
                   mode: Mode,
                   halted: Option<String>,
                   stalled: Option<StalledInfo>| {
        let previous = status.borrow().clone();
        let _ = status.send(IngestStatus {
            source_observed_at_ms: previous.source_observed_at_ms,
            source_error: previous.source_error,
            indexed,
            best,
            mode,
            source: source_name.clone(),
            halted,
            stalled,
        });
    };
    // Every transient path is the same three steps — publish, sleep (racing shutdown), retry —
    // so they share one macro rather than five hand-copied blocks that can drift apart.
    macro_rules! retry {
        ($indexed:expr) => {{
            publish($indexed, best, mode, None, stall.as_ref().map(Stall::info));
            // While stalled, re-asking at `poll_ms` only hammers the source; back off.
            let wait = if stall.is_some() {
                poll.max(STALL_BACKOFF)
            } else {
                poll
            };
            if sleep_or_shutdown(wait, &shutdown).await {
                return Ok(());
            }
            continue;
        }};
    }
    // Publishes `halted` and returns the typed store error, so the caller keeps the real
    // cause (and its source chain) rather than a re-worded string.
    macro_rules! halt_store {
        ($indexed:expr, $err:expr) => {{
            let err: StoreError = $err;
            publish(
                $indexed,
                best,
                mode,
                Some(err.to_string()),
                stall.as_ref().map(Stall::info),
            );
            return Err(err.into());
        }};
    }
    // Publishes `halted` and turns the reason into the error `run` returns.
    macro_rules! halt {
        ($indexed:expr, $reason:expr) => {{
            let reason: String = $reason;
            publish(
                $indexed,
                best,
                mode,
                Some(reason.clone()),
                stall.as_ref().map(Stall::info),
            );
            return Err(anyhow::anyhow!(reason));
        }};
    }

    // The chain-spec genesis boxes belong to no block, so a store that will sync from
    // height 1 has to be given them before the first block is applied — otherwise height 1's
    // spend of the emission box, and later spends of the other two, look like missing inputs.
    // Skipped for a store that already indexed something (it either seeded already or was
    // seeded partial from a later height).
    loop {
        if shutdown.is_cancelled() {
            return Ok(());
        }
        let empty = match store.indexed_height() {
            Ok(v) => v.is_none(),
            Err(e) => halt_store!(None, e),
        };
        let seeded = match store.genesis_seeded() {
            Ok(v) => v,
            Err(e) => halt_store!(None, e),
        };
        if !empty || seeded {
            break;
        }
        let json = match source.genesis_boxes_json().await {
            Ok(j) => j,
            Err(e) => {
                warn!(source = %source_name, error = %e, "genesis box fetch failed; retrying");
                retry!(None);
            }
        };
        let s = store.clone();
        let out = tokio::task::spawn_blocking(move || -> Result<usize, ApplyErr> {
            let boxes = xp_wire::decode_genesis_boxes(&json)
                .map_err(|e| ApplyErr::Decode(e.to_string()))?;
            if boxes.is_empty() {
                // Still seeded (the flag is what stops us re-fetching, and test doubles
                // legitimately return an empty list), but on a real chain this guarantees
                // the first block that spends a genesis box will halt ingest.
                warn!(
                    "node returned zero genesis boxes; a from-scratch mainnet sync will halt at height 1 — check the source"
                );
            }
            s.seed_genesis(&boxes).map_err(ApplyErr::Store)?;
            Ok(boxes.len())
        })
        .await;
        match out {
            Ok(Ok(n)) => info!(boxes = n, "seeded chain-spec genesis boxes"),
            Ok(Err(ApplyErr::Store(e))) => halt_store!(None, e),
            Ok(Err(ApplyErr::Decode(e))) => halt!(None, format!("undecodable genesis boxes: {e}")),
            Err(join) => halt!(None, join.to_string()),
        }
        break;
    }

    loop {
        if shutdown.is_cancelled() {
            return Ok(());
        }

        best = match source.best_height().await {
            Ok(b) => {
                status.send_modify(|s| {
                    s.source_observed_at_ms = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    );
                    s.source_error = None;
                });
                b
            }
            Err(e) => {
                warn!(source = %source_name, error = %e, "best_height failed; retrying");
                status.send_modify(|s| s.source_error = Some(e.to_string()));
                let cur = match store.indexed_height() {
                    Ok(v) => v,
                    Err(se) => halt_store!(None, se),
                };
                retry!(cur);
            }
        };
        let indexed_opt = match store.indexed_height() {
            Ok(v) => v,
            Err(e) => halt_store!(None, e),
        };
        // `indexed` is the arithmetic height (0 on an empty store); `cur` is what gets
        // published, and stays `None` for an empty store rather than claiming height 0. Both
        // follow a rollback below.
        let mut indexed = indexed_opt.unwrap_or(0);
        let mut cur = indexed_opt;

        // The fork check runs before the `indexed >= best` short-circuit: a reorg that swaps
        // the tip without changing the height (the common case for a 1-block reorg) is
        // invisible to a height comparison, and would otherwise never be noticed.
        // A parent check that has failed `PARENT_MISMATCH_LIMIT` times at the same height is
        // not the source's chain moving under us — our own tip is the problem (classically an
        // orphan sibling that applied because it built on our tip while nothing builds on it).
        // The plain fork check cannot see that: it compares our tip against the source's id at
        // that height and, if they agree, concludes there is nothing to do. So force it to
        // start one block below our tip, which drops the suspect header no matter what.
        let distrust_tip = parent_mismatch.is_some_and(|(_, n)| n >= PARENT_MISMATCH_LIMIT);
        match fork_check(&store, &source, indexed, distrust_tip).await {
            ForkCheck::Agreed => {}
            ForkCheck::Wait => {
                debug!(
                    height = indexed,
                    "source has no block at our tip height; waiting"
                );
                retry!(cur);
            }
            ForkCheck::TooDeep => halt!(
                cur,
                format!("reindex required: fork deeper than {ROLLBACK_WINDOW} blocks")
            ),
            ForkCheck::StoreError(e) => halt_store!(cur, e),
            ForkCheck::SourceError(e) => {
                warn!(source = %source_name, error = %e, "fork check failed; retrying");
                retry!(cur);
            }
            ForkCheck::RollbackTo(h) => {
                info!(from = indexed, to = h, "fork detected; rolling back");
                let s = store.clone();
                let out = match tokio::task::spawn_blocking(move || s.rollback_to(h)).await {
                    Ok(v) => v,
                    Err(join) => halt!(cur, join.to_string()),
                };
                if let Err(e) = out {
                    halt_store!(cur, e);
                }
                indexed = h;
                cur = Some(indexed);
                // The wanted range moved, so a stall about a body the source would not serve
                // is about a height we are no longer at. A parent-mismatch stall is not: this
                // rollback is the *attempt* to clear it, and until a batch actually applies
                // the wedge is still on. Dropping it here would restart `since_secs` on every
                // rollback→re-apply cycle and silence the re-warn cadence.
                if matches!(
                    stall.as_ref().map(|s| s.origin),
                    None | Some(StallOrigin::MissingBody)
                ) {
                    stall = None;
                }
                parent_mismatch = None;
                publish(cur, best, mode, None, stall.as_ref().map(Stall::info));
            }
        }

        // Computed before the idle short-circuit below: an indexer that has caught up is in
        // tip mode, and `/v1/status` must say so instead of repeating whatever mode the last
        // block-applying iteration happened to use (which, right after a bulk catch-up,
        // means reporting "bulk" forever while sitting idle at the tip).
        mode = if best.saturating_sub(indexed) > cfg.tip_lag_for_bulk {
            Mode::Bulk
        } else {
            Mode::Tip
        };

        if indexed >= best {
            publish(cur, best, mode, None, stall.as_ref().map(Stall::info));
            if sleep_or_shutdown(poll, &shutdown).await {
                return Ok(());
            }
            continue;
        }

        let (batch, concurrency) = match mode {
            Mode::Bulk => (cfg.bulk_batch.max(1), cfg.bulk_concurrency.max(1)),
            Mode::Tip => (1, 1),
        };
        let end = best.min(indexed + batch as u32);

        let fetched = match fetch_range(&source, indexed + 1, end, concurrency).await {
            Ok(b) => b,
            Err(e) => {
                warn!(source = %source_name, error = %e, "fetch failed; retrying");
                retry!(cur);
            }
        };
        let bodies = fetched.bodies;
        if bodies.is_empty() {
            // The source advertised a higher tip than it will serve bodies for; wait it out.
            // If it did so by announcing a header and then refusing its body, ingest can wait
            // forever, so that case is recorded as a stall and published — silence here is
            // what made the 545683 freeze look like a healthy idle indexer.
            if let Some(height) = fetched.missing_body_at {
                let reason = format!(
                    "source {source_name} announced a header at height {height} but serves no block body"
                );
                note_stall(&mut stall, height, StallOrigin::MissingBody, reason);
            }
            retry!(cur);
        }

        // Decoding and applying are CPU/redb work: keep them off the async runtime's threads.
        let s = store.clone();
        let durable = mode == Mode::Tip
            || cfg.durable_every == 0
            || (indexed / cfg.durable_every)
                != ((indexed + bodies.len() as u32) / cfg.durable_every);
        // Returns the new tip height rather than the blocks: nothing downstream needs the
        // decoded bodies, and they are large.
        let applied = tokio::task::spawn_blocking(move || -> Result<u32, ApplyErr> {
            let blocks: Vec<DecodedBlock> = bodies
                .iter()
                .map(|j| decode_block(j))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ApplyErr::Decode(e.to_string()))?;
            let tip = blocks.last().map(|b| b.header.height).unwrap_or(0);
            s.apply_batch(&blocks, durable).map_err(ApplyErr::Store)?;
            Ok(tip)
        })
        .await;
        let applied = match applied {
            Ok(v) => v,
            Err(join) => halt!(cur, join.to_string()),
        };

        match applied {
            Ok(new_indexed) => {
                debug!(
                    from = indexed + 1,
                    to = new_indexed,
                    ?mode,
                    durable,
                    "applied"
                );
                // Progress: whatever the source was withholding, it is behind us now.
                stall = None;
                parent_mismatch = None;
                publish(Some(new_indexed), best, mode, None, None);
            }
            Err(ApplyErr::Store(StoreError::ParentMismatch { height, have, want })) => {
                // Usually the source's chain moved under us mid-batch and the next fork check
                // resolves it. When it does not, the same height fails over and over and
                // ingest looks idle while getting nowhere — the 1789057 wedge. Count the run,
                // and once it is clearly not transient say so and distrust our own tip (see
                // `distrust_tip` above).
                debug!(height, %have, %want, "parent mismatch; re-checking fork");
                let times = match parent_mismatch {
                    Some((h, n)) if h == height => n + 1,
                    // The failing height moved: only the counter restarts. A stall of some
                    // other origin (a body the source withheld) is still true, and a
                    // mismatch stall for the old height is replaced by `note_stall` below
                    // once this run reaches the limit — not silently dropped in between.
                    _ => 1,
                };
                parent_mismatch = Some((height, times));
                if times >= PARENT_MISMATCH_LIMIT {
                    note_stall(
                        &mut stall,
                        height,
                        StallOrigin::ParentMismatch,
                        "parent mismatch persists (orphan sibling applied?)".to_owned(),
                    );
                }
                retry!(cur);
            }
            Err(ApplyErr::Store(e)) => halt_store!(cur, e),
            Err(ApplyErr::Decode(e)) => halt!(cur, format!("undecodable block: {e}")),
        }
    }
}

enum ApplyErr {
    Store(StoreError),
    Decode(String),
}

/// Compares the store's tip id with the source's id at the same height, walking backwards
/// until they agree. A height the store simply has no header for counts as a disagreement: we
/// cannot confirm a common ancestor we do not hold. A store *error*, by contrast, is never a
/// disagreement — it is reported as [`ForkCheck::StoreError`] so the caller halts on the real
/// cause instead of walking the window and blaming a fork.
///
/// With `distrust_tip`, the comparison starts one height *below* the store's tip, so the
/// answer is never [`ForkCheck::Agreed`] and at least one block is always rolled back. The
/// caller uses it when the tip is the suspect: nothing the source says about that height can
/// clear it, because agreeing there is exactly what leaves ingest wedged. Below height 2 there
/// is nothing to walk back to, so it yields [`ForkCheck::Wait`].
async fn fork_check(
    store: &Arc<Store>,
    source: &Arc<dyn BlockSource>,
    indexed: u32,
    distrust_tip: bool,
) -> ForkCheck {
    if distrust_tip && indexed <= 1 {
        // There is no height below 1 to fall back to — height 0 is not a block we hold, so
        // walking there would read as a fork past the window and halt. Wait instead: a chain
        // this short cannot have the deep orphan problem `distrust_tip` exists for.
        return ForkCheck::Wait;
    }
    if indexed == 0 {
        return ForkCheck::Agreed;
    }
    let mut h = if distrust_tip { indexed - 1 } else { indexed };
    loop {
        let ours: Option<Hash32> = match store.header_id_at(h) {
            Ok(v) => v,
            Err(e) => return ForkCheck::StoreError(e),
        };
        let theirs = match source.header_id_at(h).await {
            Ok(v) => v,
            Err(e) => return ForkCheck::SourceError(e),
        };
        match (ours, theirs) {
            // Source is behind us or on a shorter chain: wait, never roll back on absence.
            (_, None) => return ForkCheck::Wait,
            (Some(a), Some(b)) if a == b => {
                return if h == indexed {
                    ForkCheck::Agreed
                } else {
                    ForkCheck::RollbackTo(h)
                }
            }
            _ => {
                if h == 0 || indexed - h >= ROLLBACK_WINDOW {
                    return ForkCheck::TooDeep;
                }
                h -= 1;
            }
        }
    }
}

/// What one height's fetch produced.
enum FetchOne {
    Body(String),
    /// The source has no header at that height: it is simply behind us.
    NoHeader,
    /// The source announced a header but would not serve its body — a hole in the source.
    NoBody,
}

/// The contiguous run of bodies from `lo`, and why it stopped.
struct FetchedRange {
    bodies: Vec<String>,
    /// `Some(h)` when the run stopped at a height the source announced a header for but has
    /// no body for. Distinguished from "the source is behind us" because only this one can
    /// last forever, and so is what a stall is reported on.
    missing_body_at: Option<u32>,
}

/// Fetches `lo..=hi`, `concurrency` requests in flight, preserving height order. Stops early
/// at the first height the source has no block for (a gap makes every later block unusable
/// anyway, since `apply_batch` requires a contiguous run).
async fn fetch_range(
    source: &Arc<dyn BlockSource>,
    lo: u32,
    hi: u32,
    concurrency: usize,
) -> Result<FetchedRange, SourceError> {
    let fetched: Vec<Result<FetchOne, SourceError>> = futures::stream::iter((lo..=hi).map(|h| {
        let src = source.clone();
        async move {
            match src.header_id_at(h).await? {
                None => Ok(FetchOne::NoHeader),
                Some(id) => Ok(match src.full_block_json(&id).await? {
                    Some(json) => FetchOne::Body(json),
                    None => FetchOne::NoBody,
                }),
            }
        }
    }))
    .buffered(concurrency)
    .collect()
    .await;

    let mut out = FetchedRange {
        bodies: Vec::with_capacity(fetched.len()),
        missing_body_at: None,
    };
    for (i, r) in fetched.into_iter().enumerate() {
        match r? {
            FetchOne::Body(json) => out.bodies.push(json),
            FetchOne::NoHeader => break,
            FetchOne::NoBody => {
                out.missing_body_at = Some(lo + i as u32);
                break;
            }
        }
    }
    Ok(out)
}

/// Sleeps for `d`, or returns early. `true` means shutdown was requested.
async fn sleep_or_shutdown(d: Duration, shutdown: &CancellationToken) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(d) => false,
        _ = shutdown.cancelled() => true,
    }
}
