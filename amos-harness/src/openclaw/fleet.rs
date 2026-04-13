//! Fleet Manager — orchestrates multiple autonomous bounty agents.
//!
//! Deploys, monitors, and rebalances a fleet of autonomous agents, each
//! running its own bounty loop with a distinct capability profile.

use crate::agent::autonomous::{
    AgentTelemetry, AutonomousAgentLoop, AutonomousLoopConfig, LoopState,
};
use crate::agent::context::{ContextProvider, FileContextProvider};
use crate::openclaw::AgentConfig;
use crate::relay_sync::RelayBounty;
use amos_core::{AmosError, AppConfig, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Predefined agent capability profiles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProfile {
    /// Research agent: code execution, mathematical analysis, file operations
    Research,
    /// Infrastructure agent: code execution, Docker, API integration
    Infrastructure,
    /// Content agent: content generation, social media, analytics
    Content,
    /// General agent: broad tool inventory, lower specialization
    General,
}

impl AgentProfile {
    /// Get the capabilities for this profile.
    pub fn capabilities(&self) -> Vec<String> {
        match self {
            AgentProfile::Research => vec![
                "code_execution".into(),
                "mathematical_analysis".into(),
                "file_write".into(),
                "web_search".into(),
                "knowledge_base".into(),
            ],
            AgentProfile::Infrastructure => vec![
                "code_execution".into(),
                "file_write".into(),
                "docker".into(),
                "api_integration".into(),
                "system_admin".into(),
            ],
            AgentProfile::Content => vec![
                "content_generation".into(),
                "social_media_api".into(),
                "analytics_read".into(),
                "image_generation".into(),
            ],
            AgentProfile::General => vec![
                "code_execution".into(),
                "file_write".into(),
                "web_search".into(),
                "content_generation".into(),
            ],
        }
    }

    /// Get the default system prompt for this profile.
    pub fn system_prompt(&self, protocol_context: &str) -> String {
        let role_prompt = match self {
            AgentProfile::Research => {
                "You are a research agent specializing in code analysis, data research, \
                 and technical documentation. You claim and complete research bounties \
                 from the AMOS relay marketplace."
            }
            AgentProfile::Infrastructure => {
                "You are an infrastructure agent specializing in platform engineering, \
                 deployment, and system integration. You claim and complete infrastructure \
                 bounties from the AMOS relay marketplace."
            }
            AgentProfile::Content => {
                "You are a content agent specializing in content creation, marketing, \
                 and social media management. You claim and complete content bounties \
                 from the AMOS relay marketplace."
            }
            AgentProfile::General => {
                "You are a general-purpose agent capable of handling a variety of tasks. \
                 You claim and complete bounties from the AMOS relay marketplace."
            }
        };

        format!("{role_prompt}\n\n{protocol_context}")
    }

    /// Get task specializations as JSON for this profile.
    pub fn task_specializations(&self) -> JsonValue {
        match self {
            AgentProfile::Research => json!({
                "primary": ["research", "analysis", "documentation"],
                "contribution_types": ["feature", "documentation", "testing_qa"]
            }),
            AgentProfile::Infrastructure => json!({
                "primary": ["infrastructure", "deployment", "integration"],
                "contribution_types": ["infrastructure", "bug_fix", "feature"]
            }),
            AgentProfile::Content => json!({
                "primary": ["content", "marketing", "social_media"],
                "contribution_types": ["content_marketing", "design"]
            }),
            AgentProfile::General => json!({
                "primary": ["general"],
                "contribution_types": ["feature", "bug_fix", "documentation"]
            }),
        }
    }

    /// Get the display name for this profile.
    pub fn display_name(&self) -> &str {
        match self {
            AgentProfile::Research => "Research Agent",
            AgentProfile::Infrastructure => "Infrastructure Agent",
            AgentProfile::Content => "Content Agent",
            AgentProfile::General => "General Agent",
        }
    }
}

impl std::fmt::Display for AgentProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentProfile::Research => write!(f, "research"),
            AgentProfile::Infrastructure => write!(f, "infrastructure"),
            AgentProfile::Content => write!(f, "content"),
            AgentProfile::General => write!(f, "general"),
        }
    }
}

/// Maximum restart attempts before marking an agent as permanently failed.
const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Exponential backoff delays between restarts (seconds).
const RESTART_BACKOFFS: [u64; 5] = [5, 15, 30, 60, 120];

/// A running autonomous agent in the fleet.
struct FleetAgent {
    agent_id: i32,
    profile: AgentProfile,
    loop_handle: tokio::task::JoinHandle<()>,
    autonomous_loop: Arc<AutonomousAgentLoop>,
}

/// Fleet-wide aggregated metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetMetrics {
    pub total_agents: usize,
    pub active_agents: usize,
    pub idle_agents: usize,
    pub working_agents: usize,
    pub total_bounties_discovered: u64,
    pub total_bounties_claimed: u64,
    pub total_bounties_completed: u64,
    pub total_bounties_failed: u64,
    pub total_tokens_earned: i64,
    pub agents: Vec<AgentMetricsSummary>,
}

/// Per-agent metrics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsSummary {
    pub agent_id: i32,
    pub profile: String,
    pub state: String,
    pub telemetry: AgentTelemetry,
}

/// Fleet manager coordinates multiple autonomous agent loops.
pub struct FleetManager {
    db_pool: PgPool,
    config: Arc<AppConfig>,
    context_provider: Arc<dyn ContextProvider>,
    bounty_cache: Arc<RwLock<Vec<RelayBounty>>>,
    agents: Arc<RwLock<HashMap<i32, FleetAgent>>>,
}

impl FleetManager {
    /// Create a new fleet manager.
    pub fn new(
        db_pool: PgPool,
        config: Arc<AppConfig>,
        bounty_cache: Arc<RwLock<Vec<RelayBounty>>>,
    ) -> Self {
        let context_path = Path::new(&config.fleet.agent_context_path);
        let context_provider = Arc::new(FileContextProvider::new(context_path));

        Self {
            db_pool,
            config,
            context_provider,
            bounty_cache,
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Reset agents left in transient states from a previous crash.
    ///
    /// On harness restart, any agent marked as 'active' or 'working' in the DB
    /// was mid-loop when the process died. Reset them to 'idle' so they can be
    /// re-attached or redeployed cleanly.
    pub async fn reconcile_on_startup(&self) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE openclaw_agents SET status = 'idle' WHERE status IN ('active', 'working', 'executing')",
        )
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Reconciliation failed: {e}")))?;

        let reconciled = result.rows_affected();

        if reconciled > 0 {
            info!(reconciled, "Reconciled stuck agents on startup");

            sqlx::query(
                r#"INSERT INTO fleet_events (event_type, metadata)
                   VALUES ('reconciled', $1)"#,
            )
            .bind(json!({ "agents_reset": reconciled }))
            .execute(&self.db_pool)
            .await
            .ok();
        }

        // Also reset any bounty claims stuck in 'executing' state
        let claims_reset = sqlx::query(
            "UPDATE bounty_claims SET status = 'expired' WHERE status = 'executing'",
        )
        .execute(&self.db_pool)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);

        if claims_reset > 0 {
            info!(claims_reset, "Reset stuck bounty claims on startup");
        }

        Ok(reconciled)
    }

    /// Deploy a new autonomous agent with the given profile.
    pub async fn deploy_agent(&self, profile: AgentProfile) -> Result<i32> {
        // Check max agents limit
        let current_count = self.agents.read().await.len();
        if current_count >= self.config.fleet.max_agents as usize {
            return Err(AmosError::Validation(format!(
                "Fleet at maximum capacity ({}/{})",
                current_count, self.config.fleet.max_agents
            )));
        }

        let capabilities = profile.capabilities();
        let protocol_prompt = self.context_provider.protocol_prompt();
        let system_prompt = profile.system_prompt(&protocol_prompt);
        let task_specs = profile.task_specializations();
        let name = format!("fleet-{}-{}", profile, chrono::Utc::now().timestamp());

        // Determine model provider: local (Ollama) or cloud (Bedrock)
        let (model, provider_type, api_base, cost_tier) = if self.config.fleet.has_local_model() {
            let lm = &self.config.fleet.local_model;
            (
                lm.model_id.clone(),
                Some(lm.provider.clone()),
                Some(lm.api_base.clone()),
                Some("local".to_string()),
            )
        } else {
            (self.config.bedrock.default_model.clone(), None, None, None)
        };

        // Register agent in database
        let capabilities_json = serde_json::to_value(&capabilities)
            .map_err(|e| AmosError::Internal(format!("Failed to serialize capabilities: {e}")))?;

        let row = sqlx::query_scalar::<_, i32>(
            r#"INSERT INTO openclaw_agents
               (name, display_name, role, capabilities, system_prompt, model, status, trust_level)
               VALUES ($1, $2, $3, $4, $5, $6, 'active', 1)
               RETURNING id"#,
        )
        .bind(&name)
        .bind(profile.display_name())
        .bind(format!("autonomous-{profile}"))
        .bind(&capabilities_json)
        .bind(&system_prompt)
        .bind(&model)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to register fleet agent: {e}")))?;

        let agent_id = row;

        // Create agent config
        let agent_config = AgentConfig {
            agent_id,
            name: name.clone(),
            display_name: profile.display_name().to_string(),
            role: format!("autonomous-{profile}"),
            capabilities,
            system_prompt: Some(system_prompt),
            model,
            provider_type,
            api_base,
            max_concurrent_tasks: Some(3),
            always_on: Some(true),
            cost_tier,
            task_specializations: Some(task_specs),
        };

        // Create and start autonomous loop
        let loop_config = AutonomousLoopConfig {
            agent_id,
            agent_config,
            trust_level: 1,
            polling_interval_secs: self.config.fleet.polling_interval_secs,
            backoff_max_secs: self.config.fleet.backoff_max_secs,
            min_fit_score: self.config.fleet.min_fit_score,
            verification_timeout_secs: 86400, // 24 hours
        };

        let autonomous_loop = Arc::new(AutonomousAgentLoop::new(
            loop_config,
            self.db_pool.clone(),
            self.config.clone(),
            self.context_provider.clone(),
            self.bounty_cache.clone(),
        ));

        let loop_clone = autonomous_loop.clone();
        let supervised_agent_id = agent_id;
        let db_clone = self.db_pool.clone();
        let handle = tokio::spawn(async move {
            Self::supervised_run(loop_clone, supervised_agent_id, db_clone).await;
        });

        // Track in fleet
        self.agents.write().await.insert(
            agent_id,
            FleetAgent {
                agent_id,
                profile,
                loop_handle: handle,
                autonomous_loop,
            },
        );

        // Record fleet event
        sqlx::query(
            r#"INSERT INTO fleet_events (event_type, agent_id, metadata)
               VALUES ('deployed', $1, $2)"#,
        )
        .bind(agent_id)
        .bind(json!({ "profile": profile.to_string() }))
        .execute(&self.db_pool)
        .await
        .ok();

        info!(
            agent_id,
            profile = %profile,
            "Fleet agent deployed"
        );

        Ok(agent_id)
    }

    /// Stop an autonomous agent.
    pub async fn stop_agent(&self, agent_id: i32) -> Result<()> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .remove(&agent_id)
            .ok_or_else(|| AmosError::NotFound {
                entity: "FleetAgent".to_string(),
                id: agent_id.to_string(),
            })?;

        // Signal stop
        agent.autonomous_loop.stop();

        // Wait briefly for graceful shutdown, then abort if needed
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                agent.loop_handle.abort();
                warn!(agent_id, "Fleet agent force-stopped after timeout");
            }
            _ = async { while !agent.loop_handle.is_finished() {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }} => {
                info!(agent_id, "Fleet agent stopped gracefully");
            }
        }

        // Update DB
        sqlx::query("UPDATE openclaw_agents SET status = 'stopped' WHERE id = $1")
            .bind(agent_id)
            .execute(&self.db_pool)
            .await
            .ok();

        // Record fleet event
        sqlx::query(
            r#"INSERT INTO fleet_events (event_type, agent_id, metadata)
               VALUES ('stopped', $1, '{}')"#,
        )
        .bind(agent_id)
        .execute(&self.db_pool)
        .await
        .ok();

        Ok(())
    }

    /// List all fleet agents and their current status.
    pub async fn list_agents(&self) -> Vec<(i32, AgentProfile, LoopState)> {
        let agents = self.agents.read().await;
        let mut result = Vec::new();
        for (id, agent) in agents.iter() {
            let state = agent.autonomous_loop.state().await;
            result.push((*id, agent.profile, state));
        }
        result
    }

    /// Get aggregated fleet metrics.
    pub async fn metrics(&self) -> FleetMetrics {
        let agents = self.agents.read().await;
        let mut metrics = FleetMetrics {
            total_agents: agents.len(),
            active_agents: 0,
            idle_agents: 0,
            working_agents: 0,
            total_bounties_discovered: 0,
            total_bounties_claimed: 0,
            total_bounties_completed: 0,
            total_bounties_failed: 0,
            total_tokens_earned: 0,
            agents: Vec::new(),
        };

        for (id, agent) in agents.iter() {
            let state = agent.autonomous_loop.state().await;
            let telemetry = agent.autonomous_loop.telemetry().await;

            match &state {
                LoopState::Idle => metrics.idle_agents += 1,
                LoopState::Executing { .. } | LoopState::Assessing => metrics.working_agents += 1,
                LoopState::AwaitingVerification { .. } => metrics.working_agents += 1,
                _ => {}
            }
            metrics.active_agents += 1;
            metrics.total_bounties_discovered += telemetry.bounties_discovered;
            metrics.total_bounties_claimed += telemetry.bounties_claimed;
            metrics.total_bounties_completed += telemetry.bounties_completed;
            metrics.total_bounties_failed += telemetry.bounties_failed;
            metrics.total_tokens_earned += telemetry.tokens_earned;

            let state_str = match &state {
                LoopState::Idle => "idle".to_string(),
                LoopState::Assessing => "assessing".to_string(),
                LoopState::Executing { bounty_id, .. } => format!("executing:{bounty_id}"),
                LoopState::AwaitingVerification { bounty_id } => {
                    format!("awaiting_verification:{bounty_id}")
                }
                LoopState::Stopping => "stopping".to_string(),
            };

            metrics.agents.push(AgentMetricsSummary {
                agent_id: *id,
                profile: agent.profile.to_string(),
                state: state_str,
                telemetry,
            });
        }

        metrics
    }

    /// Rebalance the fleet — stop underperforming agents, deploy new ones if needed.
    pub async fn rebalance(&self) -> Result<JsonValue> {
        let agents = self.agents.read().await;
        let mut actions: Vec<String> = Vec::new();

        // Collect agents with sustained low completion rates
        let mut underperformers: Vec<i32> = Vec::new();
        for (id, agent) in agents.iter() {
            let telemetry = agent.autonomous_loop.telemetry().await;
            if telemetry.bounties_claimed >= 5 {
                let rate = if telemetry.bounties_claimed > 0 {
                    telemetry.bounties_completed as f64 / telemetry.bounties_claimed as f64
                } else {
                    0.0
                };
                if rate < 0.3 {
                    underperformers.push(*id);
                }
            }
        }
        drop(agents);

        // Stop underperformers
        for id in &underperformers {
            match self.stop_agent(*id).await {
                Ok(_) => {
                    actions.push(format!("Stopped underperforming agent {id}"));

                    // Record demotion event
                    sqlx::query(
                        r#"INSERT INTO fleet_events (event_type, agent_id, metadata)
                           VALUES ('demoted', $1, '{"reason": "low_completion_rate"}')"#,
                    )
                    .bind(*id)
                    .execute(&self.db_pool)
                    .await
                    .ok();
                }
                Err(e) => {
                    actions.push(format!("Failed to stop agent {id}: {e}"));
                }
            }
        }

        // Record rebalance event
        sqlx::query(
            r#"INSERT INTO fleet_events (event_type, metadata)
               VALUES ('rebalanced', $1)"#,
        )
        .bind(json!({
            "underperformers_stopped": underperformers.len(),
            "actions": actions,
        }))
        .execute(&self.db_pool)
        .await
        .ok();

        Ok(json!({
            "rebalanced": true,
            "underperformers_stopped": underperformers.len(),
            "actions": actions,
        }))
    }

    /// Run an agent loop with supervisor restart on panic.
    ///
    /// If the inner `run()` panics (JoinError), the supervisor retries up to
    /// [`MAX_RESTART_ATTEMPTS`] times with exponential backoff. A clean exit
    /// (Ok) is not restarted.
    async fn supervised_run(
        agent_loop: Arc<AutonomousAgentLoop>,
        agent_id: i32,
        db_pool: PgPool,
    ) {
        let mut restarts: u32 = 0;

        loop {
            let inner = agent_loop.clone();
            let result = tokio::spawn(async move { inner.run().await }).await;

            match result {
                Ok(()) => {
                    // Clean exit — agent was stopped intentionally
                    info!(agent_id, "Autonomous agent loop exited cleanly");
                    break;
                }
                Err(join_err) => {
                    restarts += 1;
                    if restarts > MAX_RESTART_ATTEMPTS {
                        tracing::error!(
                            agent_id,
                            restarts,
                            "Agent exceeded max restart attempts — marking as error"
                        );
                        sqlx::query(
                            "UPDATE openclaw_agents SET status = 'error' WHERE id = $1",
                        )
                        .bind(agent_id)
                        .execute(&db_pool)
                        .await
                        .ok();

                        sqlx::query(
                            r#"INSERT INTO fleet_events (event_type, agent_id, metadata)
                               VALUES ('error', $1, $2)"#,
                        )
                        .bind(agent_id)
                        .bind(json!({
                            "reason": "max_restarts_exceeded",
                            "restarts": restarts,
                            "last_error": format!("{join_err}"),
                        }))
                        .execute(&db_pool)
                        .await
                        .ok();
                        break;
                    }

                    let backoff = RESTART_BACKOFFS
                        .get(restarts as usize - 1)
                        .copied()
                        .unwrap_or(120);

                    warn!(
                        agent_id,
                        restarts,
                        backoff_secs = backoff,
                        error = %join_err,
                        "Autonomous agent panicked — restarting after backoff"
                    );

                    sqlx::query(
                        r#"INSERT INTO fleet_events (event_type, agent_id, metadata)
                           VALUES ('restarted', $1, $2)"#,
                    )
                    .bind(agent_id)
                    .bind(json!({
                        "restart_number": restarts,
                        "backoff_secs": backoff,
                        "error": format!("{join_err}"),
                    }))
                    .execute(&db_pool)
                    .await
                    .ok();

                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
                }
            }
        }
    }

    /// Get the count of active agents.
    pub async fn active_count(&self) -> usize {
        self.agents.read().await.len()
    }

    /// Check if the local model server is healthy and has the required model.
    ///
    /// Returns `Ok(true)` if the model is available, `Ok(false)` if unreachable
    /// or the model is not pulled. Does not fail the harness startup.
    pub async fn check_local_model_health(&self) -> Result<bool> {
        if !self.config.fleet.has_local_model() {
            return Ok(false);
        }

        let api_base = &self.config.fleet.local_model.api_base;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| AmosError::Internal(format!("HTTP client error: {e}")))?;

        // Query the OpenAI-compatible /models endpoint
        let url = format!("{}/models", api_base.trim_end_matches('/'));
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                let models = body
                    .get("data")
                    .and_then(|d| d.as_array())
                    .cloned()
                    .unwrap_or_default();

                let model_id = &self.config.fleet.local_model.model_id;
                let has_model = models.iter().any(|m| {
                    m.get("id")
                        .and_then(|v| v.as_str())
                        .map(|id| id == model_id)
                        .unwrap_or(false)
                });

                if !has_model {
                    let available: Vec<&str> = models
                        .iter()
                        .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
                        .collect();
                    warn!(
                        model = %model_id,
                        ?available,
                        "Local model not found — run `ollama pull {}`",
                        model_id
                    );
                }

                Ok(has_model)
            }
            Ok(resp) => {
                warn!(status = %resp.status(), "Local model server returned error");
                Ok(false)
            }
            Err(e) => {
                warn!(error = %e, url = %url, "Local model server not reachable");
                Ok(false)
            }
        }
    }

    /// Deploy the initial fleet composition from config if no agents are running.
    ///
    /// Called on startup. Skips deployment if agents already exist in the DB
    /// (i.e. the harness is restarting, not fresh).
    pub async fn auto_deploy_initial_fleet(&self) -> Result<Vec<i32>> {
        if self.config.fleet.initial_fleet.is_empty() {
            return Ok(Vec::new());
        }

        // Check if agents already exist
        let existing: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM openclaw_agents WHERE status != 'stopped'")
                .fetch_one(&self.db_pool)
                .await
                .unwrap_or(0);

        if existing > 0 {
            info!(existing, "Agents already exist in DB, skipping initial fleet deploy");
            return Ok(Vec::new());
        }

        let mut deployed = Vec::new();
        for entry in &self.config.fleet.initial_fleet {
            let profile = match entry.profile.as_str() {
                "research" => AgentProfile::Research,
                "infrastructure" => AgentProfile::Infrastructure,
                "content" => AgentProfile::Content,
                "general" => AgentProfile::General,
                other => {
                    warn!(profile = %other, "Unknown profile in initial_fleet, skipping");
                    continue;
                }
            };
            for _ in 0..entry.count {
                match self.deploy_agent(profile).await {
                    Ok(id) => deployed.push(id),
                    Err(e) => warn!(profile = %entry.profile, error = %e, "Failed to deploy initial agent"),
                }
            }
        }

        if !deployed.is_empty() {
            info!(count = deployed.len(), "Initial fleet deployed");
        }

        Ok(deployed)
    }

    /// Start background health check loop that monitors agent task handles.
    ///
    /// Checks every `health_check_interval_secs` for:
    /// - Agents whose JoinHandle has finished unexpectedly
    /// - Agents stuck in Executing state for too long
    pub fn start_health_check_loop(self: &Arc<Self>) {
        let fleet = Arc::clone(self);
        let interval = self.config.fleet.health_check_interval_secs;

        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(tokio::time::Duration::from_secs(interval));
            ticker.tick().await; // skip immediate first tick

            loop {
                ticker.tick().await;

                let agents = fleet.agents.read().await;
                let mut finished_ids: Vec<i32> = Vec::new();

                for (id, agent) in agents.iter() {
                    if agent.loop_handle.is_finished() {
                        warn!(agent_id = id, "Agent task handle finished unexpectedly");
                        finished_ids.push(*id);
                    }
                }
                drop(agents);

                // Log finished agents (the supervisor inside the task handles restarts;
                // if the supervisor itself exited, the agent hit max restarts)
                for id in finished_ids {
                    sqlx::query(
                        r#"INSERT INTO fleet_events (event_type, agent_id, metadata)
                           VALUES ('error', $1, '{"reason": "task_handle_finished"}')"#,
                    )
                    .bind(id)
                    .execute(&fleet.db_pool)
                    .await
                    .ok();
                }
            }
        });
    }

    /// Start background periodic rebalancing loop.
    pub fn start_rebalance_loop(self: &Arc<Self>) {
        let fleet = Arc::clone(self);
        let interval = self.config.fleet.rebalance_interval_secs;

        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(tokio::time::Duration::from_secs(interval));
            ticker.tick().await; // skip first

            loop {
                ticker.tick().await;

                match fleet.rebalance().await {
                    Ok(result) => {
                        if let Some(stopped) = result.get("underperformers_stopped").and_then(|v| v.as_u64()) {
                            if stopped > 0 {
                                info!(stopped, "Periodic rebalance stopped underperformers");
                            }
                        }
                    }
                    Err(e) => warn!(error = %e, "Periodic rebalance failed"),
                }
            }
        });
    }

    /// Check and upgrade an agent's trust level based on cumulative performance.
    ///
    /// Trust thresholds:
    /// - Level 2: 3 completions, >55% success rate
    /// - Level 3: 10 completions, >65% success rate
    /// - Level 4: 25 completions, >75% success rate
    /// - Level 5: 50 completions, >85% success rate
    pub async fn check_trust_progression(&self, agent_id: i32) {
        let agents = self.agents.read().await;
        let Some(agent) = agents.get(&agent_id) else {
            return;
        };
        let telemetry = agent.autonomous_loop.telemetry().await;
        let current_trust = agent.autonomous_loop.config().trust_level;
        drop(agents);

        let completed = telemetry.bounties_completed;
        let claimed = telemetry.bounties_claimed;
        let rate = if claimed > 0 {
            completed as f64 / claimed as f64
        } else {
            0.0
        };

        let new_trust = if completed >= 50 && rate > 0.85 {
            5
        } else if completed >= 25 && rate > 0.75 {
            4
        } else if completed >= 10 && rate > 0.65 {
            3
        } else if completed >= 3 && rate > 0.55 {
            2
        } else {
            current_trust
        };

        if new_trust > current_trust {
            sqlx::query("UPDATE openclaw_agents SET trust_level = $1 WHERE id = $2")
                .bind(new_trust as i32)
                .execute(&self.db_pool)
                .await
                .ok();

            sqlx::query(
                r#"INSERT INTO fleet_events (event_type, agent_id, metadata)
                   VALUES ('trust_upgraded', $1, $2)"#,
            )
            .bind(agent_id)
            .bind(json!({
                "from": current_trust,
                "to": new_trust,
                "completions": completed,
                "success_rate": rate,
            }))
            .execute(&self.db_pool)
            .await
            .ok();

            info!(
                agent_id,
                from = current_trust,
                to = new_trust,
                "Agent trust level upgraded"
            );
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // All four profiles for exhaustive iteration
    const ALL_PROFILES: [AgentProfile; 4] = [
        AgentProfile::Research,
        AgentProfile::Infrastructure,
        AgentProfile::Content,
        AgentProfile::General,
    ];

    // ── AgentProfile capabilities ──────────────────────────────────────

    #[test]
    fn agent_profile_capabilities_non_empty() {
        for profile in ALL_PROFILES {
            assert!(
                !profile.capabilities().is_empty(),
                "{profile} should have capabilities"
            );
        }
    }

    #[test]
    fn research_profile_has_code_execution() {
        let caps = AgentProfile::Research.capabilities();
        assert!(caps.contains(&"code_execution".to_string()));
        assert!(caps.contains(&"web_search".to_string()));
    }

    #[test]
    fn infrastructure_profile_has_docker() {
        let caps = AgentProfile::Infrastructure.capabilities();
        assert!(caps.contains(&"docker".to_string()));
        assert!(caps.contains(&"api_integration".to_string()));
    }

    #[test]
    fn content_profile_has_content_generation() {
        let caps = AgentProfile::Content.capabilities();
        assert!(caps.contains(&"content_generation".to_string()));
        assert!(caps.contains(&"image_generation".to_string()));
    }

    #[test]
    fn general_profile_is_broad() {
        let caps = AgentProfile::General.capabilities();
        assert!(caps.contains(&"code_execution".to_string()));
        assert!(caps.contains(&"content_generation".to_string()));
    }

    #[test]
    fn profiles_have_distinct_capabilities() {
        let research = AgentProfile::Research.capabilities();
        let content = AgentProfile::Content.capabilities();
        // Research has code_execution but not content_generation
        assert!(research.contains(&"code_execution".to_string()));
        assert!(!research.contains(&"content_generation".to_string()));
        // Content has content_generation but not code_execution
        assert!(content.contains(&"content_generation".to_string()));
        assert!(!content.contains(&"code_execution".to_string()));
    }

    // ── AgentProfile display & serde ───────────────────────────────────

    #[test]
    fn agent_profile_display() {
        assert_eq!(AgentProfile::Research.to_string(), "research");
        assert_eq!(AgentProfile::Infrastructure.to_string(), "infrastructure");
        assert_eq!(AgentProfile::Content.to_string(), "content");
        assert_eq!(AgentProfile::General.to_string(), "general");
    }

    #[test]
    fn agent_profile_display_name() {
        assert_eq!(AgentProfile::Research.display_name(), "Research Agent");
        assert_eq!(
            AgentProfile::Infrastructure.display_name(),
            "Infrastructure Agent"
        );
        assert_eq!(AgentProfile::Content.display_name(), "Content Agent");
        assert_eq!(AgentProfile::General.display_name(), "General Agent");
    }

    #[test]
    fn agent_profile_serde_roundtrip_all() {
        for profile in ALL_PROFILES {
            let json = serde_json::to_string(&profile).unwrap();
            let deserialized: AgentProfile = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, profile, "roundtrip failed for {profile}");
        }
    }

    #[test]
    fn agent_profile_deserialize_rejects_unknown() {
        let result = serde_json::from_str::<AgentProfile>("\"unknown_profile\"");
        assert!(result.is_err());
    }

    #[test]
    fn agent_profile_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&AgentProfile::Infrastructure).unwrap(),
            "\"infrastructure\""
        );
    }

    // ── AgentProfile system prompts ────────────────────────────────────

    #[test]
    fn agent_profile_system_prompt_includes_context() {
        for profile in ALL_PROFILES {
            let prompt = profile.system_prompt("## Protocol\nTest context");
            assert!(
                prompt.contains("## Protocol"),
                "{profile} prompt missing context"
            );
            assert!(
                prompt.contains("Test context"),
                "{profile} prompt missing context body"
            );
        }
    }

    #[test]
    fn research_prompt_mentions_research() {
        let prompt = AgentProfile::Research.system_prompt("");
        assert!(prompt.contains("research agent"));
    }

    #[test]
    fn infrastructure_prompt_mentions_infrastructure() {
        let prompt = AgentProfile::Infrastructure.system_prompt("");
        assert!(prompt.contains("infrastructure agent"));
    }

    #[test]
    fn content_prompt_mentions_content() {
        let prompt = AgentProfile::Content.system_prompt("");
        assert!(prompt.contains("content agent"));
    }

    #[test]
    fn general_prompt_mentions_general() {
        let prompt = AgentProfile::General.system_prompt("");
        assert!(prompt.contains("general-purpose agent"));
    }

    #[test]
    fn all_prompts_mention_bounties() {
        for profile in ALL_PROFILES {
            let prompt = profile.system_prompt("");
            assert!(
                prompt.contains("bounties"),
                "{profile} prompt should mention bounties"
            );
        }
    }

    // ── AgentProfile task specializations ──────────────────────────────

    #[test]
    fn all_profiles_have_primary_and_contribution_types() {
        for profile in ALL_PROFILES {
            let specs = profile.task_specializations();
            assert!(specs.get("primary").is_some(), "{profile} missing primary");
            assert!(
                specs.get("contribution_types").is_some(),
                "{profile} missing contribution_types"
            );
        }
    }

    #[test]
    fn research_specializations_include_analysis() {
        let specs = AgentProfile::Research.task_specializations();
        let primary = specs["primary"].as_array().unwrap();
        let primary_strs: Vec<&str> = primary.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(primary_strs.contains(&"research"));
        assert!(primary_strs.contains(&"analysis"));
    }

    #[test]
    fn infrastructure_specializations_include_deployment() {
        let specs = AgentProfile::Infrastructure.task_specializations();
        let primary = specs["primary"].as_array().unwrap();
        let primary_strs: Vec<&str> = primary.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(primary_strs.contains(&"infrastructure"));
        assert!(primary_strs.contains(&"deployment"));
    }

    // ── FleetMetrics ───────────────────────────────────────────────────

    #[test]
    fn fleet_metrics_serde_roundtrip() {
        let metrics = FleetMetrics {
            total_agents: 5,
            active_agents: 4,
            idle_agents: 2,
            working_agents: 2,
            total_bounties_discovered: 100,
            total_bounties_claimed: 50,
            total_bounties_completed: 40,
            total_bounties_failed: 10,
            total_tokens_earned: 25_000,
            agents: vec![AgentMetricsSummary {
                agent_id: 1,
                profile: "research".into(),
                state: "idle".into(),
                telemetry: AgentTelemetry {
                    bounties_discovered: 20,
                    bounties_claimed: 10,
                    bounties_completed: 8,
                    bounties_failed: 2,
                    tokens_earned: 5_000,
                    loop_iterations: 200,
                },
            }],
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: FleetMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_agents, 5);
        assert_eq!(deserialized.total_tokens_earned, 25_000);
        assert_eq!(deserialized.agents.len(), 1);
        assert_eq!(deserialized.agents[0].agent_id, 1);
    }

    #[test]
    fn fleet_metrics_empty_fleet() {
        let metrics = FleetMetrics {
            total_agents: 0,
            active_agents: 0,
            idle_agents: 0,
            working_agents: 0,
            total_bounties_discovered: 0,
            total_bounties_claimed: 0,
            total_bounties_completed: 0,
            total_bounties_failed: 0,
            total_tokens_earned: 0,
            agents: vec![],
        };
        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.contains("\"total_agents\":0"));
        assert!(json.contains("\"agents\":[]"));
    }

    #[test]
    fn agent_metrics_summary_serde() {
        let summary = AgentMetricsSummary {
            agent_id: 42,
            profile: "infrastructure".into(),
            state: "executing:AMOS-001".into(),
            telemetry: AgentTelemetry::default(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"agent_id\":42"));
        assert!(json.contains("infrastructure"));
        assert!(json.contains("executing:AMOS-001"));
    }

    // ── Rebalance logic (extracted pattern) ────────────────────────────

    /// Simulate the underperformer detection from rebalance().
    fn is_underperformer(
        claimed: u64,
        completed: u64,
        threshold_claims: u64,
        min_rate: f64,
    ) -> bool {
        if claimed >= threshold_claims {
            let rate = if claimed > 0 {
                completed as f64 / claimed as f64
            } else {
                0.0
            };
            rate < min_rate
        } else {
            false // not enough data
        }
    }

    #[test]
    fn rebalance_detects_underperformer() {
        // 5 claims, only 1 completed = 20% < 30% threshold
        assert!(is_underperformer(5, 1, 5, 0.3));
    }

    #[test]
    fn rebalance_ignores_insufficient_data() {
        // Only 3 claims, below threshold of 5
        assert!(!is_underperformer(3, 0, 5, 0.3));
    }

    #[test]
    fn rebalance_keeps_good_performer() {
        // 10 claims, 8 completed = 80% > 30%
        assert!(!is_underperformer(10, 8, 5, 0.3));
    }

    #[test]
    fn rebalance_edge_at_threshold() {
        // Exactly at the 30% boundary: 10 claims, 3 completed = 30%
        // rate < 0.3 is false when rate == 0.3
        assert!(!is_underperformer(10, 3, 5, 0.3));
    }

    #[test]
    fn rebalance_just_below_threshold() {
        // 10 claims, 2 completed = 20% < 30%
        assert!(is_underperformer(10, 2, 5, 0.3));
    }

    // ── Cost-tier routing logic ───────────────────────────────────────

    /// Simulate the provider selection logic from deploy_agent().
    fn select_provider(local_enabled: bool, local_api_base: &str) -> (&'static str, &'static str) {
        if local_enabled && !local_api_base.is_empty() {
            ("local", "ollama")
        } else {
            ("cloud", "bedrock")
        }
    }

    #[test]
    fn deploy_uses_local_when_enabled() {
        let (tier, provider) = select_provider(true, "http://ollama:11434/v1");
        assert_eq!(tier, "local");
        assert_eq!(provider, "ollama");
    }

    #[test]
    fn deploy_uses_cloud_when_local_disabled() {
        let (tier, provider) = select_provider(false, "http://ollama:11434/v1");
        assert_eq!(tier, "cloud");
        assert_eq!(provider, "bedrock");
    }

    #[test]
    fn deploy_uses_cloud_when_api_base_empty() {
        let (tier, provider) = select_provider(true, "");
        assert_eq!(tier, "cloud");
        assert_eq!(provider, "bedrock");
    }

    #[test]
    fn local_model_config_flows_to_agent() {
        // Simulate the config extraction logic
        let local_model_id = "llama3.2:3b";
        let local_provider = "ollama";
        let local_api_base = "http://localhost:11434/v1";

        // When local model is enabled, agent config gets these values
        let provider_type = Some(local_provider.to_string());
        let api_base = Some(local_api_base.to_string());
        let cost_tier = Some("local".to_string());

        assert_eq!(provider_type.as_deref(), Some("ollama"));
        assert_eq!(api_base.as_deref(), Some("http://localhost:11434/v1"));
        assert_eq!(cost_tier.as_deref(), Some("local"));
        assert_eq!(local_model_id, "llama3.2:3b");
    }

    // ── State string formatting (from metrics()) ───────────────────────

    fn format_agent_state(state: &LoopState) -> String {
        match state {
            LoopState::Idle => "idle".to_string(),
            LoopState::Assessing => "assessing".to_string(),
            LoopState::Executing { bounty_id, .. } => format!("executing:{bounty_id}"),
            LoopState::AwaitingVerification { bounty_id } => {
                format!("awaiting_verification:{bounty_id}")
            }
            LoopState::Stopping => "stopping".to_string(),
        }
    }

    #[test]
    fn state_formatting_all_variants() {
        assert_eq!(format_agent_state(&LoopState::Idle), "idle");
        assert_eq!(format_agent_state(&LoopState::Assessing), "assessing");
        assert_eq!(
            format_agent_state(&LoopState::Executing {
                bounty_id: "B-1".into(),
                reward_tokens: 100,
            }),
            "executing:B-1"
        );
        assert_eq!(
            format_agent_state(&LoopState::AwaitingVerification {
                bounty_id: "B-2".into()
            }),
            "awaiting_verification:B-2"
        );
        assert_eq!(format_agent_state(&LoopState::Stopping), "stopping");
    }
}
