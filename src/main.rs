//! Configuration and wiring. Behaviour lives in the crates under `crates/`.
//!
//! This is the composition root, and it is the only file that knows which implementations are
//! live: which price source, which database, which address. Every crate below it is handed
//! what it needs and knows nothing about where it came from, which is what makes swapping one
//! a change here and nowhere else (N-54).

mod config;

use std::net::SocketAddr;
use std::sync::Arc;

use config::Config;
use watcher_api::AppState;
use watcher_feeds::ConfiguredPriceSource;

/// Caddy is the only way in (C-21, C-22), so the port is not configurable: the compose file
/// does not publish it and the proxy is what enforces basic auth. A variable here would be a
/// variable that must agree with the Caddyfile, and the two would eventually disagree.
const PORT: u16 = 8080;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env().map_err(|e| format!("configuration error: {e}"))?;

    // Migrations first, as the owner role, before anything connects as the application. The
    // alternative is applying them by hand before a deploy, which is a step that gets skipped
    // exactly once — and the symptom is a binary querying a table that does not exist.
    watcher_store::migrate(config.migrate_database_url.expose())
        .await
        .map_err(|e| format!("migration error: {e}"))?;

    let db = watcher_store::connect(config.database_url.expose())
        .await
        .map_err(|e| format!("database error: {e}"))?;

    let prices = ConfiguredPriceSource::from_config(&config.price_source)
        .map_err(|e| format!("price source error: {e}"))?;

    let address = SocketAddr::from(([0, 0, 0, 0], PORT));
    println!(
        "watcher listening on {address}: model {} via {}, prices from {}",
        config.model_name,
        config.model_provider,
        prices.name()
    );

    let state = AppState {
        db,
        prices: Arc::new(prices),
        // One seeded user until there is a login (D14). The id comes from the migration that
        // inserted it rather than from configuration, because it is a fact about the schema.
        user_id: watcher_store::OWNER_USER_ID,
    };

    watcher_api::serve(address, watcher_api::router(state))
        .await
        .map_err(|e| format!("server error: {e}"))?;

    Ok(())
}
