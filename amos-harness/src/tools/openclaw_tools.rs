//! OpenClaw agent management tools
//!
//! Tools for managing autonomous AI agents via the OpenClaw platform.
//! OpenClaw agents are external, self-directed AI agents that register with AMOS
//! and can be managed like employees — they do real work, not just messaging.

use super::{Tool, ToolCategory, ToolResult};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};

// ── RegisterAgent ────────────────────────────────────────────────────────

/// Register a new OpenClaw agent with AMOS
pub struct RegisterAgentTool {
    db_pool: PgPool,
}

impl RegisterAgentTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for RegisterAgentTool {
    fn name(&self) -> &str {
        "register_agent"
    }

    fn description(&self) -> &str {
        "Register a new OpenClaw autonomous agent with AMOS. Agents are like AI employees — they can independently perform tasks such as research, code generation, data analysis, web scraping, report writing, and more."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Agent name (e.g. 'research-agent', 'data-analyst')"
                },
                "display_name": {
                    "type": "string",
                    "description": "Human-readable name (e.g. 'Research Agent', 'Data Analyst')"
                },
                "role": {
                    "type": "string",
                    "description": "Description of the agent's role and responsibilities"
                },
                "model": {
                    "type": "string",
                    "description": "LLM model the agent uses (e.g. 'claude-3-5-sonnet', 'gpt-4o', 'llama-3'). Defaults to 'claude-3-5-sonnet'"
                },
                "capabilities": {
                    "type": "array",
                    "description": "List of capabilities: 'shell', 'browser', 'file_system', 'api_calls', 'code_generation', 'web_search', 'email'",
                    "items": {
                        "type": "string"
                    }
                },
                "system_prompt": {
                    "type": "string",
                    "description": "Custom system prompt defining the agent's behavior and expertise"
                }
            },
            "required": ["name", "display_name", "role"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("name is required".to_string()))?;

        let display_name = params["display_name"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("display_name is required".to_string())
        })?;

        let role = params["role"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("role is required".to_string()))?;

        let model = params
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-3-5-sonnet");
        let capabilities = params.get("capabilities").cloned().unwrap_or(json!([]));
        let system_prompt = params.get("system_prompt").and_then(|v| v.as_str());

        let result = sqlx::query(
            r#"
            INSERT INTO openclaw_agents (name, display_name, role, model, capabilities, system_prompt, status, trust_level)
            VALUES ($1, $2, $3, $4, $5, $6, 'registered', 0)
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(display_name)
        .bind(role)
        .bind(model)
        .bind(&capabilities)
        .bind(system_prompt)
        .fetch_one(&self.db_pool)
        .await;

        match result {
            Ok(row) => {
                let agent_id: i32 = row.get(0);
                Ok(ToolResult::success(json!({
                    "agent_id": agent_id,
                    "name": name,
                    "display_name": display_name,
                    "role": role,
                    "model": model,
                    "status": "registered",
                    "message": format!("Agent '{}' registered successfully. Use assign_task to give it work.", display_name)
                })))
            }
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to register agent: {}",
                e
            ))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::OpenClaw
    }
}

// ── ListAgents ───────────────────────────────────────────────────────────

/// List all registered OpenClaw agents
pub struct ListAgentsTool {
    db_pool: PgPool,
}

impl ListAgentsTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for ListAgentsTool {
    fn name(&self) -> &str {
        "list_agents"
    }

    fn description(&self) -> &str {
        "List all registered OpenClaw agents, their roles, status, and trust levels"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "status_filter": {
                    "type": "string",
                    "enum": ["registered", "active", "working", "idle", "stopped", "error"],
                    "description": "Filter agents by status (optional)"
                }
            }
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let status_filter = params.get("status_filter").and_then(|v| v.as_str());

        let rows = if let Some(status) = status_filter {
            sqlx::query(
                "SELECT id, name, display_name, role, model, capabilities, status, trust_level, created_at \
                 FROM openclaw_agents WHERE status = $1 ORDER BY created_at DESC",
            )
            .bind(status)
            .fetch_all(&self.db_pool)
            .await
        } else {
            sqlx::query(
                "SELECT id, name, display_name, role, model, capabilities, status, trust_level, created_at \
                 FROM openclaw_agents ORDER BY created_at DESC",
            )
            .fetch_all(&self.db_pool)
            .await
        };

        match rows {
            Ok(rows) => {
                let mut agents = Vec::new();
                for row in rows {
                    let id: i32 = row.get(0);
                    let name: String = row.get(1);
                    let display_name: String = row.get(2);
                    let role: String = row.get(3);
                    let model: String = row.get(4);
                    let capabilities: JsonValue = row.get(5);
                    let status: String = row.get(6);
                    let trust_level: i32 = row.get(7);

                    agents.push(json!({
                        "id": id,
                        "name": name,
                        "display_name": display_name,
                        "role": role,
                        "model": model,
                        "capabilities": capabilities,
                        "status": status,
                        "trust_level": trust_level
                    }));
                }

                Ok(ToolResult::success(json!({
                    "agents": agents,
                    "count": agents.len()
                })))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to list agents: {}", e))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::OpenClaw
    }
}

// ── AssignTask ───────────────────────────────────────────────────────────

/// Assign a task to an OpenClaw agent
pub struct AssignTaskTool {
    db_pool: PgPool,
}

impl AssignTaskTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for AssignTaskTool {
    fn name(&self) -> &str {
        "assign_task"
    }

    fn description(&self) -> &str {
        "Assign a task to an OpenClaw agent. The agent will work on it autonomously and report back when done. Tasks can be anything: research, writing, data analysis, code generation, etc."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "integer",
                    "description": "ID of the agent to assign the task to"
                },
                "title": {
                    "type": "string",
                    "description": "Short title for the task"
                },
                "description": {
                    "type": "string",
                    "description": "Detailed description of what the agent should do"
                },
                "priority": {
                    "type": "string",
                    "enum": ["low", "normal", "high", "urgent"],
                    "description": "Task priority (default: normal)"
                },
                "context": {
                    "type": "object",
                    "description": "Additional context data the agent may need (JSON)"
                }
            },
            "required": ["agent_id", "title", "description"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let agent_id = params["agent_id"]
            .as_i64()
            .ok_or_else(|| amos_core::AmosError::Validation("agent_id is required".to_string()))?
            as i32;

        let title = params["title"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("title is required".to_string()))?;

        let description = params["description"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("description is required".to_string())
        })?;

        let priority = params
            .get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");
        let context = params.get("context").cloned().unwrap_or(json!({}));

        // Verify agent exists
        let agent_exists = sqlx::query("SELECT id FROM openclaw_agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(&self.db_pool)
            .await;

        match agent_exists {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Ok(ToolResult::error(format!(
                    "Agent with id {} not found",
                    agent_id
                )));
            }
            Err(e) => {
                return Ok(ToolResult::error(format!("Database error: {}", e)));
            }
        }

        // Create task record
        let result = sqlx::query(
            r#"
            INSERT INTO openclaw_tasks (agent_id, title, description, priority, context, status)
            VALUES ($1, $2, $3, $4, $5, 'pending')
            RETURNING id
            "#,
        )
        .bind(agent_id)
        .bind(title)
        .bind(description)
        .bind(priority)
        .bind(&context)
        .fetch_one(&self.db_pool)
        .await;

        match result {
            Ok(row) => {
                let task_id: i32 = row.get(0);

                // Update agent status to working
                let _ = sqlx::query("UPDATE openclaw_agents SET status = 'working' WHERE id = $1")
                    .bind(agent_id)
                    .execute(&self.db_pool)
                    .await;

                Ok(ToolResult::success(json!({
                    "task_id": task_id,
                    "agent_id": agent_id,
                    "title": title,
                    "priority": priority,
                    "status": "pending",
                    "message": format!("Task '{}' assigned to agent {}. The agent will work on it autonomously.", title, agent_id)
                })))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to assign task: {}", e))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::OpenClaw
    }
}

// ── GetAgentStatus ───────────────────────────────────────────────────────

/// Get detailed status of an OpenClaw agent and its tasks
pub struct GetAgentStatusTool {
    db_pool: PgPool,
}

impl GetAgentStatusTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for GetAgentStatusTool {
    fn name(&self) -> &str {
        "get_agent_status"
    }

    fn description(&self) -> &str {
        "Get the current status of an OpenClaw agent, including its active and recent tasks"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "integer",
                    "description": "Agent ID to check"
                }
            },
            "required": ["agent_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let agent_id = params["agent_id"]
            .as_i64()
            .ok_or_else(|| amos_core::AmosError::Validation("agent_id is required".to_string()))?
            as i32;

        // Get agent info
        let agent_row = sqlx::query(
            "SELECT id, name, display_name, role, model, status, trust_level, capabilities \
             FROM openclaw_agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&self.db_pool)
        .await;

        let agent_row = match agent_row {
            Ok(Some(row)) => row,
            Ok(None) => {
                return Ok(ToolResult::error(format!(
                    "Agent with id {} not found",
                    agent_id
                )));
            }
            Err(e) => {
                return Ok(ToolResult::error(format!("Database error: {}", e)));
            }
        };

        let agent = json!({
            "id": agent_row.get::<i32, _>(0),
            "name": agent_row.get::<String, _>(1),
            "display_name": agent_row.get::<String, _>(2),
            "role": agent_row.get::<String, _>(3),
            "model": agent_row.get::<String, _>(4),
            "status": agent_row.get::<String, _>(5),
            "trust_level": agent_row.get::<i32, _>(6),
            "capabilities": agent_row.get::<JsonValue, _>(7),
        });

        // Get recent tasks for this agent
        let task_rows = sqlx::query(
            "SELECT id, title, status, priority, created_at \
             FROM openclaw_tasks WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 10",
        )
        .bind(agent_id)
        .fetch_all(&self.db_pool)
        .await;

        let tasks = match task_rows {
            Ok(rows) => {
                let mut tasks = Vec::new();
                for row in rows {
                    tasks.push(json!({
                        "task_id": row.get::<i32, _>(0),
                        "title": row.get::<String, _>(1),
                        "status": row.get::<String, _>(2),
                        "priority": row.get::<String, _>(3),
                    }));
                }
                tasks
            }
            Err(_) => Vec::new(),
        };

        Ok(ToolResult::success(json!({
            "agent": agent,
            "recent_tasks": tasks,
            "active_task_count": tasks.iter().filter(|t| {
                t["status"].as_str().is_some_and(|s| s == "pending" || s == "in_progress")
            }).count()
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::OpenClaw
    }
}

// ── StopAgent ────────────────────────────────────────────────────────────

/// Stop an OpenClaw agent
pub struct StopAgentTool {
    db_pool: PgPool,
}

impl StopAgentTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for StopAgentTool {
    fn name(&self) -> &str {
        "stop_agent"
    }

    fn description(&self) -> &str {
        "Stop an OpenClaw agent. This will mark the agent as stopped and cancel any pending tasks."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "integer",
                    "description": "Agent ID to stop"
                }
            },
            "required": ["agent_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let agent_id = params["agent_id"]
            .as_i64()
            .ok_or_else(|| amos_core::AmosError::Validation("agent_id is required".to_string()))?
            as i32;

        // Update agent status
        let result = sqlx::query("UPDATE openclaw_agents SET status = 'stopped' WHERE id = $1")
            .bind(agent_id)
            .execute(&self.db_pool)
            .await;

        match result {
            Ok(r) => {
                if r.rows_affected() == 0 {
                    return Ok(ToolResult::error(format!(
                        "Agent with id {} not found",
                        agent_id
                    )));
                }

                // Cancel pending tasks
                let _ = sqlx::query(
                    "UPDATE openclaw_tasks SET status = 'cancelled' WHERE agent_id = $1 AND status IN ('pending', 'in_progress')",
                )
                .bind(agent_id)
                .execute(&self.db_pool)
                .await;

                Ok(ToolResult::success(json!({
                    "agent_id": agent_id,
                    "status": "stopped",
                    "message": "Agent stopped and pending tasks cancelled"
                })))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to stop agent: {}", e))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::OpenClaw
    }
}
