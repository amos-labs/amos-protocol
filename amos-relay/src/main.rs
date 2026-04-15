//! # AMOS Network Relay Binary
//!
//! Main entry point for the AMOS Network Relay service.
//!
//! This binary starts the HTTP API server on port 4100 for:
//! - Global bounty marketplace
//! - Agent directory and discovery
//! - Cross-harness reputation tracking
//! - Protocol fee management

use amos_core::AppConfig;
use amos_relay::{server, RelayState, Result, VERSION};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Load relay-specific .env before the shared AppConfig reads the workspace root .env.
    // This ensures AMOS__DATABASE__URL points at amos_relay_dev, not amos_harness_development.
    let relay_env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    if relay_env.exists() {
        dotenvy::from_path(&relay_env).ok();
    }

    // Initialize tracing
    init_tracing()?;

    info!("Starting AMOS Network Relay v{}", VERSION);

    // Load configuration
    let config = AppConfig::load()?;
    info!("Configuration loaded: HTTP port={}", config.server.port);

    // Initialize relay state (DB, Redis, optional Solana)
    let state = RelayState::new(config).await?;
    info!("Relay state initialized successfully");

    // Run database migrations
    state.run_migrations().await?;
    info!("Database migrations completed");

    // Start HTTP server
    let http_server = server::start_http_server(state);

    tokio::select! {
        result = http_server => {
            error!("HTTP server exited: {:?}", result);
            result
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down gracefully");
            Ok(())
        }
    }
}

/// Initialize tracing with JSON logging.
fn init_tracing() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,amos_relay=debug,amos_core=debug"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .json();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init()
        .map_err(|e| amos_core::AmosError::Internal(format!("Failed to init tracing: {}", e)))?;

    Ok(())
}
