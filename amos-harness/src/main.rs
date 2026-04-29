//! Binary entry point for AMOS Harness server

#![allow(clippy::format_in_format_args)]

use amos_core::{AppConfig, Result};
use amos_harness::create_server;
use amos_harness::shutdown::shutdown_signal;
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    init_tracing();

    info!("Starting AMOS Harness v{}", amos_harness::VERSION);

    // Load configuration
    let config = Arc::new(AppConfig::load()?);
    info!("Configuration loaded");

    // ── TLS enforcement for production ──────────────────────────────
    // In production (AMOS__ENV=production) the harness refuses to start if
    // the database or Redis URLs are not configured for TLS. In development
    // the same issues are surfaced as warnings so operators can fix them
    // before promoting to production.
    if AppConfig::is_production_env() {
        config.validate_startup()?;
    } else {
        for issue in config.tls_issues() {
            tracing::info!(
                "{} (acceptable for dev; would block startup in production)",
                issue
            );
        }
    }

    // Connect to PostgreSQL
    info!("Connecting to PostgreSQL");
    let db_pool = PgPoolOptions::new()
        .max_connections(config.database.pool_size)
        .connect(config.database.url.expose_secret())
        .await
        .map_err(|e| {
            amos_core::AmosError::Internal(format!(
                "Database: Failed to connect to database: {}",
                e
            ))
        })?;
    info!("Database connection established");

    info!("Running database migrations");
    sqlx::migrate!("./migrations")
        .run(&db_pool)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("Migration failed: {}", e)))?;
    info!("Migrations completed");

    // Connect to Redis
    info!("Connecting to Redis");
    let redis_client = redis::Client::open(config.redis.url.as_str()).map_err(|e| {
        amos_core::AmosError::Internal(format!("Cache: Failed to connect to Redis: {}", e))
    })?;

    // Verify Redis connection
    let mut conn = redis_client.get_connection().map_err(|e| {
        amos_core::AmosError::Internal(format!(
            "Cache: {}",
            format!("Failed to get Redis connection: {}", e)
        ))
    })?;
    redis::cmd("PING").query::<String>(&mut conn).map_err(|e| {
        amos_core::AmosError::Internal(format!("Cache: {}", format!("Redis PING failed: {}", e)))
    })?;
    info!("Redis connection established");

    // Initialize harness
    amos_harness::init().await?;

    // Create and start server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting server on {}", addr);

    let app = create_server(config, db_pool, redis_client).await?;

    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        amos_core::AmosError::Internal(format!("Failed to bind to {}: {}", addr, e))
    })?;

    info!("Server listening on {}", addr);

    // Graceful shutdown: SIGTERM/SIGINT (Unix) or Ctrl-C (non-Unix) stops
    // accepting new connections and drains in-flight requests — including
    // long-running SSE chat streams. A 30s watchdog (see `shutdown_signal`)
    // force-exits if the drain hangs so rolling deploys don't stall.
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("Server error: {}", e)))?;

    info!("Server shut down cleanly");
    Ok(())
}

/// Initialize tracing with configured log level and formatting
fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,amos_harness=debug,amos_core=debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_level(true)
                .with_ansi(true),
        )
        .init();
}
