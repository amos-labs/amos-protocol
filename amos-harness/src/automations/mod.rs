//! Event & Automation System
//!
//! Automations react to data changes, schedules, and webhooks to trigger actions
//! (create records, send notifications, call webhooks, run agent tasks).
//!
//! ## Trigger types
//!
//! - `record_created` / `record_updated` / `record_deleted` — fires when schema records change
//! - `schedule` — cron-based scheduling (checked every 60 seconds)
//! - `webhook` — incoming HTTP POST to `/api/v1/hooks/{path}`
//! - `manual` — fired explicitly via the `test_automation` agent tool

pub mod engine;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// ── Types ────────────────────────────────────────────────────────────────

/// How an automation is triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    RecordCreated,
    RecordUpdated,
    RecordDeleted,
    Schedule,
    Webhook,
    Manual,
    /// Fired after another automation completes successfully. Use the
    /// `automation_id` field in `trigger_config` to chain a specific upstream
    /// automation. Enables multi-step flows (e.g., "when onboarding emails
    /// automation finishes, run the welcome-task automation").
    AutomationCompleted,
}

impl TriggerType {
    pub fn as_str(&self) -> &str {
        match self {
            TriggerType::RecordCreated => "record_created",
            TriggerType::RecordUpdated => "record_updated",
            TriggerType::RecordDeleted => "record_deleted",
            TriggerType::Schedule => "schedule",
            TriggerType::Webhook => "webhook",
            TriggerType::Manual => "manual",
            TriggerType::AutomationCompleted => "automation_completed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "record_created" => Some(TriggerType::RecordCreated),
            "record_updated" => Some(TriggerType::RecordUpdated),
            "record_deleted" => Some(TriggerType::RecordDeleted),
            "schedule" => Some(TriggerType::Schedule),
            "webhook" => Some(TriggerType::Webhook),
            "manual" => Some(TriggerType::Manual),
            "automation_completed" => Some(TriggerType::AutomationCompleted),
            _ => None,
        }
    }
}

/// What action an automation performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    CreateRecord,
    UpdateRecord,
    SendNotification,
    CallWebhook,
    RunAgentTask,
    CreateBounty,
}

impl ActionType {
    pub fn as_str(&self) -> &str {
        match self {
            ActionType::CreateRecord => "create_record",
            ActionType::UpdateRecord => "update_record",
            ActionType::SendNotification => "send_notification",
            ActionType::CallWebhook => "call_webhook",
            ActionType::RunAgentTask => "run_agent_task",
            ActionType::CreateBounty => "create_bounty",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "create_record" => Some(ActionType::CreateRecord),
            "update_record" => Some(ActionType::UpdateRecord),
            "send_notification" => Some(ActionType::SendNotification),
            "call_webhook" => Some(ActionType::CallWebhook),
            "run_agent_task" => Some(ActionType::RunAgentTask),
            "create_bounty" => Some(ActionType::CreateBounty),
            _ => None,
        }
    }
}

/// An automation rule stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub trigger_type: TriggerType,
    pub trigger_config: JsonValue,
    pub condition: Option<JsonValue>,
    pub action_type: ActionType,
    pub action_config: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A log entry for an automation execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRun {
    pub id: Uuid,
    pub automation_id: Uuid,
    pub trigger_data: JsonValue,
    pub status: String,
    pub result: Option<JsonValue>,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// An event that can trigger automations.
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    pub event_type: TriggerType,
    pub collection: Option<String>,
    pub record_id: Option<Uuid>,
    pub data: JsonValue,
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trigger_type_roundtrip() {
        let types = vec![
            TriggerType::RecordCreated,
            TriggerType::RecordUpdated,
            TriggerType::RecordDeleted,
            TriggerType::Schedule,
            TriggerType::Webhook,
            TriggerType::Manual,
            TriggerType::AutomationCompleted,
        ];
        for tt in types {
            assert_eq!(TriggerType::from_str(tt.as_str()), Some(tt));
        }
    }

    #[test]
    fn automation_completed_serializes_as_snake_case() {
        assert_eq!(
            TriggerType::AutomationCompleted.as_str(),
            "automation_completed"
        );
    }

    #[test]
    fn action_type_roundtrip() {
        let types = vec![
            ActionType::CreateRecord,
            ActionType::UpdateRecord,
            ActionType::SendNotification,
            ActionType::CallWebhook,
            ActionType::RunAgentTask,
            ActionType::CreateBounty,
        ];
        for at in types {
            assert_eq!(ActionType::from_str(at.as_str()), Some(at));
        }
    }

    #[test]
    fn trigger_event_construction() {
        let event = TriggerEvent {
            event_type: TriggerType::RecordCreated,
            collection: Some("contacts".to_string()),
            record_id: Some(Uuid::new_v4()),
            data: json!({"name": "Alice"}),
        };
        assert_eq!(event.event_type, TriggerType::RecordCreated);
        assert_eq!(event.collection.as_deref(), Some("contacts"));
    }

    #[test]
    fn automation_serde_roundtrip() {
        let automation = Automation {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            description: Some("A test automation".to_string()),
            enabled: true,
            trigger_type: TriggerType::RecordCreated,
            trigger_config: json!({"collection": "orders"}),
            condition: None,
            action_type: ActionType::CreateRecord,
            action_config: json!({"collection": "audit_log"}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let serialized = serde_json::to_string(&automation).unwrap();
        let deserialized: Automation = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.trigger_type, TriggerType::RecordCreated);
        assert_eq!(deserialized.action_type, ActionType::CreateRecord);
    }

    #[test]
    fn unknown_trigger_type_returns_none() {
        assert_eq!(TriggerType::from_str("unknown"), None);
    }

    #[test]
    fn unknown_action_type_returns_none() {
        assert_eq!(ActionType::from_str("unknown"), None);
    }
}
