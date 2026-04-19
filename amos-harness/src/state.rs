//! Application state shared across all request handlers

use crate::{
    automations::{engine::AutomationEngine, TriggerEvent},
    canvas::CanvasEngine,
    documents::DocumentProcessor,
    embeddings::EmbeddingService,
    geo::GeoLocator,
    image_gen::ImageGenClient,
    integrations::{etl::EtlPipeline, executor::ApiExecutor},
    openclaw::{fleet::FleetManager, AgentManager},
    orchestrator::HarnessOrchestrator,
    ses::SesClient,
    storage::StorageClient,
    task_queue::TaskQueue,
    tools::ToolRegistry,
};
use amos_core::{AppConfig, CredentialVault};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

/// A destructive command awaiting user confirmation before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConfirmation {
    /// The shell command to execute if approved
    pub command: String,
    /// Working directory (optional)
    pub working_dir: Option<String>,
    /// Timeout in seconds
    pub timeout_secs: u64,
    /// Human-readable warning shown to the user
    pub warning: String,
    /// When this confirmation was created (epoch secs)
    pub created_at: u64,
}

/// Thread-safe store for commands awaiting user confirmation.
///
/// Keys are confirmation tokens (UUID strings). Entries auto-expire
/// after `TTL_SECS` — the confirm endpoint checks this before executing.
pub struct PendingConfirmations {
    inner: DashMap<String, PendingConfirmation>,
}

impl PendingConfirmations {
    /// Confirmations expire after 5 minutes.
    const TTL_SECS: u64 = 300;

    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Store a pending confirmation. Returns the token.
    pub fn insert(&self, token: String, entry: PendingConfirmation) {
        self.inner.insert(token, entry);
    }

    /// Remove and return a pending confirmation if it exists and hasn't expired.
    pub fn take(&self, token: &str) -> Option<PendingConfirmation> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        match self.inner.remove(token) {
            Some((_, entry)) if now - entry.created_at <= Self::TTL_SECS => Some(entry),
            Some(_) => None, // expired
            None => None,
        }
    }

    /// Prune expired entries (call periodically if desired).
    pub fn prune_expired(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.inner
            .retain(|_, entry| now - entry.created_at <= Self::TTL_SECS);
    }
}

/// Shared application state
///
/// This struct holds all shared resources that are accessible from route handlers.
/// It is wrapped in an Arc to allow cheap cloning across async tasks.
#[derive(Clone)]
pub struct AppState {
    /// PostgreSQL connection pool
    pub db_pool: PgPool,

    /// Redis client for caching and pub/sub
    pub redis: redis::Client,

    /// Application configuration
    pub config: Arc<AppConfig>,

    /// Canvas rendering and generation engine
    pub canvas_engine: Arc<CanvasEngine>,

    /// Tool registry for agent execution
    pub tool_registry: Arc<ToolRegistry>,

    /// OpenClaw agent manager for autonomous AI agent orchestration
    pub agent_manager: Arc<AgentManager>,

    /// Task queue for background work (internal sub-agents + external bounties)
    pub task_queue: Arc<TaskQueue>,

    /// File storage client (local filesystem or S3)
    pub storage: Arc<StorageClient>,

    /// Document processor for extracting text from uploaded files (PDF, DOCX, etc.)
    pub document_processor: Arc<DocumentProcessor>,

    /// Image generation client (Google Imagen API)
    /// `None` if credentials are not configured.
    pub image_gen: Option<Arc<ImageGenClient>>,

    /// Universal API executor for making authenticated calls to external APIs
    pub api_executor: Arc<ApiExecutor>,

    /// ETL pipeline for syncing external API data into AMOS collections
    pub etl_pipeline: Arc<EtlPipeline>,

    /// Credential vault for AES-256-GCM encrypted secret storage
    pub vault: Arc<CredentialVault>,

    /// IP geolocation service (cached lookups)
    pub geo_locator: Arc<GeoLocator>,

    /// Embedding service for semantic search (OpenAI-compatible API).
    /// `None` if `AMOS__EMBEDDING__API_KEY` is not set.
    pub embedding_service: Option<Arc<EmbeddingService>>,

    /// Automation engine for event-driven triggers and scheduled actions
    pub automation_engine: Arc<AutomationEngine>,

    /// Channel for schema CRUD events → automation engine (breaks async type cycle)
    pub automation_event_tx: tokio::sync::mpsc::Sender<TriggerEvent>,

    /// Multi-harness orchestrator (primary harness only).
    /// Provides discovery cache and proxy for specialist harness management.
    pub orchestrator: Option<Arc<HarnessOrchestrator>>,

    /// Fleet manager for autonomous bounty agents.
    /// `None` if `AMOS__FLEET__ENABLED` is not set to true.
    pub fleet_manager: Option<Arc<FleetManager>>,

    /// Activity counters for platform telemetry (token usage, conversations, etc.)
    pub activity_counters: Arc<crate::platform_sync::ActivityCounters>,

    /// Pending destructive commands awaiting user confirmation before execution.
    pub pending_confirmations: Arc<PendingConfirmations>,

    /// AWS SES email client. `None` if `AMOS__EMAIL__FROM_ADDRESS` is not configured.
    pub email_client: Option<Arc<SesClient>>,

    /// Platform sync client — polls the platform for new releases so the
    /// frontend update banner can tell the user "new version available" without
    /// a manual platform-dashboard visit. `None` for self-hosted deploys with
    /// no platform configured.
    pub platform_sync: Option<Arc<crate::platform_sync::PlatformSyncClient>>,
}

impl AppState {
    /// Get a Redis connection from the pool
    pub fn get_redis_connection(&self) -> Result<redis::Connection, redis::RedisError> {
        self.redis.get_connection()
    }

    /// Get an async Redis connection
    #[allow(deprecated)]
    pub async fn get_redis_async_connection(
        &self,
    ) -> Result<redis::aio::Connection, redis::RedisError> {
        self.redis.get_async_connection().await
    }
}
