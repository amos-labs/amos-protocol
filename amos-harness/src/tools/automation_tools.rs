//! Automation tools — let the AI agent create, manage, and test automations.
//!
//! Two tools:
//! - `manage_automation`: Create, update, delete, or list automation rules (CRUD)
//! - `test_automation`: Manually fire an automation with sample data

use super::{Tool, ToolCategory, ToolResult};
use crate::automations::engine::AutomationEngine;
use crate::automations::{TriggerEvent, TriggerType};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use uuid::Uuid;

// ── ManageAutomation ────────────────────────────────────────────────────

pub struct ManageAutomationTool {
    engine: Arc<AutomationEngine>,
}

impl ManageAutomationTool {
    pub fn new(engine: Arc<AutomationEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl Tool for ManageAutomationTool {
    fn name(&self) -> &str {
        "manage_automation"
    }

    fn description(&self) -> &str {
        "Create, update, delete, or list automation rules. Operations: 'create' (new rule), 'update' (modify existing), 'delete' (remove), 'list' (show all). Trigger types: record_created, record_updated, record_deleted, schedule, webhook, manual. Action types: create_record, update_record, send_notification, call_webhook, run_agent_task."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "update", "delete", "list"],
                    "description": "Operation to perform"
                },
                "automation_id": {
                    "type": "string",
                    "description": "UUID of the automation (required for update/delete)"
                },
                "name": {
                    "type": "string",
                    "description": "Human-readable name (required for create)"
                },
                "description": {
                    "type": "string",
                    "description": "What this automation does"
                },
                "enabled": {
                    "type": "boolean",
                    "description": "Enable or disable (for update)"
                },
                "trigger_type": {
                    "type": "string",
                    "enum": ["record_created", "record_updated", "record_deleted", "schedule", "webhook", "manual"],
                    "description": "When this automation fires (required for create)"
                },
                "trigger_config": {
                    "type": "object",
                    "description": "Trigger config. Record: {\"collection\": \"orders\"}. Schedule: {\"cron\": \"0 9 * * MON\"}. Webhook: {\"path\": \"my-hook\"}."
                },
                "condition": {
                    "type": "object",
                    "description": "Optional condition — field match against trigger data. E.g. {\"status\": \"paid\"}"
                },
                "action_type": {
                    "type": "string",
                    "enum": ["create_record", "update_record", "send_notification", "call_webhook", "run_agent_task"],
                    "description": "Action to take when triggered (required for create)"
                },
                "action_config": {
                    "type": "object",
                    "description": "Action config. create_record: {\"collection\": \"audit_log\", \"data_template\": {...}}. call_webhook: {\"url\": \"...\", \"method\": \"POST\"}. run_agent_task: {\"title\": \"...\", \"description\": \"...\"}."
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let operation = params["operation"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("operation is required".to_string()))?;

        match operation {
            "create" => self.create(params).await,
            "update" => self.update(params).await,
            "delete" => self.delete(params).await,
            "list" => self.list().await,
            _ => Ok(ToolResult::error(format!(
                "Unknown operation '{}'. Use: create, update, delete, list",
                operation
            ))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Automation
    }
}

impl ManageAutomationTool {
    async fn create(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("name is required for create".to_string())
        })?;

        let description = params.get("description").and_then(|v| v.as_str());

        let trigger_type = params["trigger_type"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("trigger_type is required for create".to_string())
        })?;

        let trigger_config = params
            .get("trigger_config")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let condition = params.get("condition").cloned();

        let action_type = params["action_type"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("action_type is required for create".to_string())
        })?;

        let action_config = params
            .get("action_config")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let automation = self
            .engine
            .create_automation(
                name,
                description,
                trigger_type,
                trigger_config,
                condition,
                action_type,
                action_config,
            )
            .await?;

        Ok(ToolResult::success(json!({
            "automation_id": automation.id.to_string(),
            "name": automation.name,
            "trigger_type": automation.trigger_type.as_str(),
            "action_type": automation.action_type.as_str(),
            "enabled": automation.enabled,
            "message": format!("Automation '{}' created successfully", automation.name)
        })))
    }

    async fn update(&self, params: JsonValue) -> Result<ToolResult> {
        let id_str = params["automation_id"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("automation_id is required for update".to_string())
        })?;

        let id = Uuid::parse_str(id_str)
            .map_err(|_| amos_core::AmosError::Validation(format!("Invalid UUID: {}", id_str)))?;

        let automation = self.engine.update_automation(id, params).await?;

        Ok(ToolResult::success(json!({
            "automation_id": automation.id.to_string(),
            "name": automation.name,
            "enabled": automation.enabled,
            "trigger_type": automation.trigger_type.as_str(),
            "action_type": automation.action_type.as_str(),
            "message": "Automation updated successfully"
        })))
    }

    async fn delete(&self, params: JsonValue) -> Result<ToolResult> {
        let id_str = params["automation_id"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("automation_id is required for delete".to_string())
        })?;

        let id = Uuid::parse_str(id_str)
            .map_err(|_| amos_core::AmosError::Validation(format!("Invalid UUID: {}", id_str)))?;

        self.engine.delete_automation(id).await?;

        Ok(ToolResult::success(json!({
            "deleted": true,
            "automation_id": id_str,
            "message": "Automation deleted successfully"
        })))
    }

    async fn list(&self) -> Result<ToolResult> {
        let automations = self.engine.list_automations().await?;

        let mut results = Vec::new();
        for a in &automations {
            let runs = self.engine.get_runs(a.id, 1).await.unwrap_or_default();
            let last_run = runs.first().map(|r| {
                json!({
                    "status": r.status,
                    "at": r.created_at.to_rfc3339(),
                    "duration_ms": r.duration_ms,
                })
            });

            results.push(json!({
                "id": a.id.to_string(),
                "name": a.name,
                "description": a.description,
                "enabled": a.enabled,
                "trigger_type": a.trigger_type.as_str(),
                "trigger_config": a.trigger_config,
                "action_type": a.action_type.as_str(),
                "action_config": a.action_config,
                "last_run": last_run,
                "created_at": a.created_at.to_rfc3339(),
            }));
        }

        Ok(ToolResult::success(json!({
            "automations": results,
            "count": results.len()
        })))
    }
}

// ── TestAutomation ───────────────────────────────────────────────────────

pub struct TestAutomationTool {
    engine: Arc<AutomationEngine>,
}

impl TestAutomationTool {
    pub fn new(engine: Arc<AutomationEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl Tool for TestAutomationTool {
    fn name(&self) -> &str {
        "test_automation"
    }

    fn description(&self) -> &str {
        "Manually fire an automation with sample trigger data to test it. Returns the execution result immediately."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "automation_id": {
                    "type": "string",
                    "description": "UUID of the automation to test"
                },
                "trigger_data": {
                    "type": "object",
                    "description": "Sample trigger data to use for the test run"
                }
            },
            "required": ["automation_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let id_str = params["automation_id"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("automation_id is required".to_string())
        })?;

        let id = Uuid::parse_str(id_str)
            .map_err(|_| amos_core::AmosError::Validation(format!("Invalid UUID: {}", id_str)))?;

        let trigger_data = params
            .get("trigger_data")
            .cloned()
            .unwrap_or_else(|| json!({"test": true}));

        // Fire as a manual event (synchronous-ish via fire_event)
        let event = TriggerEvent {
            event_type: TriggerType::Manual,
            collection: None,
            record_id: None,
            data: trigger_data.clone(),
        };

        // Get the automation and execute directly
        let automation = self.engine.get_automation(id).await?;

        // Use fire_event which will spawn the execution
        self.engine.fire_event(event).await;

        // Wait a moment for the run to complete, then fetch latest run
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let runs = self.engine.get_runs(id, 1).await.unwrap_or_default();
        let latest_run = runs.first();

        Ok(ToolResult::success(json!({
            "automation_id": id_str,
            "automation_name": automation.name,
            "trigger_data": trigger_data,
            "run": latest_run.map(|r| json!({
                "status": r.status,
                "result": r.result,
                "error": r.error,
                "duration_ms": r.duration_ms,
            })),
            "message": format!("Test fired for automation '{}'", automation.name)
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Automation
    }
}
