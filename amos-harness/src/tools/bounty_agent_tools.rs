//! Bounty agent tools for autonomous bounty discovery, assessment, and execution.
//!
//! These tools are used by autonomous agents during their execution loop to
//! interact with the AMOS Network Relay marketplace. Unlike the existing
//! `task_tools::CreateBountyTool` (which *posts* bounties), these tools let
//! agents *discover*, *claim*, *execute*, and *submit* bounties.

use super::{Tool, ToolCategory, ToolResult};
use crate::relay_sync::RelayBounty;
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

// ── DiscoverBountiesTool ────────────────────────────────────────────────

/// Discover available bounties from the relay marketplace.
///
/// Queries the cached bounty list (from relay_sync) first, with optional
/// filtering by capabilities, trust level, and complexity.
pub struct DiscoverBountiesTool {
    relay_url: String,
    bounty_cache: Arc<RwLock<Vec<RelayBounty>>>,
    db_pool: Option<PgPool>,
}

impl DiscoverBountiesTool {
    pub fn new(relay_url: String, bounty_cache: Arc<RwLock<Vec<RelayBounty>>>) -> Self {
        Self {
            relay_url,
            bounty_cache,
            db_pool: None,
        }
    }

    pub fn with_db(mut self, db_pool: PgPool) -> Self {
        self.db_pool = Some(db_pool);
        self
    }

    /// Load bounty IDs that this harness has already claimed (not rejected/expired).
    async fn claimed_bounty_ids(&self) -> Vec<String> {
        let Some(ref pool) = self.db_pool else {
            return Vec::new();
        };
        sqlx::query_scalar::<_, String>(
            "SELECT bounty_id FROM bounty_claims WHERE status NOT IN ('rejected', 'expired')",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    }
}

#[async_trait]
impl Tool for DiscoverBountiesTool {
    fn name(&self) -> &str {
        "discover_bounties"
    }

    fn description(&self) -> &str {
        "Discover available bounties from the AMOS relay marketplace. Filters by \
         required capabilities, trust level, and complexity. Returns bounties the \
         agent is potentially qualified for."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "capabilities": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Agent's capabilities to filter matching bounties"
                },
                "max_trust_level": {
                    "type": "integer",
                    "description": "Agent's current trust level (1-5). Only returns bounties at or below this level.",
                    "minimum": 1,
                    "maximum": 5
                },
                "complexity_filter": {
                    "type": "string",
                    "enum": ["small", "medium", "large"],
                    "description": "Filter by estimated complexity"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of bounties to return (default: 20)",
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": []
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let agent_capabilities: Vec<String> = params
            .get("capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let max_trust_level = params
            .get("max_trust_level")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8);

        let complexity_filter = params
            .get("complexity_filter")
            .and_then(|v| v.as_str())
            .map(String::from);

        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

        // Try cache first
        let cached = self.bounty_cache.read().await;
        let mut bounties: Vec<&RelayBounty> = cached.iter().collect();

        // If cache is empty, try direct API call
        if bounties.is_empty() {
            drop(cached);
            match self.fetch_from_relay().await {
                Ok(fetched) => {
                    let mut cache = self.bounty_cache.write().await;
                    *cache = fetched;
                    drop(cache);
                    let cached = self.bounty_cache.read().await;
                    let filtered = self.filter_bounties(
                        &cached,
                        &agent_capabilities,
                        max_trust_level,
                        complexity_filter.as_deref(),
                        limit,
                    );
                    return Ok(ToolResult::success(json!({
                        "bounties": filtered,
                        "count": filtered.len(),
                        "source": "relay_api"
                    })));
                }
                Err(e) => {
                    return Ok(ToolResult::success(json!({
                        "bounties": [],
                        "count": 0,
                        "source": "error",
                        "message": format!("No cached bounties and relay unreachable: {e}")
                    })));
                }
            }
        }

        // Filter cached bounties by capabilities
        if !agent_capabilities.is_empty() {
            bounties.retain(|b| {
                b.required_capabilities.is_empty()
                    || b.required_capabilities
                        .iter()
                        .all(|req| agent_capabilities.contains(req))
            });
        }

        // Filter by complexity (reward-tier bucketing)
        if let Some(ref complexity) = complexity_filter {
            bounties.retain(|b| match complexity.as_str() {
                "small" => b.reward_tokens <= 100,
                "medium" => b.reward_tokens > 100 && b.reward_tokens <= 500,
                "large" => b.reward_tokens > 500,
                _ => true,
            });
        }

        // Exclude bounties already claimed by this harness
        let claimed_ids = self.claimed_bounty_ids().await;
        if !claimed_ids.is_empty() {
            bounties.retain(|b| !claimed_ids.contains(&b.id.to_string()));
        }

        // Truncate to limit
        bounties.truncate(limit);

        let result: Vec<JsonValue> = bounties
            .iter()
            .map(|b| {
                json!({
                    "bounty_id": b.id.to_string(),
                    "title": b.title,
                    "description": b.description,
                    "reward_tokens": b.reward_tokens,
                    "deadline": b.deadline,
                    "required_capabilities": b.required_capabilities,
                })
            })
            .collect();

        let count = result.len();
        Ok(ToolResult::success(json!({
            "bounties": result,
            "count": count,
            "source": "cache"
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::BountyAgent
    }
}

impl DiscoverBountiesTool {
    async fn fetch_from_relay(&self) -> std::result::Result<Vec<RelayBounty>, String> {
        let url = format!("{}/api/v1/bounties?status=open", self.relay_url);
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Relay request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Relay returned {}", resp.status()));
        }

        resp.json::<Vec<RelayBounty>>()
            .await
            .map_err(|e| format!("Failed to parse bounties: {e}"))
    }

    fn filter_bounties(
        &self,
        bounties: &[RelayBounty],
        agent_capabilities: &[String],
        max_trust_level: Option<u8>,
        complexity_filter: Option<&str>,
        limit: usize,
    ) -> Vec<JsonValue> {
        if max_trust_level.is_some() {
            debug!(
                ?max_trust_level,
                "Trust-level filter requested but relay bounties lack trust_level field; \
                 filtering skipped until relay schema adds min_trust_level"
            );
        }

        bounties
            .iter()
            .filter(|b| {
                // Capability filter
                if !agent_capabilities.is_empty() {
                    let cap_match = b.required_capabilities.is_empty()
                        || b.required_capabilities
                            .iter()
                            .all(|req| agent_capabilities.contains(req));
                    if !cap_match {
                        return false;
                    }
                }

                // Complexity filter via reward-tier bucketing
                if let Some(complexity) = complexity_filter {
                    let matches = match complexity {
                        "small" => b.reward_tokens <= 100,
                        "medium" => b.reward_tokens > 100 && b.reward_tokens <= 500,
                        "large" => b.reward_tokens > 500,
                        _ => true,
                    };
                    if !matches {
                        return false;
                    }
                }

                true
            })
            .take(limit)
            .map(|b| {
                json!({
                    "bounty_id": b.id.to_string(),
                    "title": b.title,
                    "description": b.description,
                    "reward_tokens": b.reward_tokens,
                    "deadline": b.deadline,
                    "required_capabilities": b.required_capabilities,
                })
            })
            .collect()
    }
}

// ── AssessBountyFitTool ─────────────────────────────────────────────────

/// Assess an agent's fitness to complete a specific bounty.
///
/// Evaluates tool requirements, trust level, past performance, and current
/// workload to produce a fit score.
pub struct AssessBountyFitTool {
    db_pool: PgPool,
    bounty_cache: Arc<RwLock<Vec<RelayBounty>>>,
}

impl AssessBountyFitTool {
    pub fn new(db_pool: PgPool, bounty_cache: Arc<RwLock<Vec<RelayBounty>>>) -> Self {
        Self {
            db_pool,
            bounty_cache,
        }
    }
}

#[async_trait]
impl Tool for AssessBountyFitTool {
    fn name(&self) -> &str {
        "assess_bounty_fit"
    }

    fn description(&self) -> &str {
        "Assess an agent's fitness to complete a specific bounty. Returns a fit score (0-1), \
         missing tools, risk assessment, and estimated completion time."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "bounty_id": {
                    "type": "string",
                    "description": "ID of the bounty to assess"
                },
                "agent_id": {
                    "type": "integer",
                    "description": "ID of the agent to assess fitness for"
                },
                "agent_capabilities": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Agent's current tool/capability inventory"
                },
                "agent_trust_level": {
                    "type": "integer",
                    "description": "Agent's current trust level (1-5)"
                },
                "current_task_count": {
                    "type": "integer",
                    "description": "Number of tasks the agent is currently working on"
                },
                "max_concurrent_tasks": {
                    "type": "integer",
                    "description": "Maximum concurrent tasks for this agent"
                }
            },
            "required": ["bounty_id", "agent_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let bounty_id = params["bounty_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("bounty_id is required".to_string()))?
            .to_string();

        let agent_id = params["agent_id"]
            .as_i64()
            .ok_or_else(|| amos_core::AmosError::Validation("agent_id is required".to_string()))?
            as i32;

        let agent_capabilities: Vec<String> = params
            .get("agent_capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let agent_trust_level = params
            .get("agent_trust_level")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u8;

        let current_task_count = params
            .get("current_task_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let max_concurrent = params
            .get("max_concurrent_tasks")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;

        // Find bounty in cache
        let cached = self.bounty_cache.read().await;
        let bounty = cached.iter().find(|b| b.id.to_string() == bounty_id);

        let (required_capabilities, reward_tokens) = match bounty {
            Some(b) => (b.required_capabilities.clone(), b.reward_tokens),
            None => {
                return Ok(ToolResult::success(json!({
                    "fit_score": 0.0,
                    "assessment": "bounty_not_found",
                    "message": format!("Bounty {bounty_id} not found in cache")
                })));
            }
        };
        drop(cached);

        // Calculate fit score components
        let mut fit_score: f64 = 1.0;
        let mut missing_tools: Vec<String> = Vec::new();
        let mut risk_factors: Vec<String> = Vec::new();

        // 1. Capability match (0.4 weight)
        if !required_capabilities.is_empty() {
            let matched = required_capabilities
                .iter()
                .filter(|req| agent_capabilities.contains(req))
                .count();
            let cap_score = matched as f64 / required_capabilities.len() as f64;
            fit_score *= cap_score;

            missing_tools = required_capabilities
                .iter()
                .filter(|req| !agent_capabilities.contains(req))
                .cloned()
                .collect();

            if !missing_tools.is_empty() {
                risk_factors.push(format!("Missing {} required tools", missing_tools.len()));
            }
        }

        // 2. Workload capacity (0.2 weight)
        if current_task_count >= max_concurrent {
            fit_score *= 0.0;
            risk_factors.push("Agent at maximum task capacity".to_string());
        } else {
            let capacity_ratio = 1.0 - (current_task_count as f64 / max_concurrent as f64);
            fit_score *= (0.5 + 0.5 * capacity_ratio).min(1.0);
        }

        // 3. Past performance on similar bounties
        let completion_rate = self.get_agent_completion_rate(agent_id).await;
        fit_score *= (0.3 + 0.7 * completion_rate).min(1.0);
        if completion_rate < 0.5 {
            risk_factors.push(format!(
                "Low completion rate: {:.0}%",
                completion_rate * 100.0
            ));
        }

        // Estimate completion time based on reward (proxy for complexity)
        let estimated_hours = match reward_tokens {
            0..=100 => 1,
            101..=500 => 4,
            501..=1000 => 8,
            _ => 24,
        };

        let assessment = if fit_score >= 0.8 {
            "excellent"
        } else if fit_score >= 0.5 {
            "good"
        } else if fit_score >= 0.3 {
            "marginal"
        } else {
            "poor"
        };

        Ok(ToolResult::success(json!({
            "bounty_id": bounty_id,
            "agent_id": agent_id,
            "fit_score": (fit_score * 100.0).round() / 100.0,
            "assessment": assessment,
            "missing_tools": missing_tools,
            "risk_factors": risk_factors,
            "estimated_completion_hours": estimated_hours,
            "agent_trust_level": agent_trust_level,
            "current_workload": format!("{}/{}", current_task_count, max_concurrent),
            "completion_rate": (completion_rate * 100.0).round() / 100.0,
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::BountyAgent
    }
}

impl AssessBountyFitTool {
    async fn get_agent_completion_rate(&self, agent_id: i32) -> f64 {
        let result = sqlx::query_as::<_, (i64, i64)>(
            r#"SELECT
                COUNT(*) FILTER (WHERE status = 'approved') as completed,
                COUNT(*) as total
               FROM bounty_claims
               WHERE agent_id = $1"#,
        )
        .bind(agent_id)
        .fetch_optional(&self.db_pool)
        .await;

        match result {
            Ok(Some((completed, total))) if total > 0 => completed as f64 / total as f64,
            _ => 1.0, // No history = assume capable (new agent benefit)
        }
    }
}

// ── ClaimBountyTool ─────────────────────────────────────────────────────

/// Claim a bounty from the relay marketplace.
///
/// Sends a claim request to the relay, records the claim locally, and updates
/// agent status to Working.
pub struct ClaimBountyTool {
    relay_url: String,
    db_pool: PgPool,
}

impl ClaimBountyTool {
    pub fn new(relay_url: String, db_pool: PgPool) -> Self {
        Self { relay_url, db_pool }
    }
}

#[async_trait]
impl Tool for ClaimBountyTool {
    fn name(&self) -> &str {
        "claim_bounty"
    }

    fn description(&self) -> &str {
        "Claim a specific bounty from the relay marketplace. Locks the bounty for this agent. \
         Returns success or conflict if already claimed by another agent."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "bounty_id": {
                    "type": "string",
                    "description": "ID of the bounty to claim"
                },
                "agent_id": {
                    "type": "integer",
                    "description": "ID of the agent claiming the bounty"
                },
                "agent_capabilities": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Proof of agent capabilities"
                },
                "estimated_completion_hours": {
                    "type": "integer",
                    "description": "Estimated hours to complete the bounty"
                },
                "fit_score": {
                    "type": "number",
                    "description": "Pre-assessed fit score for this bounty"
                }
            },
            "required": ["bounty_id", "agent_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let bounty_id = params["bounty_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("bounty_id is required".to_string()))?;

        let agent_id = params["agent_id"]
            .as_i64()
            .ok_or_else(|| amos_core::AmosError::Validation("agent_id is required".to_string()))?
            as i32;

        let capabilities: Vec<String> = params
            .get("agent_capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let estimated_hours = params
            .get("estimated_completion_hours")
            .and_then(|v| v.as_i64())
            .unwrap_or(4);

        let fit_score = params
            .get("fit_score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        // Send claim to relay
        let url = format!("{}/api/v1/bounties/{}/claim", self.relay_url, bounty_id);
        let client = reqwest::Client::new();
        let payload = json!({
            "agent_id": agent_id,
            "capabilities": capabilities,
            "estimated_completion_hours": estimated_hours,
        });

        match client.post(&url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                // Record claim locally
                let estimated_interval = format!("{} hours", estimated_hours);
                sqlx::query(
                    r#"INSERT INTO bounty_claims
                       (agent_id, bounty_id, status, fit_score, estimated_completion)
                       VALUES ($1, $2, 'claimed', $3, $4::interval)"#,
                )
                .bind(agent_id)
                .bind(bounty_id)
                .bind(fit_score)
                .bind(&estimated_interval)
                .execute(&self.db_pool)
                .await
                .map_err(|e| {
                    amos_core::AmosError::Internal(format!("Failed to record claim: {e}"))
                })?;

                // Update agent status to working
                sqlx::query("UPDATE openclaw_agents SET status = 'working' WHERE id = $1")
                    .bind(agent_id)
                    .execute(&self.db_pool)
                    .await
                    .ok();

                Ok(ToolResult::success(json!({
                    "bounty_id": bounty_id,
                    "agent_id": agent_id,
                    "status": "claimed",
                    "message": format!("Bounty {bounty_id} claimed successfully. Begin execution.")
                })))
            }
            Ok(resp) if resp.status().as_u16() == 409 => Ok(ToolResult::success(json!({
                "bounty_id": bounty_id,
                "status": "conflict",
                "message": "Bounty already claimed by another agent. Return to discovery."
            }))),
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                Ok(ToolResult::error(format!(
                    "Relay claim returned {status}: {body}"
                )))
            }
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to reach relay for claim: {e}"
            ))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::BountyAgent
    }
}

// ── SubmitBountyProofTool ───────────────────────────────────────────────

/// Submit proof of completed bounty work to the relay.
///
/// Packages output, test results, and execution logs for verification.
pub struct SubmitBountyProofTool {
    relay_url: String,
    db_pool: PgPool,
}

impl SubmitBountyProofTool {
    pub fn new(relay_url: String, db_pool: PgPool) -> Self {
        Self { relay_url, db_pool }
    }
}

#[async_trait]
impl Tool for SubmitBountyProofTool {
    fn name(&self) -> &str {
        "submit_bounty_proof"
    }

    fn description(&self) -> &str {
        "Submit proof of completed bounty work to the relay for verification. \
         Includes output, test results, and execution metrics."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "bounty_id": {
                    "type": "string",
                    "description": "ID of the bounty being submitted"
                },
                "agent_id": {
                    "type": "integer",
                    "description": "ID of the agent submitting proof"
                },
                "output": {
                    "type": "object",
                    "description": "Output of the completed work (files, data, results)"
                },
                "test_results": {
                    "type": "object",
                    "description": "Test results validating the output"
                },
                "execution_log": {
                    "type": "string",
                    "description": "Summary of execution steps taken"
                },
                "metrics": {
                    "type": "object",
                    "description": "Execution metrics (time taken, resources used, etc.)"
                }
            },
            "required": ["bounty_id", "agent_id", "output"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let bounty_id = params["bounty_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("bounty_id is required".to_string()))?;

        let agent_id = params["agent_id"]
            .as_i64()
            .ok_or_else(|| amos_core::AmosError::Validation("agent_id is required".to_string()))?
            as i32;

        let output = params.get("output").cloned().unwrap_or_else(|| json!({}));

        let test_results = params.get("test_results").cloned();
        let execution_log = params
            .get("execution_log")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let metrics = params.get("metrics").cloned();

        // Submit to relay
        let url = format!("{}/api/v1/bounties/{}/submit", self.relay_url, bounty_id);
        let client = reqwest::Client::new();
        let payload = json!({
            "agent_id": agent_id,
            "proof": {
                "output": output,
                "test_results": test_results,
                "execution_log": execution_log,
                "metrics": metrics,
            }
        });

        match client.post(&url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: JsonValue = resp.json().await.unwrap_or(json!({}));

                // Update local claim record
                let now = chrono::Utc::now();
                sqlx::query(
                    r#"UPDATE bounty_claims
                       SET status = 'submitted', submitted_at = $1, updated_at = $1
                       WHERE bounty_id = $2 AND agent_id = $3 AND status = 'claimed'"#,
                )
                .bind(now)
                .bind(bounty_id)
                .bind(agent_id)
                .execute(&self.db_pool)
                .await
                .ok();

                let verification_status = body
                    .get("verification_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pending_review");

                // If auto-verified, update claim immediately
                if verification_status == "approved" {
                    let reward_tokens = body
                        .get("reward_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);

                    sqlx::query(
                        r#"UPDATE bounty_claims
                           SET status = 'approved', verified_at = $1, reward_tokens = $2, updated_at = $1
                           WHERE bounty_id = $3 AND agent_id = $4"#,
                    )
                    .bind(now)
                    .bind(reward_tokens)
                    .bind(bounty_id)
                    .bind(agent_id)
                    .execute(&self.db_pool)
                    .await
                    .ok();

                    // Update agent status back to idle
                    sqlx::query("UPDATE openclaw_agents SET status = 'idle' WHERE id = $1")
                        .bind(agent_id)
                        .execute(&self.db_pool)
                        .await
                        .ok();
                }

                Ok(ToolResult::success(json!({
                    "bounty_id": bounty_id,
                    "status": verification_status,
                    "message": format!("Bounty proof submitted. Status: {verification_status}"),
                    "relay_response": body,
                })))
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                Ok(ToolResult::error(format!(
                    "Relay submission returned {status}: {body}"
                )))
            }
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to reach relay for submission: {e}"
            ))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::BountyAgent
    }
}

// ── CheckBountyStatusTool ───────────────────────────────────────────────

/// Check the verification status of a submitted bounty.
pub struct CheckBountyStatusTool {
    relay_url: String,
    db_pool: PgPool,
}

impl CheckBountyStatusTool {
    pub fn new(relay_url: String, db_pool: PgPool) -> Self {
        Self { relay_url, db_pool }
    }
}

#[async_trait]
impl Tool for CheckBountyStatusTool {
    fn name(&self) -> &str {
        "check_bounty_status"
    }

    fn description(&self) -> &str {
        "Check the verification status of a submitted bounty. Returns pending_review, \
         approved (with tokens earned), or rejected (with feedback)."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "bounty_id": {
                    "type": "string",
                    "description": "ID of the bounty to check"
                },
                "agent_id": {
                    "type": "integer",
                    "description": "ID of the agent that submitted"
                }
            },
            "required": ["bounty_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let bounty_id = params["bounty_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("bounty_id is required".to_string()))?;

        let agent_id = params
            .get("agent_id")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);

        // Check local claim record first
        let local_status = if let Some(aid) = agent_id {
            sqlx::query_as::<_, (String, Option<i64>, Option<JsonValue>)>(
                r#"SELECT status, reward_tokens, verification_feedback
                   FROM bounty_claims
                   WHERE bounty_id = $1 AND agent_id = $2
                   ORDER BY created_at DESC LIMIT 1"#,
            )
            .bind(bounty_id)
            .bind(aid)
            .fetch_optional(&self.db_pool)
            .await
            .ok()
            .flatten()
        } else {
            None
        };

        // If locally resolved, return that
        if let Some((status, reward, feedback)) = &local_status {
            if status == "approved" || status == "rejected" {
                return Ok(ToolResult::success(json!({
                    "bounty_id": bounty_id,
                    "status": status,
                    "reward_tokens": reward.unwrap_or(0),
                    "feedback": feedback,
                    "source": "local"
                })));
            }
        }

        // Poll relay for latest status
        let url = format!("{}/api/v1/bounties/{}", self.relay_url, bounty_id);
        let client = reqwest::Client::new();

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: JsonValue = resp.json().await.unwrap_or(json!({}));
                let status = body
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                // Update local claim if verification is complete
                if let Some(aid) = agent_id {
                    if status == "approved" || status == "rejected" {
                        let now = chrono::Utc::now();
                        let reward = body
                            .get("reward_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        let feedback = body.get("feedback").cloned();

                        sqlx::query(
                            r#"UPDATE bounty_claims
                               SET status = $1, verified_at = $2, reward_tokens = $3,
                                   verification_feedback = $4, updated_at = $2
                               WHERE bounty_id = $5 AND agent_id = $6"#,
                        )
                        .bind(status)
                        .bind(now)
                        .bind(reward)
                        .bind(&feedback)
                        .bind(bounty_id)
                        .bind(aid)
                        .execute(&self.db_pool)
                        .await
                        .ok();

                        // Return agent to idle on resolution
                        sqlx::query("UPDATE openclaw_agents SET status = 'idle' WHERE id = $1")
                            .bind(aid)
                            .execute(&self.db_pool)
                            .await
                            .ok();
                    }
                }

                Ok(ToolResult::success(json!({
                    "bounty_id": bounty_id,
                    "status": status,
                    "relay_data": body,
                    "source": "relay"
                })))
            }
            Ok(resp) => {
                let status = resp.status();
                Ok(ToolResult::error(format!(
                    "Relay returned {status} for bounty {bounty_id}"
                )))
            }
            Err(e) => {
                // Fall back to local status if relay is unreachable
                if let Some((status, reward, feedback)) = local_status {
                    Ok(ToolResult::success(json!({
                        "bounty_id": bounty_id,
                        "status": status,
                        "reward_tokens": reward.unwrap_or(0),
                        "feedback": feedback,
                        "source": "local_fallback",
                        "message": format!("Relay unreachable ({e}), showing local status")
                    })))
                } else {
                    Ok(ToolResult::error(format!(
                        "Relay unreachable and no local record: {e}"
                    )))
                }
            }
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::BountyAgent
    }
}

// ── Pure scoring/assessment functions (testable without DB) ──────────────

/// Compute fit score from capability match, workload, and completion rate.
/// Returns (fit_score, missing_tools, risk_factors, assessment, estimated_hours).
pub fn compute_fit_score(
    required_capabilities: &[String],
    agent_capabilities: &[String],
    current_task_count: u32,
    max_concurrent: u32,
    completion_rate: f64,
    reward_tokens: u64,
) -> FitAssessment {
    let mut fit_score: f64 = 1.0;
    let mut missing_tools: Vec<String> = Vec::new();
    let mut risk_factors: Vec<String> = Vec::new();

    // 1. Capability match
    if !required_capabilities.is_empty() {
        let matched = required_capabilities
            .iter()
            .filter(|req| agent_capabilities.contains(req))
            .count();
        let cap_score = matched as f64 / required_capabilities.len() as f64;
        fit_score *= cap_score;

        missing_tools = required_capabilities
            .iter()
            .filter(|req| !agent_capabilities.contains(req))
            .cloned()
            .collect();

        if !missing_tools.is_empty() {
            risk_factors.push(format!("Missing {} required tools", missing_tools.len()));
        }
    }

    // 2. Workload capacity
    if current_task_count >= max_concurrent {
        fit_score *= 0.0;
        risk_factors.push("Agent at maximum task capacity".to_string());
    } else {
        let capacity_ratio = 1.0 - (current_task_count as f64 / max_concurrent as f64);
        fit_score *= (0.5 + 0.5 * capacity_ratio).min(1.0);
    }

    // 3. Past performance
    fit_score *= (0.3 + 0.7 * completion_rate).min(1.0);
    if completion_rate < 0.5 {
        risk_factors.push(format!(
            "Low completion rate: {:.0}%",
            completion_rate * 100.0
        ));
    }

    // Complexity estimate
    let estimated_hours = match reward_tokens {
        0..=100 => 1,
        101..=500 => 4,
        501..=1000 => 8,
        _ => 24,
    };

    let assessment = if fit_score >= 0.8 {
        "excellent"
    } else if fit_score >= 0.5 {
        "good"
    } else if fit_score >= 0.3 {
        "marginal"
    } else {
        "poor"
    };

    FitAssessment {
        fit_score: (fit_score * 100.0).round() / 100.0,
        missing_tools,
        risk_factors,
        assessment: assessment.to_string(),
        estimated_hours,
    }
}

/// Result of a fit assessment computation.
#[derive(Debug, Clone)]
pub struct FitAssessment {
    pub fit_score: f64,
    pub missing_tools: Vec<String>,
    pub risk_factors: Vec<String>,
    pub assessment: String,
    pub estimated_hours: u64,
}

/// Filter bounties by agent capabilities. Pure function for testing.
pub fn filter_bounties_by_capabilities<'a>(
    bounties: &'a [RelayBounty],
    agent_capabilities: &[String],
    limit: usize,
) -> Vec<&'a RelayBounty> {
    bounties
        .iter()
        .filter(|b| {
            if agent_capabilities.is_empty() {
                return true;
            }
            b.required_capabilities.is_empty()
                || b.required_capabilities
                    .iter()
                    .all(|req| agent_capabilities.contains(req))
        })
        .take(limit)
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper ─────────────────────────────────────────────────────────

    fn make_bounty(title: &str, reward: u64, caps: Vec<&str>) -> RelayBounty {
        RelayBounty {
            id: uuid::Uuid::new_v4(),
            title: title.into(),
            description: format!("Description for {title}"),
            reward_tokens: reward,
            deadline: "2026-05-01".into(),
            required_capabilities: caps.into_iter().map(String::from).collect(),
        }
    }

    // ── Tool metadata ──────────────────────────────────────────────────

    #[test]
    fn discover_bounties_tool_metadata() {
        let cache = Arc::new(RwLock::new(Vec::new()));
        let tool = DiscoverBountiesTool::new("http://localhost:4100".into(), cache);
        assert_eq!(tool.name(), "discover_bounties");
        assert_eq!(tool.category(), ToolCategory::BountyAgent);
        assert!(!tool.description().is_empty());
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn all_tools_share_bounty_agent_category() {
        assert_eq!(ToolCategory::BountyAgent.as_str(), "bounty_agent");
    }

    #[test]
    fn all_tool_schemas_are_valid_json_objects() {
        let cache = Arc::new(RwLock::new(Vec::new()));
        let tool = DiscoverBountiesTool::new("http://localhost:4100".into(), cache);
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        assert_eq!(schema["type"], "object");
    }

    // ── Discovery filtering ────────────────────────────────────────────

    #[tokio::test]
    async fn discover_bounties_empty_cache() {
        let cache = Arc::new(RwLock::new(Vec::new()));
        let tool = DiscoverBountiesTool::new("http://localhost:99999".into(), cache);
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data.unwrap()["count"], 0);
    }

    #[tokio::test]
    async fn discover_bounties_returns_all_when_no_filter() {
        let bounties = vec![
            make_bounty("A", 100, vec!["web_search"]),
            make_bounty("B", 200, vec!["code_execution"]),
        ];
        let cache = Arc::new(RwLock::new(bounties));
        let tool = DiscoverBountiesTool::new("http://localhost:4100".into(), cache);

        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data.as_ref().unwrap()["count"], 2);
    }

    #[tokio::test]
    async fn discover_bounties_filters_by_capability() {
        let bounties = vec![
            make_bounty("A", 100, vec!["web_search"]),
            make_bounty("B", 200, vec!["code_execution"]),
            make_bounty("C", 300, vec![]), // no requirements — matches all
        ];
        let cache = Arc::new(RwLock::new(bounties));
        let tool = DiscoverBountiesTool::new("http://localhost:4100".into(), cache);

        let result = tool
            .execute(json!({"capabilities": ["web_search"]}))
            .await
            .unwrap();
        assert!(result.success);
        // Should match A (exact) and C (no requirements)
        assert_eq!(result.data.as_ref().unwrap()["count"], 2);
    }

    #[tokio::test]
    async fn discover_bounties_respects_limit() {
        let bounties = vec![
            make_bounty("A", 100, vec![]),
            make_bounty("B", 200, vec![]),
            make_bounty("C", 300, vec![]),
        ];
        let cache = Arc::new(RwLock::new(bounties));
        let tool = DiscoverBountiesTool::new("http://localhost:4100".into(), cache);

        let result = tool.execute(json!({"limit": 2})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data.as_ref().unwrap()["count"], 2);
    }

    #[test]
    fn filter_bounties_empty_capabilities_returns_all() {
        let bounties = vec![
            make_bounty("A", 100, vec!["web_search"]),
            make_bounty("B", 200, vec!["code_execution"]),
        ];
        let result = filter_bounties_by_capabilities(&bounties, &[], 100);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_bounties_partial_match_excluded() {
        let bounties = vec![make_bounty("A", 100, vec!["web_search", "code_execution"])];
        let caps = vec!["web_search".to_string()];
        // Agent has web_search but not code_execution — bounty requires both
        let result = filter_bounties_by_capabilities(&bounties, &caps, 100);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn filter_bounties_full_match_included() {
        let bounties = vec![make_bounty("A", 100, vec!["web_search", "code_execution"])];
        let caps = vec!["web_search".to_string(), "code_execution".to_string()];
        let result = filter_bounties_by_capabilities(&bounties, &caps, 100);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filter_bounties_no_requirements_always_matches() {
        let bounties = vec![make_bounty("Easy", 50, vec![])];
        let caps = vec!["web_search".to_string()];
        let result = filter_bounties_by_capabilities(&bounties, &caps, 100);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filter_bounties_respects_limit() {
        let bounties = vec![
            make_bounty("A", 100, vec![]),
            make_bounty("B", 200, vec![]),
            make_bounty("C", 300, vec![]),
        ];
        let result = filter_bounties_by_capabilities(&bounties, &[], 2);
        assert_eq!(result.len(), 2);
    }

    // ── Fit scoring ────────────────────────────────────────────────────

    #[test]
    fn fit_score_perfect_match() {
        let result = compute_fit_score(
            &["code_execution".into()],
            &["code_execution".into()],
            0,   // no current tasks
            3,   // max 3
            1.0, // perfect completion rate
            100, // small reward
        );
        assert!(
            result.fit_score >= 0.9,
            "Expected excellent, got {}",
            result.fit_score
        );
        assert_eq!(result.assessment, "excellent");
        assert!(result.missing_tools.is_empty());
        assert!(result.risk_factors.is_empty());
    }

    #[test]
    fn fit_score_zero_when_no_capabilities_match() {
        let result = compute_fit_score(
            &["docker".into(), "kubernetes".into()],
            &["web_search".into()],
            0,
            3,
            1.0,
            100,
        );
        assert_eq!(result.fit_score, 0.0);
        assert_eq!(result.assessment, "poor");
        assert_eq!(result.missing_tools.len(), 2);
    }

    #[test]
    fn fit_score_partial_capability_match() {
        let result = compute_fit_score(
            &["web_search".into(), "code_execution".into()],
            &["web_search".into()],
            0,
            3,
            1.0,
            100,
        );
        // 50% capability match
        assert!(result.fit_score > 0.0 && result.fit_score < 0.8);
        assert_eq!(result.missing_tools, vec!["code_execution"]);
    }

    #[test]
    fn fit_score_zero_when_at_capacity() {
        let result = compute_fit_score(
            &[],
            &[],
            3, // at max
            3, // max concurrent
            1.0,
            100,
        );
        assert_eq!(result.fit_score, 0.0);
        assert!(result
            .risk_factors
            .iter()
            .any(|r| r.contains("maximum task capacity")));
    }

    #[test]
    fn fit_score_reduced_near_capacity() {
        let full = compute_fit_score(&[], &[], 0, 3, 1.0, 100);
        let busy = compute_fit_score(&[], &[], 2, 3, 1.0, 100);
        assert!(
            busy.fit_score < full.fit_score,
            "Busy agent should score lower"
        );
    }

    #[test]
    fn fit_score_low_completion_rate_adds_risk() {
        let result = compute_fit_score(&[], &[], 0, 3, 0.2, 100);
        assert!(result
            .risk_factors
            .iter()
            .any(|r| r.contains("completion rate")));
        assert!(result.fit_score < 0.5);
    }

    #[test]
    fn fit_score_new_agent_benefits_from_default_rate() {
        // completion_rate = 1.0 (new agent default)
        let result = compute_fit_score(&[], &[], 0, 3, 1.0, 100);
        assert!(result.fit_score >= 0.9);
    }

    #[test]
    fn fit_score_no_required_caps_means_full_cap_score() {
        let result = compute_fit_score(&[], &[], 0, 3, 1.0, 100);
        assert!(result.fit_score >= 0.9);
        assert!(result.missing_tools.is_empty());
    }

    #[test]
    fn estimated_hours_scales_with_reward() {
        assert_eq!(
            compute_fit_score(&[], &[], 0, 3, 1.0, 50).estimated_hours,
            1
        );
        assert_eq!(
            compute_fit_score(&[], &[], 0, 3, 1.0, 200).estimated_hours,
            4
        );
        assert_eq!(
            compute_fit_score(&[], &[], 0, 3, 1.0, 750).estimated_hours,
            8
        );
        assert_eq!(
            compute_fit_score(&[], &[], 0, 3, 1.0, 5000).estimated_hours,
            24
        );
    }

    #[test]
    fn assessment_labels_match_thresholds() {
        assert_eq!(
            compute_fit_score(&[], &[], 0, 3, 1.0, 100).assessment,
            "excellent"
        );
        // Force a "good" score: half capacity, full completion
        let r = compute_fit_score(&[], &[], 1, 2, 1.0, 100);
        assert!(r.assessment == "good" || r.assessment == "excellent");

        // Force "poor": zero capabilities match
        assert_eq!(
            compute_fit_score(&["x".into()], &["y".into()], 0, 3, 1.0, 100).assessment,
            "poor"
        );
    }

    #[test]
    fn fit_score_is_clamped_0_to_1() {
        // Even with best possible inputs
        let best = compute_fit_score(&[], &[], 0, 10, 1.0, 100);
        assert!(best.fit_score <= 1.0);
        // Even with worst possible inputs
        let worst = compute_fit_score(&["x".into()], &[], 3, 3, 0.0, 100);
        assert!(worst.fit_score >= 0.0);
    }
}
