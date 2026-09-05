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
use xp_source::{BlockSource, RustNode};
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

/// Runs the explorer to completion and returns the process exit code: `0` on a clean
/// signal-triggered shutdown, `1` if ingest halted.
async fn run(config_path: PathBuf) -> anyhow::Result<i32> {
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config file {}", config_path.display()))?;
    let cfg = Config::parse(&text)
        .with_context(|| format!("parsing config file {}", config_path.display()))?;

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
    let source: Arc<dyn BlockSource> = Arc::new(RustNode::new(&cfg.source.url));

    let indexed = store.indexed_height().context("reading indexed height")?;
    let (status_tx, status_rx) = watch::channel(IngestStatus {
        indexed,
        best: 0,
        mode: Mode::Bulk,
        source: source.name().to_string(),
        halted: None,
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

    let app_state = xp_api::AppState {
        store: store.clone(),
        status: status_rx,
    };
    let app = xp_api::router(app_state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .with_context(|| format!("binding {}", cfg.bind))?;
    info!(addr = %cfg.bind, "listening");

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
        axum::serve(listener, app)
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
        Ok(Err(_)) => 1,
        Err(_) => 1,
    };
    Ok(exit_code)
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
