//! The ingest pipeline: fetch → fork-check → apply.
//!
//! [`run`] drives a [`Store`] towards a [`BlockSource`]'s best chain forever, until the
//! supplied [`CancellationToken`] is cancelled or an unrecoverable condition (a fork deeper
//! than [`ROLLBACK_WINDOW`], a corrupt store, an undecodable block) forces it to halt.
//! Progress is published on a [`watch`] channel so a server can serve it as a health page.

use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
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
    pub indexed: Option<u32>,
    pub best: u32,
    pub mode: Mode,
    pub source: String,
    /// `Some(reason)` once ingest has given up; [`run`] returns an error immediately after
    /// publishing it.
    pub halted: Option<String>,
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

    let publish = |indexed: Option<u32>, best: u32, mode: Mode, halted: Option<String>| {
        let _ = status.send(IngestStatus {
            indexed,
            best,
            mode,
            source: source_name.clone(),
            halted,
        });
    };
    // Publishes `halted` and turns the reason into the error `run` returns.
    macro_rules! halt {
        ($indexed:expr, $reason:expr) => {{
            let reason: String = $reason;
            publish($indexed, best, mode, Some(reason.clone()));
            return Err(anyhow::anyhow!(reason));
        }};
    }

    loop {
        if shutdown.is_cancelled() {
            return Ok(());
        }

        best = match source.best_height().await {
            Ok(b) => b,
            Err(e) => {
                warn!(source = %source_name, error = %e, "best_height failed; retrying");
                publish(store.indexed_height().ok().flatten(), best, mode, None);
                if sleep_or_shutdown(poll, &shutdown).await {
                    return Ok(());
                }
                continue;
            }
        };
        let indexed_opt = store.indexed_height()?;
        let mut indexed = indexed_opt.unwrap_or(0);
        // `Some(indexed)` once anything is indexed, tracking rollbacks below.
        let mut cur = indexed_opt;

        // The fork check runs before the `indexed >= best` short-circuit: a reorg that swaps
        // the tip without changing the height (the common case for a 1-block reorg) is
        // invisible to a height comparison, and would otherwise never be noticed.
        match fork_check(&store, &source, indexed).await {
            ForkCheck::Agreed => {}
            ForkCheck::Wait => {
                debug!(
                    height = indexed,
                    "source has no block at our tip height; waiting"
                );
                publish(indexed_opt, best, mode, None);
                if sleep_or_shutdown(poll, &shutdown).await {
                    return Ok(());
                }
                continue;
            }
            ForkCheck::TooDeep => halt!(
                indexed_opt,
                format!("reindex required: fork deeper than {ROLLBACK_WINDOW} blocks")
            ),
            ForkCheck::SourceError(e) => {
                warn!(source = %source_name, error = %e, "fork check failed; retrying");
                publish(indexed_opt, best, mode, None);
                if sleep_or_shutdown(poll, &shutdown).await {
                    return Ok(());
                }
                continue;
            }
            ForkCheck::RollbackTo(h) => {
                info!(from = indexed, to = h, "fork detected; rolling back");
                let s = store.clone();
                let out = tokio::task::spawn_blocking(move || s.rollback_to(h)).await?;
                if let Err(e) = out {
                    halt!(indexed_opt, format!("rollback to {h} failed: {e}"));
                }
                indexed = h;
                cur = Some(indexed);
                publish(cur, best, mode, None);
            }
        }

        if indexed >= best {
            publish(cur, best, mode, None);
            if sleep_or_shutdown(poll, &shutdown).await {
                return Ok(());
            }
            continue;
        }

        mode = if best - indexed > cfg.tip_lag_for_bulk {
            Mode::Bulk
        } else {
            Mode::Tip
        };
        let (batch, concurrency) = match mode {
            Mode::Bulk => (cfg.bulk_batch.max(1), cfg.bulk_concurrency.max(1)),
            Mode::Tip => (1, 1),
        };
        let end = best.min(indexed + batch as u32);

        let bodies = match fetch_range(&source, indexed + 1, end, concurrency).await {
            Ok(b) => b,
            Err(e) => {
                warn!(source = %source_name, error = %e, "fetch failed; retrying");
                publish(Some(indexed), best, mode, None);
                if sleep_or_shutdown(poll, &shutdown).await {
                    return Ok(());
                }
                continue;
            }
        };
        if bodies.is_empty() {
            // The source advertised a higher tip than it will serve bodies for; wait it out.
            publish(Some(indexed), best, mode, None);
            if sleep_or_shutdown(poll, &shutdown).await {
                return Ok(());
            }
            continue;
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
        .await?;

        match applied {
            Ok(new_indexed) => {
                debug!(
                    from = indexed + 1,
                    to = new_indexed,
                    ?mode,
                    durable,
                    "applied"
                );
                publish(Some(new_indexed), best, mode, None);
            }
            Err(ApplyErr::Store(StoreError::ParentMismatch { height, have, want })) => {
                // The source's chain moved under us mid-batch; the next fork check resolves it.
                debug!(height, %have, %want, "parent mismatch; re-checking fork");
            }
            Err(ApplyErr::Store(StoreError::ReindexRequired(n))) => {
                halt!(
                    Some(indexed),
                    format!("reindex required: fork deeper than {n} blocks")
                );
            }
            Err(ApplyErr::Store(e)) => halt!(Some(indexed), format!("store error: {e}")),
            Err(ApplyErr::Decode(e)) => halt!(Some(indexed), format!("undecodable block: {e}")),
        }
    }
}

enum ApplyErr {
    Store(StoreError),
    Decode(String),
}

/// Compares the store's tip id with the source's id at the same height, walking backwards
/// until they agree. A height the store has no header for counts as a disagreement: we cannot
/// confirm a common ancestor we do not hold.
async fn fork_check(store: &Arc<Store>, source: &Arc<dyn BlockSource>, indexed: u32) -> ForkCheck {
    if indexed == 0 {
        return ForkCheck::Agreed;
    }
    let mut h = indexed;
    loop {
        // A read failure is treated as "we don't have it", i.e. a disagreement: the walk
        // continues and, if nothing ever agrees, halts with a reindex request.
        let ours: Option<Hash32> = store.header_id_at(h).unwrap_or_default();
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

/// Fetches `lo..=hi`, `concurrency` requests in flight, preserving height order. Stops early
/// at the first height the source has no block for (a gap makes every later block unusable
/// anyway, since `apply_batch` requires a contiguous run).
async fn fetch_range(
    source: &Arc<dyn BlockSource>,
    lo: u32,
    hi: u32,
    concurrency: usize,
) -> Result<Vec<String>, SourceError> {
    let fetched: Vec<Result<Option<String>, SourceError>> =
        futures::stream::iter((lo..=hi).map(|h| {
            let src = source.clone();
            async move {
                match src.header_id_at(h).await? {
                    None => Ok(None),
                    Some(id) => src.full_block_json(&id).await,
                }
            }
        }))
        .buffered(concurrency)
        .collect()
        .await;

    let mut out = Vec::with_capacity(fetched.len());
    for r in fetched {
        match r? {
            Some(json) => out.push(json),
            None => break,
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
