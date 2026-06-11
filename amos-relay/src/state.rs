//! Relay application state shared across all handlers.

use amos_core::{AmosError, AppConfig, Result};
use redis::aio::ConnectionManager;
use secrecy::ExposeSecret;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use tracing::{info, warn};

use crate::solana::SolanaClient;

/// Shared application state for the AMOS Network Relay.
///
/// This struct is cloned cheaply (via Arc internally) and passed
/// to every HTTP handler.
#[derive(Clone)]
pub struct RelayState {
    /// PostgreSQL connection pool.
    pub db: PgPool,
    /// Redis connection manager.
    pub redis: ConnectionManager,
    /// Application configuration.
    pub config: Arc<AppConfig>,
    /// Optional Solana RPC client (None if feature disabled or connection failed).
    pub solana: Option<Arc<SolanaClient>>,
}

impl RelayState {
    /// Initialize relay state with database, Redis, and optional Solana client.
    pub async fn new(config: AppConfig) -> Result<Self> {
        // Connect to PostgreSQL
        info!("Connecting to PostgreSQL...");
        let db = PgPoolOptions::new()
            .max_connections(config.database.pool_size)
            .min_connections(config.database.min_connections)
            .acquire_timeout(std::time::Duration::from_secs(
                config.database.acquire_timeout_secs,
            ))
            .idle_timeout(std::time::Duration::from_secs(
                config.database.idle_timeout_secs,
            ))
            .max_lifetime(std::time::Duration::from_secs(
                config.database.max_lifetime_secs,
            ))
            .connect(config.database.url.expose_secret())
            .await
            .map_err(AmosError::Database)?;
        info!("PostgreSQL connection pool established");

        // Connect to Redis
        info!("Connecting to Redis at {}...", config.redis.url);
        let redis_client = redis::Client::open(config.redis.url.as_str())
            .map_err(|e| AmosError::Internal(format!("Failed to create Redis client: {}", e)))?;
        let redis = ConnectionManager::new(redis_client)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to connect to Redis: {}", e)))?;
        info!("Redis connection established");

        // Initialize Solana client with optional settlement configuration
        let solana =
            match SolanaClient::new(&config.solana.rpc_url, &config.solana.bounty_program_id) {
                Ok(mut client) => {
                    // Configure settlement (oracle keypair, mint, treasury)
                    if let Some(ref path) = config.solana.oracle_keypair_path {
                        if let Err(e) = client.load_oracle_keypair(path) {
                            warn!("Failed to load oracle keypair: {}", e);
                        }
                    }
                    if let Some(ref addr) = config.solana.mint_address {
                        if let Err(e) = client.set_mint(addr) {
                            warn!("Failed to set mint address: {}", e);
                        }
                    }
                    if let Some(ref addr) = config.solana.treasury_token_account {
                        if let Err(e) = client.set_treasury(addr) {
                            warn!("Failed to set treasury address: {}", e);
                        }
                    }

                    let ready = client.is_settlement_ready();
                    info!(
                        rpc = %config.solana.rpc_url,
                        settlement_ready = ready,
                        "Solana client initialized"
                    );
                    Some(Arc::new(client))
                }
                Err(e) => {
                    warn!("Solana client initialization failed (optional): {}", e);
                    None
                }
            };

        Ok(Self {
            db,
            redis,
            config: Arc::new(config),
            solana,
        })
    }

    /// Run database migrations (idempotent).
    pub async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations...");
        sqlx::migrate!("./migrations")
            .run(&self.db)
            .await
            .map_err(|e| AmosError::Database(e.into()))?;
        info!("Database migrations completed successfully");
        Ok(())
    }

    /// Health check: verify DB and Redis are reachable.
    pub async fn health_check(&self) -> Result<()> {
        // Check PostgreSQL
        sqlx::query("SELECT 1")
            .execute(&self.db)
            .await
            .map_err(AmosError::Database)?;

        // Check Redis
        use redis::AsyncCommands;
        let mut conn = self.redis.clone();
        conn.get::<&str, Option<String>>("__health__")
            .await
            .map_err(|e| AmosError::Internal(format!("Redis health check failed: {}", e)))?;

        Ok(())
    }
}
