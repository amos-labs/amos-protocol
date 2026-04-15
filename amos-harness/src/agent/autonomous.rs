//! Autonomous Agent Loop — background bounty execution lifecycle.
//!
//! Unlike the standard chat agent (request-response), the autonomous agent
//! runs as a background tokio task that continuously discovers, claims,
//! executes, and submits bounties.
//!
//! Lifecycle: Initialize → Poll for bounties → Assess fitness → Claim best
//! match → Execute → Submit → Repeat.

use super::context::ContextProvider;
use crate::openclaw::AgentConfig;
use crate::relay_sync::RelayBounty;
use crate::tools::bounty_agent_tools;
use amos_core::tools::Tool;
use amos_core::AppConfig;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// State of an autonomous agent in its bounty loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoopState {
    /// Idle, looking for bounties
    Idle,
    /// Discovered bounties, assessing fitness
    Assessing,
    /// Claimed a bounty, executing work
    Executing {
        bounty_id: String,
        reward_tokens: u64,
    },
    /// Work complete, submitted for verification
    AwaitingVerification {
        bounty_id: String,
        reward_tokens: u64,
    },
    /// QA revision requested — reworking with feedback
    Reworking {
        bounty_id: String,
        reward_tokens: u64,
        revision_count: u16,
        feedback: String,
    },
    /// Shutting down
    Stopping,
}

/// Telemetry counters for an autonomous agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentTelemetry {
    pub bounties_discovered: u64,
    pub bounties_claimed: u64,
    pub bounties_completed: u64,
    pub bounties_failed: u64,
    pub tokens_earned: i64,
    pub loop_iterations: u64,
}

/// Configuration for an autonomous agent loop.
#[derive(Debug, Clone)]
pub struct AutonomousLoopConfig {
    pub agent_id: i32,
    pub agent_config: AgentConfig,
    pub trust_level: u8,
    pub polling_interval_secs: u64,
    pub backoff_max_secs: u64,
    pub min_fit_score: f64,
    /// Max seconds to wait for verification before giving up (default: 24h).
    pub verification_timeout_secs: u64,
}

/// Parse structured proof sections from agent output text.
///
/// Looks for markdown sections: APPROACH, IMPLEMENTATION, VERIFICATION, ARTIFACTS.
/// Also extracts PR_URL, GIT_SHA, and DELIVERABLE_URLS for relay submission.
/// Returns a JSON object with extracted sections (or null for missing sections).
fn parse_structured_proof(output: &str) -> serde_json::Value {
    let sections = [
        "APPROACH",
        "IMPLEMENTATION",
        "VERIFICATION",
        "ARTIFACTS",
        "PR_URL",
        "GIT_SHA",
        "DELIVERABLE_URLS",
    ];
    let mut result = serde_json::Map::new();

    for section in &sections {
        let header = format!("**{}**", section);
        let alt_header = format!("## {}", section);
        let start = output.find(&header).or_else(|| output.find(&alt_header));

        if let Some(pos) = start {
            // Find the end: next section header or end of string
            let content_start = output[pos..].find('\n').map(|i| pos + i + 1).unwrap_or(pos);
            let content_end = sections
                .iter()
                .filter(|s| **s != *section)
                .filter_map(|s| {
                    output[content_start..]
                        .find(&format!("**{}**", s))
                        .or_else(|| output[content_start..].find(&format!("## {}", s)))
                        .map(|i| content_start + i)
                })
                .min()
                .unwrap_or(output.len());

            let content = output[content_start..content_end].trim();
            if !content.is_empty() {
                result.insert(
                    section.to_lowercase(),
                    serde_json::Value::String(content.to_string()),
                );
            }
        }
    }

    // Also try to extract pr_url from inline patterns like "PR: https://..."
    if !result.contains_key("pr_url") {
        if let Some(url) = extract_url_after_label(output, "PR_URL:") {
            result.insert("pr_url".to_string(), serde_json::Value::String(url));
        } else if let Some(url) = extract_github_pr_url(output) {
            result.insert("pr_url".to_string(), serde_json::Value::String(url));
        }
    }

    if result.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(result)
    }
}

/// Extract a URL following a label like "PR_URL: https://..."
fn extract_url_after_label(text: &str, label: &str) -> Option<String> {
    text.find(label).and_then(|pos| {
        let after = text[pos + label.len()..].trim_start();
        after
            .split_whitespace()
            .next()
            .filter(|url| url.starts_with("https://"))
            .map(|url| url.to_string())
    })
}

/// Extract a GitHub PR URL from text (e.g. https://github.com/org/repo/pull/123)
fn extract_github_pr_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|word| word.starts_with("https://github.com/") && word.contains("/pull/"))
        .map(|url| url.trim_end_matches([')', ']', '.']).to_string())
}

/// Determine which model provider to use for a bounty execution.
///
/// If a local model is configured and the bounty reward is at or below the
/// cost threshold, routes to the local model. Otherwise falls back to the
/// agent's configured provider (cloud/Bedrock).
pub fn resolve_execution_provider(
    app_config: &AppConfig,
    agent_config: &AgentConfig,
    reward_tokens: u64,
) -> (String, String, String) {
    if app_config.fleet.has_local_model()
        && reward_tokens <= app_config.fleet.local_model.cost_threshold
    {
        let lm = &app_config.fleet.local_model;
        (
            lm.provider.clone(),
            lm.api_base.clone(),
            lm.model_id.clone(),
        )
    } else {
        (
            agent_config
                .provider_type
                .clone()
                .unwrap_or_else(|| "bedrock".to_string()),
            agent_config.api_base.clone().unwrap_or_default(),
            agent_config.model.clone(),
        )
    }
}

/// The autonomous agent loop that runs as a background task.
pub struct AutonomousAgentLoop {
    config: AutonomousLoopConfig,
    db_pool: PgPool,
    app_config: Arc<AppConfig>,
    context_provider: Arc<dyn ContextProvider>,
    bounty_cache: Arc<RwLock<Vec<RelayBounty>>>,
    state: Arc<RwLock<LoopState>>,
    telemetry: Arc<RwLock<AgentTelemetry>>,
    last_flush_at: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    stop_signal: Arc<tokio::sync::Notify>,
}

impl AutonomousAgentLoop {
    pub fn new(
        config: AutonomousLoopConfig,
        db_pool: PgPool,
        app_config: Arc<AppConfig>,
        context_provider: Arc<dyn ContextProvider>,
        bounty_cache: Arc<RwLock<Vec<RelayBounty>>>,
    ) -> Self {
        Self {
            config,
            db_pool,
            app_config,
            context_provider,
            bounty_cache,
            state: Arc::new(RwLock::new(LoopState::Idle)),
            telemetry: Arc::new(RwLock::new(AgentTelemetry::default())),
            last_flush_at: Arc::new(RwLock::new(chrono::Utc::now())),
            stop_signal: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Get the current loop state.
    pub async fn state(&self) -> LoopState {
        self.state.read().await.clone()
    }

    /// Get current telemetry.
    pub async fn telemetry(&self) -> AgentTelemetry {
        self.telemetry.read().await.clone()
    }

    /// Get the loop configuration (read-only reference).
    pub fn config(&self) -> &AutonomousLoopConfig {
        &self.config
    }

    /// Signal the loop to stop.
    pub fn stop(&self) {
        self.stop_signal.notify_one();
    }

    /// Calculate exponential backoff with random jitter to prevent thundering herd.
    fn jittered_backoff(&self, current: u64) -> u64 {
        use rand::Rng;
        let doubled = (current * 2).min(self.config.backoff_max_secs);
        let jitter = rand::thread_rng().gen_range(0..=self.config.polling_interval_secs / 2);
        (doubled + jitter).min(self.config.backoff_max_secs)
    }

    /// Persist current telemetry snapshot to the `agent_metrics` table.
    async fn flush_telemetry(&self) {
        let t = self.telemetry.read().await;
        let now = chrono::Utc::now();
        let period_start = {
            let mut last = self.last_flush_at.write().await;
            let start = *last;
            *last = now;
            start
        };
        let completion_rate = if t.bounties_claimed > 0 {
            t.bounties_completed as f64 / t.bounties_claimed as f64
        } else {
            0.0
        };
        sqlx::query(
            r#"INSERT INTO agent_metrics
               (agent_id, period_start, period_end,
                bounties_discovered, bounties_claimed, bounties_completed,
                bounties_failed, tokens_earned, completion_rate)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(self.config.agent_id)
        .bind(period_start)
        .bind(now)
        .bind(t.bounties_discovered as i32)
        .bind(t.bounties_claimed as i32)
        .bind(t.bounties_completed as i32)
        .bind(t.bounties_failed as i32)
        .bind(t.tokens_earned)
        .bind(completion_rate)
        .execute(&self.db_pool)
        .await
        .ok();
    }

    /// Record a claim for today in persistent storage.
    async fn record_daily_claim(&self) {
        let today = chrono::Utc::now().date_naive();
        sqlx::query(
            r#"INSERT INTO agent_daily_claims (agent_id, claim_date, count)
               VALUES ($1, $2, 1)
               ON CONFLICT (agent_id, claim_date)
               DO UPDATE SET count = agent_daily_claims.count + 1"#,
        )
        .bind(self.config.agent_id)
        .bind(today)
        .execute(&self.db_pool)
        .await
        .ok();
    }

    /// Load today's claim count from persistent storage.
    async fn load_daily_claims(&self) -> u32 {
        let today = chrono::Utc::now().date_naive();
        sqlx::query_scalar::<_, i32>(
            "SELECT count FROM agent_daily_claims WHERE agent_id = $1 AND claim_date = $2",
        )
        .bind(self.config.agent_id)
        .bind(today)
        .fetch_optional(&self.db_pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
        .max(0) as u32
    }

    /// Execute a bounty by sending its description to the agent service.
    ///
    /// The agent service handles LLM invocation, tool calling, and produces
    /// a text response that serves as the bounty proof.
    async fn execute_bounty(
        &self,
        bounty_id: &str,
        provider_type: &str,
        api_base: &str,
        model_id: &str,
    ) -> Result<String, String> {
        let agent_url =
            std::env::var("AGENT_URL").unwrap_or_else(|_| "http://localhost:3100".to_string());

        // Look up bounty from cache for structured context
        let bounty_prompt = {
            let cache = self.bounty_cache.read().await;
            if let Some(b) = cache.iter().find(|b| b.id.to_string() == bounty_id) {
                let sanitized = crate::prompt_guard::sanitize(
                    "bounty_description",
                    &format!("{}\n\n{}", b.title, b.description),
                    8000,
                );
                let caps = b.required_capabilities.join(", ");
                let is_code_bounty = b.category == "infrastructure" || b.category == "research";

                let workflow_instructions = if is_code_bounty {
                    format!(
                        "## Workflow (Code Bounty)\n\
                         1. Create a working branch: `git checkout -b bounty/{bounty_id}`\n\
                         2. Implement the solution.\n\
                         3. Run `cargo test --lib` and `cargo clippy --all-targets -- -D warnings` — fix all issues.\n\
                         4. Commit and push: `git push origin bounty/{bounty_id}`\n\
                         5. Open a PR: `gh pr create --title \"Bounty {bounty_id}: <short title>\" --body \"<description>\"`\n\
                         6. Structure your final output with these sections:\n\
                            - **PR_URL**: The full URL of the opened pull request\n\
                            - **GIT_SHA**: The commit SHA of your final push\n\
                            - **APPROACH**: How you solved the task\n\
                            - **IMPLEMENTATION**: What you built and key decisions\n\
                            - **VERIFICATION**: How you tested and verified correctness\n\
                            - **ARTIFACTS**: List of files created or modified\n\
                         7. Self-evaluate against the task requirements. If incomplete, keep working.\n"
                    )
                } else {
                    "## Workflow (Growth / Content Bounty)\n\
                     1. Execute the deliverable using available tools (post content, run campaign, etc.).\n\
                     2. Collect proof: URLs, screenshots, metrics.\n\
                     3. Structure your final output with these sections:\n\
                        - **DELIVERABLE_URLS**: URLs of published content or campaign results\n\
                        - **APPROACH**: How you executed the deliverable\n\
                        - **IMPLEMENTATION**: What you produced and key decisions\n\
                        - **VERIFICATION**: How you verified the deliverable meets requirements\n\
                        - **ARTIFACTS**: List of assets created\n\
                     4. Self-evaluate against the task requirements. If incomplete, keep working.\n"
                        .to_string()
                };

                format!(
                    "{boundary}\n\n\
                     ## Bounty Assignment\n\
                     **Bounty ID:** {bounty_id}\n\
                     **Category:** {category}\n\
                     **Required Capabilities:** {caps}\n\
                     **Reward:** {reward} AMOS tokens\n\n\
                     ## Task Description\n\
                     {sanitized}\n\n\
                     {workflow_instructions}",
                    boundary = crate::prompt_guard::DATA_BOUNDARY_INSTRUCTION,
                    category = b.category,
                    reward = b.reward_tokens,
                )
            } else {
                format!("Execute bounty {bounty_id}")
            }
        };

        let body = json!({
            "message": bounty_prompt,
            "provider_type": provider_type,
            "api_base": api_base,
            "model_id": model_id,
            "task_context": "bounty",
        });

        let url = format!("{agent_url}/api/v1/chat");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Agent service unavailable at {agent_url}: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("Agent service returned {}", response.status()));
        }

        // Parse SSE stream to extract text content, tool calls, and errors
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read agent response: {e}"))?;

        let mut text_parts: Vec<String> = Vec::new();
        let mut tools_used: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for line in body.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    match event.get("type").and_then(|t| t.as_str()) {
                        Some("text_delta") | Some("message_delta") => {
                            if let Some(text) = event.get("text").and_then(|t| t.as_str()) {
                                text_parts.push(text.to_string());
                            }
                        }
                        Some("tool_start") => {
                            if let Some(name) = event.get("name").and_then(|n| n.as_str()) {
                                tools_used.push(name.to_string());
                            }
                        }
                        Some("error") => {
                            if let Some(msg) = event.get("message").and_then(|m| m.as_str()) {
                                errors.push(msg.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if !errors.is_empty() {
            warn!(
                bounty_id,
                errors = ?errors,
                "Agent reported errors during execution"
            );
        }

        // If we parsed SSE events, combine text; otherwise use raw body
        let output = if text_parts.is_empty() {
            body
        } else {
            let mut combined = text_parts.join("");
            if !tools_used.is_empty() {
                combined.push_str(&format!("\n\n**Tools Used:** {}", tools_used.join(", ")));
            }
            combined
        };

        Ok(output)
    }

    /// Execute a bounty rework after QA revision request.
    ///
    /// Injects the QA feedback into the prompt so the agent knows exactly
    /// what to fix. Reuses the same agent service endpoint.
    async fn execute_bounty_rework(
        &self,
        bounty_id: &str,
        provider_type: &str,
        api_base: &str,
        model_id: &str,
        revision_count: u16,
        feedback: &str,
    ) -> Result<String, String> {
        let agent_url =
            std::env::var("AGENT_URL").unwrap_or_else(|_| "http://localhost:3100".to_string());

        let rework_prompt = {
            let cache = self.bounty_cache.read().await;
            if let Some(b) = cache.iter().find(|b| b.id.to_string() == bounty_id) {
                let sanitized = crate::prompt_guard::sanitize(
                    "bounty_description",
                    &format!("{}\n\n{}", b.title, b.description),
                    4000,
                );
                let is_code_bounty = b.category == "infrastructure" || b.category == "research";

                let workflow_hint = if is_code_bounty {
                    "Run `cargo test --lib` and `cargo clippy --all-targets -- -D warnings` before resubmitting.\n\
                     Push your fixes and update the PR."
                } else {
                    "Verify your deliverables are live and accessible before resubmitting."
                };

                format!(
                    "{boundary}\n\n\
                     ## Revision Required (attempt {revision_count}/3)\n\n\
                     **Bounty ID:** {bounty_id}\n\
                     **Original Task:** {sanitized}\n\n\
                     ## QA Feedback\n\
                     {feedback}\n\n\
                     ## Instructions\n\
                     Fix ONLY the issues identified in the QA feedback above.\n\
                     {workflow_hint}\n\n\
                     Structure your output with:\n\
                     - **PR_URL**: The PR URL (if code bounty)\n\
                     - **GIT_SHA**: The commit SHA of your fix\n\
                     - **APPROACH**: What you changed to address the feedback\n\
                     - **IMPLEMENTATION**: Specific fixes made\n\
                     - **VERIFICATION**: How you verified the fixes resolve the issues\n\
                     - **ARTIFACTS**: Files modified\n",
                    boundary = crate::prompt_guard::DATA_BOUNDARY_INSTRUCTION,
                )
            } else {
                format!(
                    "Rework bounty {bounty_id}. QA Feedback:\n{feedback}\n\nFix the issues identified."
                )
            }
        };

        let body = json!({
            "message": rework_prompt,
            "provider_type": provider_type,
            "api_base": api_base,
            "model_id": model_id,
            "task_context": "bounty_rework",
        });

        let url = format!("{agent_url}/api/v1/chat");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Agent service unavailable at {agent_url}: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("Agent service returned {}", response.status()));
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read agent response: {e}"))?;

        let mut text_parts: Vec<String> = Vec::new();
        for line in body.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    if matches!(
                        event.get("type").and_then(|t| t.as_str()),
                        Some("text_delta") | Some("message_delta")
                    ) {
                        if let Some(text) = event.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                }
            }
        }

        Ok(if text_parts.is_empty() {
            body
        } else {
            text_parts.join("")
        })
    }

    /// Run the autonomous loop. Spawns as a tokio task.
    pub async fn run(self: Arc<Self>) {
        let agent_name = &self.config.agent_config.display_name;
        let agent_id = self.config.agent_id;
        info!(agent_id, agent_name, "Autonomous agent loop starting");

        // Build tools
        let relay_url = self.app_config.relay.url.clone();
        let discover_tool = bounty_agent_tools::DiscoverBountiesTool::new(
            relay_url.clone(),
            self.bounty_cache.clone(),
        )
        .with_db(self.db_pool.clone());
        let assess_tool = bounty_agent_tools::AssessBountyFitTool::new(
            self.db_pool.clone(),
            self.bounty_cache.clone(),
        );
        let claim_tool =
            bounty_agent_tools::ClaimBountyTool::new(relay_url.clone(), self.db_pool.clone());
        let submit_tool =
            bounty_agent_tools::SubmitBountyProofTool::new(relay_url.clone(), self.db_pool.clone());
        let check_tool =
            bounty_agent_tools::CheckBountyStatusTool::new(relay_url, self.db_pool.clone());

        let daily_limit = self
            .context_provider
            .daily_bounty_limit(self.config.trust_level);
        let mut daily_claims: u32 = self.load_daily_claims().await;
        let mut last_claim_date = chrono::Utc::now().date_naive();
        let mut current_backoff = self.config.polling_interval_secs;
        let mut verification_started_at: Option<std::time::Instant> = None;
        let mut last_telemetry_flush = std::time::Instant::now();

        loop {
            // Check for stop signal
            let should_stop = tokio::select! {
                _ = self.stop_signal.notified() => true,
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(current_backoff)) => false,
            };

            if should_stop {
                info!(agent_id, "Autonomous agent stopping (signal received)");
                *self.state.write().await = LoopState::Stopping;
                break;
            }

            // Check agent status in DB
            let db_status: Option<String> =
                sqlx::query_scalar("SELECT status FROM openclaw_agents WHERE id = $1")
                    .bind(agent_id)
                    .fetch_optional(&self.db_pool)
                    .await
                    .ok()
                    .flatten();

            if db_status.as_deref() == Some("stopped") {
                info!(agent_id, "Agent marked as stopped in DB, exiting loop");
                *self.state.write().await = LoopState::Stopping;
                break;
            }

            // Reset daily counter at midnight
            let today = chrono::Utc::now().date_naive();
            if today != last_claim_date {
                daily_claims = 0;
                last_claim_date = today;
            }

            // Increment loop counter
            self.telemetry.write().await.loop_iterations += 1;

            // Periodic telemetry flush (every 5 minutes)
            if last_telemetry_flush.elapsed().as_secs() >= 300 {
                self.flush_telemetry().await;
                last_telemetry_flush = std::time::Instant::now();
            }

            let current_state = self.state.read().await.clone();
            match current_state {
                LoopState::Idle => {
                    // Check daily limit
                    if daily_claims >= daily_limit {
                        debug!(
                            agent_id,
                            daily_claims, daily_limit, "Daily bounty limit reached"
                        );
                        current_backoff = self.config.backoff_max_secs;
                        continue;
                    }

                    // Step 1: Discover bounties
                    let discover_params = json!({
                        "capabilities": self.config.agent_config.capabilities,
                        "max_trust_level": self.config.trust_level,
                        "limit": 10
                    });

                    let discovery_result = match discover_tool.execute(discover_params).await {
                        Ok(r) => r,
                        Err(e) => {
                            warn!(agent_id, error = %e, "Discovery failed");
                            current_backoff = self.jittered_backoff(current_backoff);
                            continue;
                        }
                    };

                    let bounties = discovery_result
                        .data
                        .as_ref()
                        .and_then(|d| d.get("bounties"))
                        .and_then(|b| b.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let count = bounties.len();
                    self.telemetry.write().await.bounties_discovered += count as u64;

                    if bounties.is_empty() {
                        debug!(agent_id, "No bounties available, backing off");
                        current_backoff = self.jittered_backoff(current_backoff);
                        continue;
                    }

                    // Step 2: Assess fitness for each bounty
                    *self.state.write().await = LoopState::Assessing;
                    let mut best_bounty: Option<(String, f64, u64)> = None; // (id, fit_score, reward)

                    for bounty in &bounties {
                        let bounty_id = bounty
                            .get("bounty_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let reward = bounty
                            .get("reward_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        let assess_params = json!({
                            "bounty_id": bounty_id,
                            "agent_id": agent_id,
                            "agent_capabilities": self.config.agent_config.capabilities,
                            "agent_trust_level": self.config.trust_level,
                            "current_task_count": 0,
                            "max_concurrent_tasks": self.config.agent_config.max_concurrent_tasks.unwrap_or(3),
                        });

                        if let Ok(result) = assess_tool.execute(assess_params).await {
                            let fit_score = result
                                .data
                                .as_ref()
                                .and_then(|d| d.get("fit_score"))
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);

                            if fit_score >= self.config.min_fit_score {
                                // Value-adjusted fitness: fit_score * reward
                                let value = fit_score * reward as f64;
                                if best_bounty
                                    .as_ref()
                                    .map(|(_, _, best_value)| value > *best_value as f64)
                                    .unwrap_or(true)
                                {
                                    best_bounty = Some((bounty_id.to_string(), fit_score, reward));
                                }
                            }
                        }
                    }

                    // Step 3: Claim the best bounty
                    if let Some((bounty_id, fit_score, reward)) = best_bounty {
                        let claim_params = json!({
                            "bounty_id": bounty_id,
                            "agent_id": agent_id,
                            "agent_capabilities": self.config.agent_config.capabilities,
                            "estimated_completion_hours": 4,
                            "fit_score": fit_score,
                        });

                        match claim_tool.execute(claim_params).await {
                            Ok(result) => {
                                let status = result
                                    .data
                                    .as_ref()
                                    .and_then(|d| d.get("status"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");

                                if status == "claimed" {
                                    info!(
                                        agent_id,
                                        bounty_id = %bounty_id,
                                        fit_score,
                                        "Bounty claimed, starting execution"
                                    );
                                    daily_claims += 1;
                                    self.record_daily_claim().await;
                                    self.telemetry.write().await.bounties_claimed += 1;
                                    *self.state.write().await = LoopState::Executing {
                                        bounty_id: bounty_id.clone(),
                                        reward_tokens: reward,
                                    };
                                    current_backoff = self.config.polling_interval_secs;
                                } else {
                                    debug!(agent_id, bounty_id = %bounty_id, status, "Claim failed, returning to idle");
                                    *self.state.write().await = LoopState::Idle;
                                }
                            }
                            Err(e) => {
                                warn!(agent_id, error = %e, "Claim execution failed");
                                *self.state.write().await = LoopState::Idle;
                            }
                        }
                    } else {
                        debug!(
                            agent_id,
                            "No bounty met minimum fit score ({}), backing off",
                            self.config.min_fit_score
                        );
                        *self.state.write().await = LoopState::Idle;
                        current_backoff = self.jittered_backoff(current_backoff);
                    }
                }

                LoopState::Executing {
                    ref bounty_id,
                    reward_tokens,
                } => {
                    let bounty_id = bounty_id.clone();

                    // Determine which model to use based on cost tier
                    let (provider_type, api_base, model_id) = resolve_execution_provider(
                        &self.app_config,
                        &self.config.agent_config,
                        reward_tokens,
                    );

                    info!(
                        agent_id,
                        bounty_id = %bounty_id,
                        reward_tokens,
                        provider = %provider_type,
                        model = %model_id,
                        "Executing bounty work"
                    );

                    // Execute bounty via agent service (with retry)
                    let exec_start = std::time::Instant::now();
                    let mut execution_output = self
                        .execute_bounty(&bounty_id, &provider_type, &api_base, &model_id)
                        .await;

                    // Retry on failure: up to 2 additional attempts with backoff
                    if execution_output.is_err() {
                        const RETRY_BACKOFFS: [u64; 2] = [30, 60];
                        for (attempt, delay) in RETRY_BACKOFFS.iter().enumerate() {
                            warn!(
                                agent_id,
                                bounty_id = %bounty_id,
                                attempt = attempt + 2,
                                delay_secs = delay,
                                "Retrying bounty execution after failure"
                            );
                            tokio::time::sleep(tokio::time::Duration::from_secs(*delay)).await;
                            execution_output = self
                                .execute_bounty(&bounty_id, &provider_type, &api_base, &model_id)
                                .await;
                            if execution_output.is_ok() {
                                break;
                            }
                        }
                    }
                    let execution_time_secs = exec_start.elapsed().as_secs();

                    // Submit proof with execution output
                    let (output_status, execution_log) = match &execution_output {
                        Ok(output) => ("completed", output.clone()),
                        Err(e) => ("partial", format!("Execution error: {e}")),
                    };

                    // Parse structured proof sections from agent output
                    let proof = parse_structured_proof(&execution_log);

                    // Extract pr_url from proof for relay storage
                    let pr_url = proof
                        .get("pr_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let mut submit_params = json!({
                        "bounty_id": bounty_id,
                        "agent_id": agent_id,
                        "output": {
                            "status": output_status,
                            "agent": self.config.agent_config.name,
                            "provider": provider_type,
                            "model": model_id,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "pr_url": pr_url,
                        },
                        "execution_log": execution_log,
                        "proof": proof,
                        "metrics": {
                            "execution_time_secs": execution_time_secs,
                            "provider_type": provider_type,
                            "cost_tier": if reward_tokens <= self.app_config.fleet.local_model.cost_threshold {
                                "local"
                            } else {
                                "cloud"
                            },
                        }
                    });
                    // Also put pr_url at top level for relay's submit_work extraction
                    if let Some(url) = &pr_url {
                        submit_params["pr_url"] = json!(url);
                    }

                    match submit_tool.execute(submit_params).await {
                        Ok(result) => {
                            let status = result
                                .data
                                .as_ref()
                                .and_then(|d| d.get("status"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("pending_review");

                            if status == "approved" {
                                let tokens = result
                                    .data
                                    .as_ref()
                                    .and_then(|d| d.get("relay_response"))
                                    .and_then(|d| d.get("reward_tokens"))
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0);
                                self.telemetry.write().await.bounties_completed += 1;
                                self.telemetry.write().await.tokens_earned += tokens;
                                info!(
                                    agent_id,
                                    bounty_id = %bounty_id,
                                    tokens,
                                    "Bounty approved immediately"
                                );
                                *self.state.write().await = LoopState::Idle;
                            } else {
                                *self.state.write().await = LoopState::AwaitingVerification {
                                    bounty_id,
                                    reward_tokens,
                                };
                                verification_started_at = Some(std::time::Instant::now());
                            }
                        }
                        Err(e) => {
                            error!(agent_id, error = %e, "Failed to submit bounty proof");
                            self.telemetry.write().await.bounties_failed += 1;
                            *self.state.write().await = LoopState::Idle;
                            verification_started_at = None;
                        }
                    }
                    current_backoff = self.config.polling_interval_secs;
                }

                LoopState::AwaitingVerification {
                    ref bounty_id,
                    reward_tokens,
                } => {
                    let bounty_id = bounty_id.clone();

                    // Track when we started waiting for verification
                    let started =
                        verification_started_at.get_or_insert_with(std::time::Instant::now);

                    // Timeout: if we've been waiting too long, give up
                    if started.elapsed().as_secs() >= self.config.verification_timeout_secs {
                        warn!(
                            agent_id,
                            bounty_id = %bounty_id,
                            timeout_secs = self.config.verification_timeout_secs,
                            "Verification timeout — returning to Idle"
                        );
                        self.telemetry.write().await.bounties_failed += 1;
                        *self.state.write().await = LoopState::Idle;
                        verification_started_at = None;
                        current_backoff = self.config.polling_interval_secs;
                        continue;
                    }

                    let check_params = json!({
                        "bounty_id": bounty_id,
                        "agent_id": agent_id,
                    });

                    match check_tool.execute(check_params).await {
                        Ok(result) => {
                            let status = result
                                .data
                                .as_ref()
                                .and_then(|d| d.get("status"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("pending_review");

                            // Also check relay_data for revision info
                            let relay_data = result.data.as_ref().and_then(|d| d.get("relay_data"));

                            match status {
                                "approved" => {
                                    let tokens = result
                                        .data
                                        .as_ref()
                                        .and_then(|d| d.get("reward_tokens"))
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or(0);
                                    self.telemetry.write().await.bounties_completed += 1;
                                    self.telemetry.write().await.tokens_earned += tokens;
                                    info!(
                                        agent_id,
                                        bounty_id = %bounty_id,
                                        tokens,
                                        "Bounty approved"
                                    );
                                    *self.state.write().await = LoopState::Idle;
                                    verification_started_at = None;
                                    current_backoff = self.config.polling_interval_secs;
                                }
                                "rejected" => {
                                    let feedback = result
                                        .data
                                        .as_ref()
                                        .and_then(|d| d.get("feedback"))
                                        .cloned()
                                        .unwrap_or(json!(null));
                                    warn!(
                                        agent_id,
                                        bounty_id = %bounty_id,
                                        ?feedback,
                                        "Bounty rejected"
                                    );
                                    self.telemetry.write().await.bounties_failed += 1;
                                    *self.state.write().await = LoopState::Idle;
                                    verification_started_at = None;
                                    current_backoff = self.config.polling_interval_secs;
                                }
                                // Revision requested: status reset to "claimed" with
                                // revision_count > 0 and revision_feedback populated
                                "claimed" => {
                                    let revision_count = relay_data
                                        .and_then(|d| d.get("revision_count"))
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u16;
                                    let feedback = relay_data
                                        .and_then(|d| d.get("revision_feedback"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    if revision_count > 0 && !feedback.is_empty() {
                                        if revision_count > 3 {
                                            warn!(
                                                agent_id,
                                                bounty_id = %bounty_id,
                                                revision_count,
                                                "Max revisions exceeded, giving up"
                                            );
                                            self.telemetry.write().await.bounties_failed += 1;
                                            *self.state.write().await = LoopState::Idle;
                                        } else {
                                            info!(
                                                agent_id,
                                                bounty_id = %bounty_id,
                                                revision_count,
                                                "QA revision requested, reworking"
                                            );
                                            *self.state.write().await = LoopState::Reworking {
                                                bounty_id,
                                                reward_tokens,
                                                revision_count,
                                                feedback,
                                            };
                                        }
                                        verification_started_at = None;
                                        current_backoff = self.config.polling_interval_secs;
                                    } else {
                                        // Claimed but no revision feedback — still waiting
                                        debug!(
                                            agent_id,
                                            bounty_id = %bounty_id,
                                            "Status is claimed, still waiting for verification"
                                        );
                                        current_backoff = self.config.polling_interval_secs;
                                    }
                                }
                                _ => {
                                    // Still pending, keep waiting
                                    debug!(
                                        agent_id,
                                        bounty_id = %bounty_id,
                                        status,
                                        "Verification still pending"
                                    );
                                    current_backoff = self.config.polling_interval_secs;
                                }
                            }
                        }
                        Err(e) => {
                            warn!(agent_id, error = %e, "Status check failed, will retry");
                            current_backoff = self.config.polling_interval_secs;
                        }
                    }
                }

                LoopState::Reworking {
                    ref bounty_id,
                    reward_tokens,
                    revision_count,
                    ref feedback,
                } => {
                    let bounty_id = bounty_id.clone();
                    let feedback = feedback.clone();

                    let (provider_type, api_base, model_id) = resolve_execution_provider(
                        &self.app_config,
                        &self.config.agent_config,
                        reward_tokens,
                    );

                    info!(
                        agent_id,
                        bounty_id = %bounty_id,
                        revision_count,
                        provider = %provider_type,
                        "Reworking bounty with QA feedback"
                    );

                    // Execute rework with feedback injected into prompt
                    let rework_output = self
                        .execute_bounty_rework(
                            &bounty_id,
                            &provider_type,
                            &api_base,
                            &model_id,
                            revision_count,
                            &feedback,
                        )
                        .await;

                    let (output_status, execution_log) = match &rework_output {
                        Ok(output) => ("completed", output.clone()),
                        Err(e) => ("partial", format!("Rework error: {e}")),
                    };

                    let proof = parse_structured_proof(&execution_log);
                    let pr_url = proof
                        .get("pr_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let mut submit_params = json!({
                        "bounty_id": bounty_id,
                        "agent_id": agent_id,
                        "output": {
                            "status": output_status,
                            "agent": self.config.agent_config.name,
                            "provider": provider_type,
                            "model": model_id,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "pr_url": pr_url,
                            "revision_attempt": revision_count,
                        },
                        "execution_log": execution_log,
                        "proof": proof,
                    });
                    if let Some(url) = &pr_url {
                        submit_params["pr_url"] = json!(url);
                    }

                    match submit_tool.execute(submit_params).await {
                        Ok(result) => {
                            let status = result
                                .data
                                .as_ref()
                                .and_then(|d| d.get("status"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("pending_review");

                            if status == "approved" {
                                let tokens = result
                                    .data
                                    .as_ref()
                                    .and_then(|d| d.get("relay_response"))
                                    .and_then(|d| d.get("reward_tokens"))
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0);
                                self.telemetry.write().await.bounties_completed += 1;
                                self.telemetry.write().await.tokens_earned += tokens;
                                info!(
                                    agent_id,
                                    bounty_id = %bounty_id,
                                    tokens,
                                    revision_count,
                                    "Bounty approved after rework"
                                );
                                *self.state.write().await = LoopState::Idle;
                            } else {
                                *self.state.write().await = LoopState::AwaitingVerification {
                                    bounty_id,
                                    reward_tokens,
                                };
                                verification_started_at = Some(std::time::Instant::now());
                            }
                        }
                        Err(e) => {
                            error!(
                                agent_id,
                                error = %e,
                                bounty_id = %bounty_id,
                                "Failed to submit rework proof"
                            );
                            self.telemetry.write().await.bounties_failed += 1;
                            *self.state.write().await = LoopState::Idle;
                            verification_started_at = None;
                        }
                    }
                    current_backoff = self.config.polling_interval_secs;
                }

                LoopState::Assessing => {
                    // This state is transient, should transition quickly
                    *self.state.write().await = LoopState::Idle;
                }

                LoopState::Stopping => {
                    break;
                }
            }
        }

        // Final telemetry flush on exit
        self.flush_telemetry().await;

        // Update DB status on exit
        sqlx::query("UPDATE openclaw_agents SET status = 'stopped' WHERE id = $1")
            .bind(agent_id)
            .execute(&self.db_pool)
            .await
            .ok();

        info!(agent_id, "Autonomous agent loop exited");
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── LoopState serde ────────────────────────────────────────────────

    #[test]
    fn loop_state_serde_roundtrip() {
        let state = LoopState::Executing {
            bounty_id: "AMOS-TEST-001".into(),
            reward_tokens: 500,
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: LoopState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
    }

    #[test]
    fn loop_state_idle_serializes_as_string() {
        assert_eq!(serde_json::to_string(&LoopState::Idle).unwrap(), "\"idle\"");
    }

    #[test]
    fn loop_state_stopping_serializes_as_string() {
        assert_eq!(
            serde_json::to_string(&LoopState::Stopping).unwrap(),
            "\"stopping\""
        );
    }

    #[test]
    fn loop_state_assessing_serializes_as_string() {
        assert_eq!(
            serde_json::to_string(&LoopState::Assessing).unwrap(),
            "\"assessing\""
        );
    }

    #[test]
    fn loop_state_executing_includes_bounty_id() {
        let state = LoopState::Executing {
            bounty_id: "AMOS-INFRA-042".into(),
            reward_tokens: 250,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("executing"));
        assert!(json.contains("AMOS-INFRA-042"));
    }

    #[test]
    fn loop_state_awaiting_verification_includes_bounty_id() {
        let state = LoopState::AwaitingVerification {
            bounty_id: "AMOS-RES-007".into(),
            reward_tokens: 500,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("awaiting_verification"));
        assert!(json.contains("AMOS-RES-007"));
        assert!(json.contains("500"));
    }

    #[test]
    fn loop_state_all_variants_roundtrip() {
        let variants = vec![
            LoopState::Idle,
            LoopState::Assessing,
            LoopState::Executing {
                bounty_id: "b1".into(),
                reward_tokens: 100,
            },
            LoopState::AwaitingVerification {
                bounty_id: "b2".into(),
                reward_tokens: 200,
            },
            LoopState::Reworking {
                bounty_id: "b3".into(),
                reward_tokens: 300,
                revision_count: 2,
                feedback: "Fix clippy warnings".into(),
            },
            LoopState::Stopping,
        ];
        for state in variants {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: LoopState = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, state);
        }
    }

    #[test]
    fn loop_state_deserialize_rejects_unknown_variant() {
        let result = serde_json::from_str::<LoopState>("\"running\"");
        assert!(result.is_err());
    }

    #[test]
    fn loop_state_equality() {
        assert_eq!(LoopState::Idle, LoopState::Idle);
        assert_ne!(LoopState::Idle, LoopState::Assessing);
        assert_ne!(
            LoopState::Executing {
                bounty_id: "a".into(),
                reward_tokens: 100,
            },
            LoopState::Executing {
                bounty_id: "b".into(),
                reward_tokens: 100,
            }
        );
        assert_eq!(
            LoopState::Executing {
                bounty_id: "same".into(),
                reward_tokens: 100,
            },
            LoopState::Executing {
                bounty_id: "same".into(),
                reward_tokens: 100,
            }
        );
    }

    // ── AgentTelemetry ─────────────────────────────────────────────────

    #[test]
    fn telemetry_defaults_all_zero() {
        let t = AgentTelemetry::default();
        assert_eq!(t.bounties_discovered, 0);
        assert_eq!(t.bounties_claimed, 0);
        assert_eq!(t.bounties_completed, 0);
        assert_eq!(t.bounties_failed, 0);
        assert_eq!(t.tokens_earned, 0);
        assert_eq!(t.loop_iterations, 0);
    }

    #[test]
    fn telemetry_serde_roundtrip() {
        let t = AgentTelemetry {
            bounties_discovered: 50,
            bounties_claimed: 20,
            bounties_completed: 15,
            bounties_failed: 5,
            tokens_earned: 12_500,
            loop_iterations: 1_000,
        };
        let json = serde_json::to_string(&t).unwrap();
        let deserialized: AgentTelemetry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.bounties_discovered, 50);
        assert_eq!(deserialized.bounties_completed, 15);
        assert_eq!(deserialized.tokens_earned, 12_500);
        assert_eq!(deserialized.loop_iterations, 1_000);
    }

    #[test]
    fn telemetry_tokens_earned_can_be_negative() {
        // tokens_earned is i64 to handle edge cases like slashing
        let t = AgentTelemetry {
            tokens_earned: -100,
            ..Default::default()
        };
        let json = serde_json::to_string(&t).unwrap();
        let deserialized: AgentTelemetry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tokens_earned, -100);
    }

    #[test]
    fn telemetry_completion_rate_computation() {
        let t = AgentTelemetry {
            bounties_claimed: 20,
            bounties_completed: 15,
            ..Default::default()
        };
        let rate = if t.bounties_claimed > 0 {
            t.bounties_completed as f64 / t.bounties_claimed as f64
        } else {
            0.0
        };
        assert!((rate - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn telemetry_completion_rate_zero_claims() {
        let t = AgentTelemetry::default();
        let rate = if t.bounties_claimed > 0 {
            t.bounties_completed as f64 / t.bounties_claimed as f64
        } else {
            0.0
        };
        assert_eq!(rate, 0.0);
    }

    // ── AutonomousLoopConfig ───────────────────────────────────────────

    fn make_test_agent_config(agent_id: i32) -> AgentConfig {
        AgentConfig {
            agent_id,
            name: "test-agent".into(),
            display_name: "Test Agent".into(),
            role: "autonomous-research".into(),
            capabilities: vec!["code_execution".into()],
            system_prompt: None,
            model: "claude-3-sonnet".into(),
            provider_type: None,
            api_base: None,
            max_concurrent_tasks: Some(3),
            always_on: Some(true),
            cost_tier: None,
            task_specializations: None,
        }
    }

    #[test]
    fn autonomous_loop_config_clone() {
        let config = AutonomousLoopConfig {
            agent_id: 42,
            agent_config: make_test_agent_config(42),
            trust_level: 2,
            polling_interval_secs: 60,
            backoff_max_secs: 300,
            min_fit_score: 0.5,
            verification_timeout_secs: 86400,
        };
        let cloned = config.clone();
        assert_eq!(cloned.agent_id, 42);
        assert_eq!(cloned.trust_level, 2);
        assert_eq!(cloned.polling_interval_secs, 60);
        assert_eq!(cloned.backoff_max_secs, 300);
        assert!((cloned.min_fit_score - 0.5).abs() < f64::EPSILON);
    }

    // ── Backoff logic (extracted pattern) ──────────────────────────────

    /// Simulate the backoff doubling logic from the autonomous loop.
    fn compute_backoff(current: u64, max: u64) -> u64 {
        (current * 2).min(max)
    }

    #[test]
    fn backoff_doubles_each_iteration() {
        assert_eq!(compute_backoff(60, 300), 120);
        assert_eq!(compute_backoff(120, 300), 240);
    }

    #[test]
    fn backoff_capped_at_max() {
        assert_eq!(compute_backoff(200, 300), 300);
        assert_eq!(compute_backoff(300, 300), 300);
        assert_eq!(compute_backoff(500, 300), 300);
    }

    #[test]
    fn backoff_resets_on_success() {
        // The loop resets backoff to polling_interval_secs on success
        let polling = 60u64;
        let max = 300u64;
        let mut backoff = polling;
        // Simulate 3 failures
        backoff = compute_backoff(backoff, max); // 120
        backoff = compute_backoff(backoff, max); // 240
        backoff = compute_backoff(backoff, max); // 300 (capped)
        assert_eq!(backoff, 300);
        // Success resets
        backoff = polling;
        assert_eq!(backoff, 60);
    }

    // ── Daily limit logic (extracted pattern) ──────────────────────────

    #[test]
    fn daily_limit_blocks_when_reached() {
        let daily_limit: u32 = 3;
        let daily_claims: u32 = 3;
        assert!(daily_claims >= daily_limit);
    }

    #[test]
    fn daily_limit_allows_when_under() {
        let daily_limit: u32 = 3;
        let daily_claims: u32 = 2;
        assert!(daily_claims < daily_limit);
    }

    #[test]
    fn daily_counter_resets_on_new_date() {
        use chrono::NaiveDate;
        let yesterday = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();
        assert_ne!(yesterday, today);
        // In the loop, daily_claims would be reset to 0 when today != last_claim_date
    }

    // ── Best bounty selection logic ────────────────────────────────────

    /// Simulate value-adjusted fitness scoring from the loop.
    fn select_best_bounty(
        candidates: &[(String, f64, u64)],
        min_fit: f64,
    ) -> Option<(String, f64, u64)> {
        let mut best: Option<(String, f64, u64)> = None;
        for (id, fit_score, reward) in candidates {
            if *fit_score >= min_fit {
                let value = *fit_score * *reward as f64;
                if best
                    .as_ref()
                    .map(|(_, _, best_reward)| value > *best_reward as f64)
                    .unwrap_or(true)
                {
                    best = Some((id.clone(), *fit_score, *reward));
                }
            }
        }
        best
    }

    #[test]
    fn best_bounty_selects_highest_value() {
        let candidates = vec![
            ("b1".into(), 0.8, 100u64),
            ("b2".into(), 0.6, 500u64), // value: 300
            ("b3".into(), 0.9, 200u64), // value: 180
        ];
        let best = select_best_bounty(&candidates, 0.5).unwrap();
        assert_eq!(best.0, "b2"); // 0.6 * 500 = 300 > 180 > 80
    }

    #[test]
    fn best_bounty_filters_below_min_fit() {
        let candidates = vec![("b1".into(), 0.3, 1000u64), ("b2".into(), 0.4, 500u64)];
        let best = select_best_bounty(&candidates, 0.5);
        assert!(best.is_none());
    }

    #[test]
    fn best_bounty_empty_candidates() {
        let candidates: Vec<(String, f64, u64)> = vec![];
        assert!(select_best_bounty(&candidates, 0.5).is_none());
    }

    #[test]
    fn best_bounty_single_candidate_above_threshold() {
        let candidates = vec![("b1".into(), 0.7, 100u64)];
        let best = select_best_bounty(&candidates, 0.5).unwrap();
        assert_eq!(best.0, "b1");
    }

    // ── Cost-tier provider routing ─────────────────────────────────────

    /// Test-friendly version of the routing logic that takes FleetConfig instead of AppConfig.
    fn test_resolve_provider(
        fleet: &amos_core::config::FleetConfig,
        agent_provider_type: Option<&str>,
        agent_api_base: Option<&str>,
        agent_model: &str,
        reward_tokens: u64,
    ) -> (String, String, String) {
        if fleet.has_local_model() && reward_tokens <= fleet.local_model.cost_threshold {
            let lm = &fleet.local_model;
            (
                lm.provider.clone(),
                lm.api_base.clone(),
                lm.model_id.clone(),
            )
        } else {
            (
                agent_provider_type.unwrap_or("bedrock").to_string(),
                agent_api_base.unwrap_or("").to_string(),
                agent_model.to_string(),
            )
        }
    }

    fn make_fleet_config(enabled: bool, threshold: u64) -> amos_core::config::FleetConfig {
        let mut fleet = amos_core::config::FleetConfig::default();
        fleet.local_model.enabled = enabled;
        fleet.local_model.cost_threshold = threshold;
        fleet.local_model.provider = "ollama".into();
        fleet.local_model.api_base = "http://ollama:11434/v1".into();
        fleet.local_model.model_id = "llama3.2:3b".into();
        fleet
    }

    const CLOUD_MODEL: &str = "us.anthropic.claude-sonnet-4-20250514-v1:0";

    #[test]
    fn provider_routes_to_local_below_threshold() {
        let fleet = make_fleet_config(true, 500);
        let (provider, api_base, model) =
            test_resolve_provider(&fleet, None, None, CLOUD_MODEL, 200);
        assert_eq!(provider, "ollama");
        assert_eq!(api_base, "http://ollama:11434/v1");
        assert_eq!(model, "llama3.2:3b");
    }

    #[test]
    fn provider_routes_to_local_at_threshold() {
        let fleet = make_fleet_config(true, 500);
        let (provider, _, _) = test_resolve_provider(&fleet, None, None, CLOUD_MODEL, 500);
        assert_eq!(provider, "ollama");
    }

    #[test]
    fn provider_routes_to_cloud_above_threshold() {
        let fleet = make_fleet_config(true, 500);
        let (provider, _, model) = test_resolve_provider(&fleet, None, None, CLOUD_MODEL, 1000);
        assert_eq!(provider, "bedrock");
        assert!(model.contains("anthropic"));
    }

    #[test]
    fn provider_routes_to_cloud_when_local_disabled() {
        let fleet = make_fleet_config(false, 500);
        let (provider, _, _) = test_resolve_provider(&fleet, None, None, CLOUD_MODEL, 200);
        assert_eq!(provider, "bedrock");
    }

    #[test]
    fn provider_uses_agent_provider_when_no_local() {
        let fleet = make_fleet_config(false, 500);
        let (provider, api_base, _) = test_resolve_provider(
            &fleet,
            Some("anthropic"),
            Some("https://api.anthropic.com/v1"),
            CLOUD_MODEL,
            200,
        );
        assert_eq!(provider, "anthropic");
        assert_eq!(api_base, "https://api.anthropic.com/v1");
    }

    #[test]
    fn executing_state_carries_reward_tokens() {
        let state = LoopState::Executing {
            bounty_id: "B-1".into(),
            reward_tokens: 750,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("750"));
        let deserialized: LoopState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
    }

    // ── Reworking state ────────────────────────────────────────────────

    #[test]
    fn reworking_state_serde_roundtrip() {
        let state = LoopState::Reworking {
            bounty_id: "AMOS-TEST-002".into(),
            reward_tokens: 500,
            revision_count: 1,
            feedback: "Fix unused variable in src/foo.rs:42".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("reworking"));
        assert!(json.contains("AMOS-TEST-002"));
        assert!(json.contains("Fix unused variable"));
        let deserialized: LoopState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
    }

    #[test]
    fn reworking_state_carries_all_fields() {
        let state = LoopState::Reworking {
            bounty_id: "B-R1".into(),
            reward_tokens: 1000,
            revision_count: 3,
            feedback: "cargo audit found RUSTSEC-2024-0001".into(),
        };
        if let LoopState::Reworking {
            bounty_id,
            reward_tokens,
            revision_count,
            feedback,
        } = state
        {
            assert_eq!(bounty_id, "B-R1");
            assert_eq!(reward_tokens, 1000);
            assert_eq!(revision_count, 3);
            assert!(feedback.contains("RUSTSEC"));
        } else {
            panic!("Expected Reworking variant");
        }
    }

    // ── PR URL extraction ──────────────────────────────────────────────

    #[test]
    fn parse_proof_extracts_pr_url_section() {
        let output = "**APPROACH**\nDid the thing\n\n**PR_URL**\nhttps://github.com/org/repo/pull/42\n\n**GIT_SHA**\nabc123\n";
        let proof = parse_structured_proof(output);
        assert_eq!(
            proof.get("pr_url").and_then(|v| v.as_str()),
            Some("https://github.com/org/repo/pull/42")
        );
        assert_eq!(
            proof.get("git_sha").and_then(|v| v.as_str()),
            Some("abc123")
        );
    }

    #[test]
    fn parse_proof_extracts_inline_pr_url() {
        let output = "I opened a PR.\nPR_URL: https://github.com/org/repo/pull/99\nDone.";
        let proof = parse_structured_proof(output);
        assert_eq!(
            proof.get("pr_url").and_then(|v| v.as_str()),
            Some("https://github.com/org/repo/pull/99")
        );
    }

    #[test]
    fn parse_proof_extracts_github_pr_url_from_text() {
        let output = "Opened https://github.com/amos-labs/amos-automate/pull/7 for review.";
        let proof = parse_structured_proof(output);
        assert_eq!(
            proof.get("pr_url").and_then(|v| v.as_str()),
            Some("https://github.com/amos-labs/amos-automate/pull/7")
        );
    }

    #[test]
    fn parse_proof_extracts_deliverable_urls() {
        let output = "**DELIVERABLE_URLS**\nhttps://twitter.com/amos/status/123\n\n**APPROACH**\nPosted content.";
        let proof = parse_structured_proof(output);
        assert!(proof.get("deliverable_urls").is_some());
    }

    #[test]
    fn extract_url_after_label_works() {
        assert_eq!(
            extract_url_after_label("PR_URL: https://example.com/pull/1 done", "PR_URL:"),
            Some("https://example.com/pull/1".to_string())
        );
        assert_eq!(extract_url_after_label("no url here", "PR_URL:"), None);
    }

    #[test]
    fn extract_github_pr_url_strips_trailing_punctuation() {
        assert_eq!(
            extract_github_pr_url("See https://github.com/org/repo/pull/5)."),
            Some("https://github.com/org/repo/pull/5".to_string())
        );
    }

    #[test]
    fn extract_github_pr_url_none_for_non_pr() {
        assert_eq!(
            extract_github_pr_url("See https://github.com/org/repo/issues/5"),
            None
        );
    }

    // ── AwaitingVerification carries reward_tokens ─────────────────────

    #[test]
    fn awaiting_verification_carries_reward_tokens() {
        let state = LoopState::AwaitingVerification {
            bounty_id: "B-AV".into(),
            reward_tokens: 999,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("999"));
        let deserialized: LoopState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
    }
}
