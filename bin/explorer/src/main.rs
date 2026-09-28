//! `explorer`: standalone binary that indexes an Ergo node's blocks into a local [`xp_store`]
//! and serves them over [`xp_api`]'s read-only HTTP surface.
//!
//! Usage: `explorer [--config <path>]` (default: `explorer.toml`).

mod config;

use anyhow::Context;
use config::Config;
use std::future::IntoFuture;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use xp_ingest::{IngestConfig, IngestStatus, Mode};
use xp_source::{BlockSource, Fallback, RustNode};
use xp_store::Store;

fn print_usage() {
    eprintln!("usage: explorer [--config <path>]  (default: explorer.toml)");
}

/// Parses `--config <path>` by hand (no argument-parsing dependency, per the workspace's
/// dependency-light policy for this binary). Anything else is a usage error.
fn parse_args<I: Iterator<Item = String>>(mut args: I) -> anyhow::Result<PathBuf> {
    let path = match args.next() {
        None => PathBuf::from("explorer.toml"),
        Some(flag) if flag == "--config" => {
            let value = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("--config requires a path argument"))?;
            PathBuf::from(value)
        }
        Some(other) => anyhow::bail!("unrecognized argument: {other}"),
    };
    if args.next().is_some() {
        anyhow::bail!("unexpected extra arguments");
    }
    Ok(path)
}

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,xp_ingest=info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .init();
}

/// Resolves once either Ctrl+C or (on unix) SIGTERM is received.
async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                warn!("failed to install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Exit code for an ingest halt caused by [`xp_store::StoreError::ReindexRequired`]: the
/// store cannot go forward without a full reindex, so restarting the process is pointless.
/// Operators running under `Restart=on-failure` exclude it with
/// `RestartPreventExitStatus=3` rather than looping the unit forever.
const EXIT_REINDEX_REQUIRED: i32 = 3;

const TOKEN_NAME_BACKFILL_BATCH: usize = 500;
const TOKEN_NAME_BACKFILL_PAUSE: Duration = Duration::from_millis(25);
const TOKEN_NAME_ANCHOR_PAUSE: Duration = Duration::from_secs(1);

/// One bounded blocking batch at a time, leaving the writer available to ingest between
/// batches. Cancellation never drops a running blocking task: the caller joins this worker
/// before shutdown completes, including the last commit that was already in progress.
async fn backfill_token_names<F>(batch: F, shutdown: CancellationToken)
where
    F: Fn() -> Result<xp_store::token_search::TokenNameIndexStatus, xp_store::StoreError>
        + Send
        + Sync
        + 'static,
{
    let batch = Arc::new(batch);
    loop {
        if shutdown.is_cancelled() {
            return;
        }
        let run_batch = batch.clone();
        let batch_shutdown = shutdown.clone();
        // Do not select cancellation against this await: spawn_blocking cannot be aborted
        // once running. A queued task also checks cancellation before touching the store.
        let result = tokio::task::spawn_blocking(move || {
            (!batch_shutdown.is_cancelled()).then(|| run_batch())
        })
        .await;
        let pause = match result {
            Ok(Some(Ok(status))) if status.ready => {
                info!(
                    indexed_names = status.indexed_names,
                    unindexed_tokens = status.unindexed_tokens,
                    "token-name search index ready"
                );
                return;
            }
            Ok(Some(Ok(_))) => TOKEN_NAME_BACKFILL_PAUSE,
            // A partial start can legitimately have no indexed header until ingest commits
            // its first block. Keep search unavailable and wait for that canonical anchor.
            Ok(Some(Err(xp_store::StoreError::TokenSearchNotReady))) => TOKEN_NAME_ANCHOR_PAUSE,
            Ok(Some(Err(error))) => {
                warn!(%error, "token-name backfill stopped; search remains unavailable, ordinary indexing continues");
                return;
            }
            Ok(None) => return,
            Err(error) => {
                warn!(%error, "token-name backfill worker failed; ordinary indexing continues");
                return;
            }
        };
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(pause) => {}
        }
    }
}

/// Runs the explorer to completion and returns the process exit code: `0` on a clean
/// signal-triggered shutdown, [`EXIT_REINDEX_REQUIRED`] if ingest halted because the store
/// needs a reindex, `1` for any other halt.
async fn run(config_path: PathBuf) -> anyhow::Result<i32> {
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config file {}", config_path.display()))?;
    let cfg = Config::parse(&text)
        .with_context(|| format!("parsing config file {}", config_path.display()))?;
    // Validated up front so a bad allowlist/CIDR exits 1 like any other config error,
    // before we touch the store or bind a socket.
    let api_cfg = xp_api::ApiConfig::try_from(&cfg.api).map_err(|e| anyhow::anyhow!(e))?;

    std::fs::create_dir_all(&cfg.data_dir)
        .with_context(|| format!("creating data_dir {}", cfg.data_dir.display()))?;
    let db_path = cfg.data_dir.join("explorer.redb");

    info!(
        data_dir = %cfg.data_dir.display(),
        bind = %cfg.bind,
        source_url = %cfg.source.url,
        "starting explorer"
    );

    // Store is held here for the whole run and dropped last, after the server and the ingest
    // task have both stopped touching it.
    let store = Arc::new(
        Store::open_with_register_index_ceiling(&db_path, cfg.register_index_ceiling)
            .context("opening store")?,
    );
    let entries = store
        .register_index_entries()
        .context("reading register occupancy")?;
    match cfg.register_index_ceiling {
        None => warn!(
            entries,
            "no register ceiling configured; register index growth is unbounded"
        ),
        Some(ceiling) => info!(entries, ceiling, "register index ceiling enforced"),
    }
    let primary: Arc<dyn BlockSource> = Arc::new(RustNode::new(&cfg.source.url));
    // A fallback only ever supplies block *bodies* the primary announces but won't serve; the
    // chain being followed still comes from the primary alone (see `xp_source::Fallback`).
    let source: Arc<dyn BlockSource> = match cfg.source.fallback_url.as_deref() {
        None => primary,
        Some(url) => {
            info!(fallback_url = %url, "block-body fallback source enabled");
            Arc::new(Fallback::new(primary, RustNode::new(url)))
        }
    };

    // Bind before spawning ingest or backfill: a bad address must fail fast before any
    // background writer starts, so there is nothing running to cancel when `?` returns.
    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .with_context(|| format!("binding {}", cfg.bind))?;
    info!(addr = %cfg.bind, "listening");

    let indexed = store.indexed_height().context("reading indexed height")?;
    let (status_tx, status_rx) = watch::channel(IngestStatus {
        source_observed_at_ms: None,
        source_error: None,
        indexed,
        best: 0,
        mode: Mode::Bulk,
        source: source.name().to_string(),
        halted: None,
        stalled: None,
    });

    let ingest_cfg: IngestConfig = (&cfg.ingest).into();
    let shutdown = CancellationToken::new();

    let mut ingest_handle = tokio::spawn(xp_ingest::run(
        store.clone(),
        source.clone(),
        ingest_cfg,
        status_tx,
        shutdown.clone(),
    ));
    let backfill_store = store.clone();
    let backfill_handle = tokio::spawn(backfill_token_names(
        move || backfill_store.backfill_token_names_batch(TOKEN_NAME_BACKFILL_BATCH),
        shutdown.clone(),
    ));

    info!(
        per_second = api_cfg.per_second,
        burst = api_cfg.burst,
        max_inflight_reads = api_cfg.max_inflight_reads,
        allowlist = cfg.api.rate_limit.allowlist.len(),
        "api limits"
    );
    let app_state = xp_api::AppState {
        store: store.clone(),
        status: status_rx,
        counters: Arc::new(xp_api::Counters::default()),
        read_permits: Arc::new(tokio::sync::Semaphore::new(
            api_cfg.max_inflight_reads as usize,
        )),
    };
    let app = xp_api::router(app_state, &api_cfg).layer(axum::Extension(source));

    {
        let signal_shutdown = shutdown.clone();
        tokio::spawn(async move {
            wait_for_shutdown_signal().await;
            info!("shutdown signal received");
            signal_shutdown.cancel();
        });
    }

    let mut server_fut: std::pin::Pin<
        Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>,
    > = Box::pin(
        // `into_make_service_with_connect_info` is what puts the peer address in each
        // request's extensions; without it the rate limiter would key every client alike.
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown({
            let shutdown = shutdown.clone();
            async move { shutdown.cancelled().await }
        })
        .into_future(),
    );

    // Race the server against the ingest task so a halt (ingest returning Err while the
    // server is still up) is caught immediately and turned into a shutdown, rather than
    // leaving the server serving increasingly stale data until a signal happens to arrive.
    let (ingest_outcome, server_failed) = tokio::select! {
        biased;
        ingest_res = &mut ingest_handle => {
            match &ingest_res {
                Ok(Ok(())) => info!("ingest task finished before shutdown was requested"),
                Ok(Err(e)) => error!("ingest halted: {e:#}"),
                Err(join_err) => error!("ingest task panicked: {join_err}"),
            }
            // Halt (or an unexpected clean exit) still needs the server to stop.
            shutdown.cancel();
            let server_res = server_fut.await;
            if let Err(e) = &server_res {
                error!("server error: {e:#}");
            }
            (ingest_res, server_res.is_err())
        }
        server_res = &mut server_fut => {
            if let Err(e) = &server_res {
                error!("server error: {e:#}");
            }
            // The server stopped (normally: a signal cancelled `shutdown`). Make sure ingest
            // is told to stop too, then let it finish its current batch.
            shutdown.cancel();
            (ingest_handle.await, server_res.is_err())
        }
    };

    // The signal/server/ingest paths above all cancel shutdown. Wait out an admitted
    // backfill batch as well, so the process never exits with a detached database write.
    if let Err(error) = backfill_handle.await {
        warn!(%error, "token-name backfill task failed while joining shutdown");
    }

    Ok(explorer_exit_code(ingest_outcome, server_failed))
}

fn explorer_exit_code(
    outcome: Result<anyhow::Result<()>, tokio::task::JoinError>,
    server_failed: bool,
) -> i32 {
    match outcome {
        Ok(Ok(())) => i32::from(server_failed),
        Ok(Err(e)) if needs_reindex(&e) => EXIT_REINDEX_REQUIRED,
        Ok(Err(_)) => 1,
        Err(_) => 1,
    }
}

/// Whether an ingest error chain bottoms out in [`xp_store::StoreError::ReindexRequired`].
/// Checked through the chain rather than on the top-level error because `xp_ingest` wraps
/// the store error in its own context before returning it.
fn needs_reindex(err: &anyhow::Error) -> bool {
    err.chain().any(|c| {
        matches!(
            c.downcast_ref::<xp_store::StoreError>(),
            Some(xp_store::StoreError::ReindexRequired(_))
        )
    })
}

#[tokio::main]
async fn main() {
    let config_path = match parse_args(std::env::args().skip(1)) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{e}");
            print_usage();
            std::process::exit(2);
        }
    };

    init_tracing();

    match run(config_path).await {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            error!("fatal: {e:#}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_status(ready: bool) -> xp_store::token_search::TokenNameIndexStatus {
        xp_store::token_search::TokenNameIndexStatus {
            version: xp_store::token_search::TOKEN_NAME_INDEX_VERSION,
            ready,
            phase: if ready { "ready" } else { "building" },
            scanned_tokens: 0,
            total_tokens: 0,
            indexed_names: 0,
            unindexed_tokens: 0,
        }
    }

    #[tokio::test]
    async fn token_name_worker_reaches_ready_with_the_real_store() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "kadia-token-name-worker-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let store = Arc::new(Store::open(&directory.join("explorer.redb")).unwrap());
        let worker_store = store.clone();
        tokio::time::timeout(
            Duration::from_secs(5),
            backfill_token_names(
                move || worker_store.backfill_token_names_batch(TOKEN_NAME_BACKFILL_BATCH),
                CancellationToken::new(),
            ),
        )
        .await
        .expect("ready store must finish promptly");
        assert!(
            xp_store::Reader::new(&store)
                .unwrap()
                .token_name_index_status()
                .unwrap()
                .ready
        );
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn token_name_worker_repeats_bounded_batches_and_stops_at_ready() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        tokio::time::timeout(
            Duration::from_secs(5),
            backfill_token_names(
                move || {
                    Ok(build_status(
                        worker_calls.fetch_add(1, Ordering::SeqCst) == 2,
                    ))
                },
                CancellationToken::new(),
            ),
        )
        .await
        .expect("three bounded batches must finish");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn token_name_worker_retries_until_ingest_provides_an_anchor() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        tokio::time::timeout(
            Duration::from_secs(5),
            backfill_token_names(
                move || {
                    if worker_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        Err(xp_store::StoreError::TokenSearchNotReady)
                    } else {
                        Ok(build_status(true))
                    }
                },
                CancellationToken::new(),
            ),
        )
        .await
        .expect("missing initial anchor must be retried");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn token_name_worker_cancels_during_the_anchor_wait() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        let shutdown = CancellationToken::new();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let started = std::sync::Mutex::new(Some(started_tx));
        let handle = tokio::spawn(backfill_token_names(
            move || {
                worker_calls.fetch_add(1, Ordering::SeqCst);
                if let Some(started) = started.lock().unwrap().take() {
                    started.send(()).unwrap();
                }
                Err(xp_store::StoreError::TokenSearchNotReady)
            },
            shutdown.clone(),
        ));
        tokio::time::timeout(Duration::from_secs(5), started_rx)
            .await
            .expect("anchor check starts")
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "worker must keep waiting for an anchor"
        );
        shutdown.cancel();
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("cancellation must interrupt the anchor retry delay")
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn token_name_worker_cancellation_joins_an_inflight_write() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let shutdown = CancellationToken::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let started = std::sync::Mutex::new(Some(started_tx));
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let release = std::sync::Mutex::new(release_rx);
        let mut handle = tokio::spawn(backfill_token_names(
            move || {
                worker_calls.fetch_add(1, Ordering::SeqCst);
                started.lock().unwrap().take().unwrap().send(()).unwrap();
                release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                Ok(build_status(false))
            },
            shutdown.clone(),
        ));
        tokio::time::timeout(Duration::from_secs(5), started_rx)
            .await
            .expect("batch starts")
            .unwrap();
        shutdown.cancel();
        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut handle)
                .await
                .is_err(),
            "cancellation must wait for an already admitted write"
        );
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("worker joins after admitted write completes")
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn token_name_worker_cancelled_before_start_does_not_write() {
        let shutdown = CancellationToken::new();
        shutdown.cancel();
        tokio::time::timeout(
            Duration::from_secs(5),
            backfill_token_names(|| panic!("cancelled worker must not write"), shutdown),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn token_name_worker_failure_preserves_service_cancellation_state() {
        let shutdown = CancellationToken::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            backfill_token_names(
                || Err(xp_store::StoreError::ReadLimit("test batch failure")),
                shutdown.clone(),
            ),
        )
        .await
        .expect("failed optional index worker must return");
        assert!(!shutdown.is_cancelled());
    }

    #[test]
    fn parse_args_defaults_and_flag() {
        assert_eq!(
            parse_args(std::iter::empty()).unwrap(),
            PathBuf::from("explorer.toml")
        );
        assert_eq!(
            parse_args(["--config".to_string(), "a.toml".to_string()].into_iter()).unwrap(),
            PathBuf::from("a.toml")
        );
        assert!(parse_args(["--config".to_string()].into_iter()).is_err());
        assert!(parse_args(["oops".to_string()].into_iter()).is_err());
    }

    #[test]
    fn explorer_exit_code_reindex_required_returns_three() {
        let err = anyhow::Error::from(xp_store::StoreError::ReindexRequired(
            xp_store::ROLLBACK_WINDOW,
        ))
        .context("ingest loop");
        assert!(needs_reindex(&err));
        assert_eq!(explorer_exit_code(Ok(Err(err)), false), 3);
    }

    #[test]
    fn explorer_exit_code_other_outcomes_preserve_codes() {
        assert_eq!(explorer_exit_code(Ok(Ok(())), false), 0);
        assert_eq!(
            explorer_exit_code(Ok(Err(anyhow::anyhow!("decode failed"))), false),
            1
        );
        let err = xp_store::StoreError::Corrupt("input box missing").into();
        assert_eq!(explorer_exit_code(Ok(Err(err)), false), 1);
    }

    #[test]
    fn explorer_exit_code_server_failed_ingest_clean_returns_one() {
        assert_eq!(explorer_exit_code(Ok(Ok(())), true), 1);
    }

    #[test]
    fn explorer_exit_code_server_failed_ingest_reindex_returns_three() {
        let err = xp_store::StoreError::ReindexRequired(5_000).into();
        assert_eq!(explorer_exit_code(Ok(Err(err)), true), 3);
    }

    #[test]
    fn explorer_exit_code_server_ok_ingest_clean_returns_zero() {
        assert_eq!(explorer_exit_code(Ok(Ok(())), false), 0);
    }

    /// `xp_ingest` wraps the store error before returning it, so the check has to walk the
    /// chain rather than look only at the outermost error.
    #[test]
    fn needs_reindex_finds_the_error_through_the_chain() {
        let deep = anyhow::Error::from(xp_store::StoreError::ReindexRequired(5_000))
            .context("applying batch")
            .context("ingest loop");
        assert!(needs_reindex(&deep));

        let other = anyhow::Error::from(xp_store::StoreError::Corrupt("input box missing"))
            .context("applying batch");
        assert!(!needs_reindex(&other));
        assert!(!needs_reindex(&anyhow::anyhow!("plain string error")));
    }
}
