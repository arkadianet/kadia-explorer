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
    let store = Arc::new(Store::open(&db_path).context("opening store")?);
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

    // Bind before spawning ingest: a bad `bind` address must fail fast without the ingest
    // task ever having touched the store, so there is nothing running yet to cancel or wait
    // out when `?` returns early here.
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
    let app = xp_api::router(app_state, &api_cfg);

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
    let ingest_outcome = tokio::select! {
        biased;
        ingest_res = &mut ingest_handle => {
            match &ingest_res {
                Ok(Ok(())) => info!("ingest task finished before shutdown was requested"),
                Ok(Err(e)) => error!("ingest halted: {e:#}"),
                Err(join_err) => error!("ingest task panicked: {join_err}"),
            }
            // Halt (or an unexpected clean exit) still needs the server to stop.
            shutdown.cancel();
            if let Err(e) = server_fut.await {
                error!("server error: {e:#}");
            }
            ingest_res
        }
        server_res = &mut server_fut => {
            if let Err(e) = server_res {
                error!("server error: {e:#}");
            }
            // The server stopped (normally: a signal cancelled `shutdown`). Make sure ingest
            // is told to stop too, then let it finish its current batch.
            shutdown.cancel();
            ingest_handle.await
        }
    };

    let exit_code = match ingest_outcome {
        Ok(Ok(())) => 0,
        Ok(Err(e)) if needs_reindex(&e) => EXIT_REINDEX_REQUIRED,
        Ok(Err(_)) => 1,
        Err(_) => 1,
    };
    Ok(exit_code)
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
