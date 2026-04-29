//! Relay sync client: connects the harness to the AMOS Network Relay (marketplace layer).
//!
//! Three background loops run concurrently:
//! - **Heartbeat**: Reports health/version to relay every N seconds
//! - **Bounty sync**: Pulls available bounties from marketplace
//! - **Reputation reporter**: Pushes agent performance and completion data

use amos_core::config::{DeploymentConfig, RelayConfig};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Relay sync client manages communication between harness and relay.
pub struct RelaySyncClient {
    http: Client,
    relay_url: String,
    api_key: Option<String>,
    harness_id: String,
    harness_version: String,
    config: RelayConfig,
    /// Cached bounties (updated by sync loop).
    bounties: Arc<RwLock<Vec<RelayBounty>>>,
    /// Database pool for querying real agent metrics.
    db_pool: Option<sqlx::PgPool>,
}

/// Bounty pulled from the relay marketplace.
///
/// Mirrors the relay's `BountyResponse` — keep this in sync. Fields that the
/// agent needs to plan + claim are surfaced here so `discover_bounties` can
/// return enough signal that the agent doesn't have to fetch each bounty
/// individually to assess fit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayBounty {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub reward_tokens: u64,
    pub deadline: String,
    pub required_capabilities: Vec<String>,
    #[serde(default = "default_category")]
    pub category: String,
    /// Bounty status (open, claimed, submitted, approved, rejected, etc.).
    #[serde(default)]
    pub status: Option<String>,
    /// PR url, set once the worker submits. Useful for the agent to check
    /// whether work is already in flight on a bounty it sees in the cache.
    #[serde(default)]
    pub pr_url: Option<String>,
    /// Wallet that posted the bounty. Lets the agent avoid claiming bounties
    /// where it would be both worker and reviewer.
    #[serde(default)]
    pub poster_wallet: Option<String>,
    /// Number of revisions already used. Agents can avoid bounties that have
    /// burned through their revision budget.
    #[serde(default)]
    pub revision_count: i16,
    /// Optional structured policy block (forbidden_paths, scope constraints,
    /// minimum_coverage_pct, etc.). When present, this is the hard contract
    /// the submission must respect.
    #[serde(default)]
    pub policy: Option<JsonValue>,
}

fn default_category() -> String {
    "infrastructure".to_string()
}

/// Heartbeat payload sent to relay.
#[derive(Debug, Serialize)]
struct HeartbeatPayload {
    harness_id: String,
    harness_version: String,
    status: String,
    capabilities: Vec<String>,
    agent_count: u32,
    timestamp: String,
}

/// Reputation report sent to relay.
#[derive(Debug, Serialize)]
struct ReputationReport {
    harness_id: String,
    agents: Vec<AgentReputation>,
    timestamp: String,
}

/// Agent reputation data.
#[derive(Debug, Serialize)]
struct AgentReputation {
    agent_id: Uuid,
    bounties_completed: u32,
    avg_quality_score: f64,
    uptime_pct: f64,
}

impl RelaySyncClient {
    /// Create a new relay sync client.
    pub fn new(relay_config: &RelayConfig, deployment_config: &DeploymentConfig) -> Self {
        let api_key = relay_config
            .api_key
            .as_ref()
            .map(|s| s.expose_secret().to_string());

        // Generate a stable harness ID from env var or use a UUID
        let harness_id =
            std::env::var("HARNESS_ID").unwrap_or_else(|_| format!("harness-{}", Uuid::new_v4()));

        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
            relay_url: relay_config.url.clone(),
            api_key,
            harness_id,
            harness_version: deployment_config.harness_version.clone(),
            config: relay_config.clone(),
            bounties: Arc::new(RwLock::new(Vec::new())),
            db_pool: None,
        }
    }

    /// Set the database pool for querying real agent metrics.
    pub fn with_db_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.db_pool = Some(pool);
        self
    }

    /// Get a shared reference to the bounty cache (for tools and fleet manager).
    pub fn bounty_cache(&self) -> Arc<RwLock<Vec<RelayBounty>>> {
        self.bounties.clone()
    }

    /// Get the cached available bounties.
    pub async fn available_bounties(&self) -> Vec<RelayBounty> {
        self.bounties.read().await.clone()
    }

    /// Start all background sync loops. Returns a JoinHandle for the spawned task.
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        tokio::spawn(async move {
            if !client.config.enabled {
                info!("Relay integration disabled");
                // Just sleep forever so the task doesn't exit
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            }

            info!(
                "Relay sync started: url={}, heartbeat={}s, bounty_sync={}s, reputation={}s",
                client.relay_url,
                client.config.heartbeat_interval_secs,
                client.config.bounty_sync_interval_secs,
                client.config.reputation_report_interval_secs,
            );

            // Run all three loops concurrently
            tokio::join!(
                client.heartbeat_loop(),
                client.bounty_sync_loop(),
                client.reputation_report_loop(),
            );
        })
    }

    /// Add authorization header if API key is configured.
    fn auth_header(&self) -> Option<(String, String)> {
        self.api_key
            .as_ref()
            .map(|key| ("Authorization".to_string(), format!("Bearer {}", key)))
    }

    // ── Heartbeat Loop ──────────────────────────────────────────────────

    async fn heartbeat_loop(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.heartbeat_interval_secs,
        ));

        loop {
            interval.tick().await;

            // Query real agent count from database (includes fleet agents)
            let agent_count = if let Some(ref pool) = self.db_pool {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM openclaw_agents WHERE status IN ('active', 'working', 'idle')",
                )
                .fetch_one(pool)
                .await
                .unwrap_or(0) as u32
            } else {
                0
            };

            let payload = HeartbeatPayload {
                harness_id: self.harness_id.clone(),
                harness_version: self.harness_version.clone(),
                status: "healthy".to_string(),
                capabilities: vec![
                    "document_processing".to_string(),
                    "image_generation".to_string(),
                    "web_search".to_string(),
                    "code_execution".to_string(),
                ],
                agent_count,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };

            let url = format!("{}/api/v1/harnesses/heartbeat", self.relay_url);
            let mut req = self.http.post(&url).json(&payload);
            if let Some((key, value)) = self.auth_header() {
                req = req.header(&key, &value);
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    debug!("Relay heartbeat sent successfully");
                }
                Ok(resp) => {
                    warn!(
                        "Relay heartbeat returned {}: {}",
                        resp.status(),
                        resp.status()
                    );
                }
                Err(e) => {
                    debug!("Relay heartbeat failed (relay may be unreachable): {}", e);
                }
            }
        }
    }

    // ── Bounty Sync Loop ────────────────────────────────────────────────

    async fn bounty_sync_loop(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.bounty_sync_interval_secs,
        ));

        loop {
            interval.tick().await;
            let url = format!("{}/api/v1/bounties?status=open", self.relay_url);
            let mut req = self.http.get(&url);
            if let Some((key, value)) = self.auth_header() {
                req = req.header(&key, &value);
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<Vec<RelayBounty>>().await {
                        Ok(bounties) => {
                            let count = bounties.len();
                            let mut cached = self.bounties.write().await;
                            *cached = bounties;
                            debug!("Bounty sync completed: {} bounties available", count);
                        }
                        Err(e) => {
                            warn!("Failed to parse bounties: {}", e);
                        }
                    }
                }
                Ok(resp) => {
                    debug!("Bounty sync returned {}", resp.status());
                }
                Err(e) => {
                    debug!("Bounty sync failed (relay may be unreachable): {}", e);
                }
            }
        }
    }

    // ── Reputation Report Loop ──────────────────────────────────────────

    async fn reputation_report_loop(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.reputation_report_interval_secs,
        ));

        loop {
            interval.tick().await;

            // Query agent reputation data from external_agents and fleet bounty_claims
            let mut agents: Vec<AgentReputation> = Vec::new();

            if let Some(ref pool) = self.db_pool {
                // Legacy: external_agents table
                if let Ok(rows) = sqlx::query_as::<_, (uuid::Uuid, i64, f64)>(
                    r#"
                    SELECT ea.id, ea.total_tasks_completed, ea.average_quality_score
                    FROM external_agents ea
                    WHERE ea.status = 'active' AND ea.total_tasks_completed > 0
                    "#,
                )
                .fetch_all(pool)
                .await
                {
                    for (id, completed, quality) in rows {
                        agents.push(AgentReputation {
                            agent_id: id,
                            bounties_completed: completed as u32,
                            avg_quality_score: quality,
                            uptime_pct: 99.0,
                        });
                    }
                }

                // Fleet agents: reputation derived from bounty_claims
                if let Ok(rows) = sqlx::query_as::<_, (i32, i64, i64)>(
                    r#"
                    SELECT
                        bc.agent_id,
                        COUNT(*) FILTER (WHERE bc.status = 'approved') as completed,
                        COUNT(*) as total
                    FROM bounty_claims bc
                    JOIN openclaw_agents oa ON oa.id = bc.agent_id
                    WHERE oa.status IN ('active', 'working', 'idle')
                    GROUP BY bc.agent_id
                    HAVING COUNT(*) > 0
                    "#,
                )
                .fetch_all(pool)
                .await
                {
                    for (agent_id, completed, total) in rows {
                        let quality = if total > 0 {
                            completed as f64 / total as f64 * 100.0
                        } else {
                            0.0
                        };
                        // Use a deterministic UUID based on agent_id for relay compatibility
                        let agent_uuid = uuid::Uuid::from_u128(agent_id as u128);
                        agents.push(AgentReputation {
                            agent_id: agent_uuid,
                            bounties_completed: completed as u32,
                            avg_quality_score: quality,
                            uptime_pct: 99.0,
                        });
                    }
                }
            }

            let report = ReputationReport {
                harness_id: self.harness_id.clone(),
                agents,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };

            // Skip empty reports
            if report.agents.is_empty() {
                debug!("Skipping empty reputation report (no active agents with completions)");
                continue;
            }

            let url = format!("{}/api/v1/reputation/report", self.relay_url);
            let mut req = self.http.post(&url).json(&report);
            if let Some((key, value)) = self.auth_header() {
                req = req.header(&key, &value);
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    debug!("Reputation report sent: {} agents", report.agents.len());
                }
                Ok(resp) => {
                    warn!("Reputation report returned {}", resp.status());
                }
                Err(e) => {
                    debug!("Reputation report failed: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amos_core::config::{DeploymentConfig, RelayConfig};

    #[test]
    fn test_relay_sync_client_creation() {
        let relay_config = RelayConfig::default();
        let deployment_config = DeploymentConfig::default();
        let client = RelaySyncClient::new(&relay_config, &deployment_config);

        assert_eq!(client.relay_url, "http://localhost:4100");
        assert!(client.api_key.is_none());
        assert!(!client.harness_id.is_empty());
        assert!(client.db_pool.is_none());
    }

    #[tokio::test]
    async fn test_relay_bounty_cache_default() {
        let relay_config = RelayConfig::default();
        let deployment_config = DeploymentConfig::default();
        let client = RelaySyncClient::new(&relay_config, &deployment_config);

        let bounties = client.available_bounties().await;
        assert!(bounties.is_empty());
    }

    #[tokio::test]
    async fn test_relay_sync_disabled() {
        let mut relay_config = RelayConfig::default();
        relay_config.enabled = false;
        let deployment_config = DeploymentConfig::default();
        let client = Arc::new(RelaySyncClient::new(&relay_config, &deployment_config));

        // Start should return immediately when disabled
        let handle = client.start();

        // Give it a moment to initialize
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Should still be running (sleeping forever)
        assert!(!handle.is_finished());

        // Clean up
        handle.abort();
    }
}
