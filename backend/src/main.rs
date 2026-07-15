mod api;
mod config;
pub mod dsp;
mod error;
mod library;
mod mpd;
mod state;
mod types;

use crate::dsp::camilladsp::{DEFAULT_CAPTURE_DEVICE, DEFAULT_CAPTURE_RATE};
use crate::state::AppState;
use anyhow::Context;
use clap::Parser;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = config::Cli::parse();
    let config = config::Config::load(cli.config.as_deref(), &cli)?;

    if !cli.allow_root && running_as_root() {
        tracing::warn!(
            "oxide-player is running as the root user with no built-in privilege \
             drop. Prefer a dedicated unprivileged user (e.g. the shipped systemd \
             unit uses User=oxide). Set --allow-root to silence this warning."
        );
    }

    std::fs::create_dir_all(&config.data_dir).context("create data dir")?;
    std::fs::create_dir_all(&config.cover_cache_dir()).context("create cover cache dir")?;

    let db = library::LibraryDb::open(&config.db_path())?;
    db.migrate()?;

    let dsp = dsp::DspManager::new(
        config.camilladsp_config_path.clone(),
        config.camilladsp_ws_url.clone(),
        config
            .camilladsp_capture_device
            .clone()
            .unwrap_or_else(|| DEFAULT_CAPTURE_DEVICE.to_string()),
        config.camilladsp_capture_rate.unwrap_or(DEFAULT_CAPTURE_RATE),
    );

    let mpd = mpd::Mpd::connect(&config.mpd_host, config.mpd_port).await;

    let state = state::AppState::new(config, db, dsp, mpd);
    state.spawn_status_poller();

    // The frontend is served from this same origin, so no cross-origin access
    // is needed. A permissive layer would let any website the user visits drive
    // the (currently unauthenticated) API. Tighten this if you expose the server
    // and serve the UI from a different origin.
    let app = api::router(state.clone()).layer(CorsLayer::new());

    let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
    tracing::info!("oxide-player listening on http://{}", cli.listen);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state))
        .await?;
    Ok(())
}

/// Resolve when the process receives a termination signal (SIGINT/SIGTERM on
/// Unix, Ctrl-C / close on other platforms). On trigger we stop MPD playback so
/// the process does not leave music running after it exits. This is a deliberate
/// choice: oxide-player is a control layer over MPD, but we halt playback rather
/// than let it continue unattended once the controller is gone.
async fn shutdown_signal(state: AppState) {
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = sigint.recv() => {},
            _ = sigterm.recv() => {},
        }
    };
    #[cfg(not(unix))]
    let terminate = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    terminate.await;

    tracing::info!("shutting down: stopping playback");
    if let Err(e) = state.mpd().stop().await {
        tracing::warn!("failed to stop playback on exit: {e}");
    }
}

/// Best-effort check for uid 0. Reads the real uid from /proc on Unix systems;
/// returns false on other platforms (where we cannot cheaply determine it).
fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("Uid:") {
                    // Real uid is the first field.
                    if let Some(uid) = rest.split_whitespace().next() {
                        return uid == "0";
                    }
                }
            }
        }
        false
    }
    #[cfg(not(unix))]
    {
        false
    }
}
