mod api;
mod config;
pub mod dsp;
mod error;
mod library;
mod mpd;
mod state;
mod types;

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

    std::fs::create_dir_all(&config.data_dir).context("create data dir")?;
    std::fs::create_dir_all(&config.cover_cache_dir()).context("create cover cache dir")?;

    let db = library::LibraryDb::open(&config.db_path())?;
    db.migrate()?;

    let dsp = dsp::DspManager::new(
        config.camilladsp_config_path.clone(),
        config.camilladsp_ws_url.clone(),
    );

    let mpd = mpd::Mpd::connect(&config.mpd_host, config.mpd_port).await;

    let state = state::AppState::new(config, db, dsp, mpd);
    state.spawn_status_poller();

    let app = api::router(state).layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
    tracing::info!("oxide-player listening on http://{}", cli.listen);
    axum::serve(listener, app).await?;
    Ok(())
}
