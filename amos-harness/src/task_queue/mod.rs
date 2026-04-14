//! Task queue infrastructure for the AMOS harness.
//!
//! Provides a unified task system with two execution tiers:
//!
//! - **Internal tasks**: Executed by harness sub-agents (background tokio tasks
//!   running lightweight agent loops). Used when AMOS can handle the work with
//!   its own tools but wants to do it asynchronously.
//!
//! - **External tasks (bounties)**: Posted to the task queue for external
//!   OpenClaw agents to claim and execute. Used when the work requires
//!   capabilities outside the harness (shell access, browser control, etc.)
//!   or when AMOS wants to delegate to specialized agents.
//!
//! The message bus (`TaskMessage`) provides buffered, persistent communication
//! between running tasks and the main AMOS conversation. AMOS checks for
//! pending messages and relays status updates, questions, and results to the user.

pub mod sub_agent;

use amos_core::{AmosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

// ── Task types ──────────────────────────────────────────────────────────

/// Category determines how a task is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    /// Handled by a harness sub-agent (background agent loop)
    Internal,
    /// Posted as a bounty for external OpenClaw agents
    External,
}

impl TaskCategory {
    pub fn as_str(&self) -> &str {
        match self {
            TaskCategory::Internal => "internal",
            TaskCategory::External => "external",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "internal" => Some(TaskCategory::Internal),
            "external" => Some(TaskCategory::External),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Task lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Created but not yet picked up
    Pending,
    /// Assigned to an agent (internal or external) but not started
    Assigned,
    /// Actively being worked on
    Running,
    /// Successfully completed
    Completed,
    /// Failed with an error
    Failed,
    /// Cancelled by AMOS or user
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(&self) -> &str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Assigned => "assigned",
            TaskStatus::Running => "running",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(TaskStatus::Pending),
            "assigned" => Some(TaskStatus::Assigned),
            "running" => Some(TaskStatus::Running),
            "completed" => Some(TaskStatus::Completed),
            "failed" => Some(TaskStatus::Failed),
            "cancelled" => Some(TaskStatus::Cancelled),
            _ => None,
        }
    }

    /// Whether the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A task in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub context: JsonValue,
    pub category: TaskCategory,
    pub task_type: Option<String>,
    pub priority: i32,
    pub status: TaskStatus,
    pub assigned_to: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    pub result: Option<JsonValue>,
    pub error_message: Option<String>,
    pub reward_tokens: i64,
    pub reward_claimed: bool,
    pub deadline_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Parameters for creating a new task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskParams {
    pub title: String,
    pub description: Option<String>,
    pub context: Option<JsonValue>,
    pub category: TaskCategory,
    pub task_type: Option<String>,
    pub priority: Option<i32>,
    pub session_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    pub reward_tokens: Option<i64>,
    pub deadline_at: Option<DateTime<Utc>>,
}

// ── Task message types ──────────────────────────────────────────────────

/// Direction of a message in the task message bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDirection {
    /// From a working agent back to AMOS
    AgentToAmos,
    /// From AMOS to a working agent (instructions, answers)
    AmosToAgent,
    /// From AMOS to the user conversation (status relay)
    AmosToUser,
}

impl MessageDirection {
    pub fn as_str(&self) -> &str {
        match self {
            MessageDirection::AgentToAmos => "agent_to_amos",
            MessageDirection::AmosToAgent => "amos_to_agent",
            MessageDirection::AmosToUser => "amos_to_user",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "agent_to_amos" => Some(MessageDirection::AgentToAmos),
            "amos_to_agent" => Some(MessageDirection::AmosToAgent),
            "amos_to_user" => Some(MessageDirection::AmosToUser),
            _ => None,
        }
    }
}

/// Classification of a task message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Progress or status update
    StatusUpdate,
    /// Agent needs input or clarification
    Question,
    /// Final result delivery
    Result,
    /// Error report
    Error,
    /// Incremental progress (percentage, milestones)
    Progress,
    /// Agent requests approval before proceeding
    ApprovalRequest,
}

impl MessageType {
    pub fn as_str(&self) -> &str {
        match self {
            MessageType::StatusUpdate => "status_update",
            MessageType::Question => "question",
            MessageType::Result => "result",
            MessageType::Error => "error",
            MessageType::Progress => "progress",
            MessageType::ApprovalRequest => "approval_request",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "status_update" => Some(MessageType::StatusUpdate),
            "question" => Some(MessageType::Question),
            "result" => Some(MessageType::Result),
            "error" => Some(MessageType::Error),
            "progress" => Some(MessageType::Progress),
            "approval_request" => Some(MessageType::ApprovalRequest),
            _ => None,
        }
    }
}

/// A message in the task message bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub id: Uuid,
    pub task_id: Uuid,
    pub direction: MessageDirection,
    pub message_type: MessageType,
    pub content: JsonValue,
    pub acknowledged: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// ── Real-time notification ──────────────────────────────────────────────

/// Lightweight event broadcast when a task message is written to the DB.
/// The main agent loop subscribes to this channel and checks for pending
/// messages when it gets a notification.
#[derive(Debug, Clone)]
pub struct TaskNotification {
    pub task_id: Uuid,
    pub message_type: MessageType,
    pub summary: String,
}

// ── TaskQueue (the harness infrastructure) ──────────────────────────────

/// The task queue manages task lifecycle and the message bus.
/// This is harness-level infrastructure that AMOS uses via tools.
pub struct TaskQueue {
    db_pool: PgPool,
    /// Broadcast channel for real-time task notifications.
    /// The agent loop subscribes; task workers publish.
    notify_tx: broadcast::Sender<TaskNotification>,
}

impl TaskQueue {
    pub fn new(db_pool: PgPool) -> Self {
        let (notify_tx, _) = broadcast::channel(256);
        Self { db_pool, notify_tx }
    }

    /// Subscribe to real-time task notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<TaskNotification> {
        self.notify_tx.subscribe()
    }

    /// Get a reference to the notification sender (for sub-agents to use).
    pub fn notify_sender(&self) -> broadcast::Sender<TaskNotification> {
        self.notify_tx.clone()
    }

    // ── Task CRUD ───────────────────────────────────────────────────

    /// Create a new task.
    pub async fn create_task(&self, params: CreateTaskParams) -> Result<Task> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let context = params.context.unwrap_or(serde_json::json!({}));
        let priority = params.priority.unwrap_or(5);
        let reward_tokens = params.reward_tokens.unwrap_or(0);

        sqlx::query(
            r#"INSERT INTO tasks
               (id, title, description, context, category, task_type, priority,
                status, session_id, parent_task_id, reward_tokens, deadline_at,
                created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
        )
        .bind(id)
        .bind(&params.title)
        .bind(&params.description)
        .bind(&context)
        .bind(params.category.as_str())
        .bind(&params.task_type)
        .bind(priority)
        .bind(TaskStatus::Pending.as_str())
        .bind(params.session_id)
        .bind(params.parent_task_id)
        .bind(reward_tokens)
        .bind(params.deadline_at)
        .bind(now)
        .bind(now)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to create task: {e}")))?;

        Ok(Task {
            id,
            title: params.title,
            description: params.description,
            context,
            category: params.category,
            task_type: params.task_type,
            priority,
            status: TaskStatus::Pending,
            assigned_to: None,
            session_id: params.session_id,
            parent_task_id: params.parent_task_id,
            result: None,
            error_message: None,
            reward_tokens,
            reward_claimed: false,
            deadline_at: params.deadline_at,
            created_at: now,
            updated_at: now,
            assigned_at: None,
            started_at: None,
            completed_at: None,
        })
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: Uuid) -> Result<Task> {
        let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to fetch task: {e}")))?
            .ok_or_else(|| AmosError::NotFound {
                entity: "Task".to_string(),
                id: task_id.to_string(),
            })?;

        Ok(row.into_task())
    }

    /// List tasks, optionally filtered by session and/or status.
    pub async fn list_tasks(
        &self,
        session_id: Option<Uuid>,
        status_filter: Option<TaskStatus>,
        limit: Option<i64>,
    ) -> Result<Vec<Task>> {
        // Build query dynamically
        let mut sql = String::from("SELECT * FROM tasks WHERE 1=1");
        let mut param_idx = 1u32;

        if session_id.is_some() {
            sql.push_str(&format!(" AND session_id = ${param_idx}"));
            param_idx += 1;
        }
        if status_filter.is_some() {
            sql.push_str(&format!(" AND status = ${param_idx}"));
            param_idx += 1;
        }

        sql.push_str(" ORDER BY priority ASC, created_at DESC");

        if limit.is_some() {
            sql.push_str(&format!(" LIMIT ${param_idx}"));
            // param_idx += 1;
        }

        // We need to build the query with dynamic binds.
        // sqlx requires compile-time checking, so we use query_as with raw SQL.
        let mut query = sqlx::query_as::<_, TaskRow>(&sql);

        if let Some(sid) = session_id {
            query = query.bind(sid);
        }
        if let Some(sf) = status_filter {
            query = query.bind(sf.as_str().to_string());
        }
        if let Some(lim) = limit {
            query = query.bind(lim);
        }

        let rows = query
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to list tasks: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_task()).collect())
    }

    /// Transition a task to a new status with optional metadata updates.
    pub async fn update_task_status(
        &self,
        task_id: Uuid,
        new_status: TaskStatus,
        result: Option<JsonValue>,
        error_message: Option<String>,
    ) -> Result<Task> {
        let now = Utc::now();

        // Set timestamp columns based on the transition
        let (assigned_at_clause, started_at_clause, completed_at_clause) = match new_status {
            TaskStatus::Assigned => ("assigned_at = $6,", "", ""),
            TaskStatus::Running => ("", "started_at = $6,", ""),
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                ("", "", "completed_at = $6,")
            }
            _ => ("", "", ""),
        };

        let sql = format!(
            r#"UPDATE tasks
               SET status = $1,
                   result = COALESCE($2, result),
                   error_message = COALESCE($3, error_message),
                   updated_at = $4,
                   {assigned_at_clause}
                   {started_at_clause}
                   {completed_at_clause}
                   id = id
               WHERE id = $5
               RETURNING *"#,
        );

        // We always bind 6 params; for unused timestamp clauses the value is ignored
        let row = sqlx::query_as::<_, TaskRow>(&sql)
            .bind(new_status.as_str())
            .bind(&result)
            .bind(&error_message)
            .bind(now)
            .bind(task_id)
            .bind(now) // $6 -- used by whichever timestamp clause is active
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to update task: {e}")))?
            .ok_or_else(|| AmosError::NotFound {
                entity: "Task".to_string(),
                id: task_id.to_string(),
            })?;

        Ok(row.into_task())
    }

    /// Cancel a task (if not already terminal).
    pub async fn cancel_task(&self, task_id: Uuid) -> Result<Task> {
        self.update_task_status(task_id, TaskStatus::Cancelled, None, None)
            .await
    }

    /// Get active (non-terminal) tasks for a session.
    pub async fn active_tasks_for_session(&self, session_id: Uuid) -> Result<Vec<Task>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            r#"SELECT * FROM tasks
               WHERE session_id = $1
                 AND status NOT IN ('completed', 'failed', 'cancelled')
               ORDER BY priority ASC, created_at ASC
               LIMIT 200"#,
        )
        .bind(session_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to fetch active tasks: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_task()).collect())
    }

    // ── Task message bus ────────────────────────────────────────────

    /// Post a message to the task message bus.
    pub async fn post_message(
        &self,
        task_id: Uuid,
        direction: MessageDirection,
        message_type: MessageType,
        content: JsonValue,
    ) -> Result<TaskMessage> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO task_messages (id, task_id, direction, message_type, content, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(id)
        .bind(task_id)
        .bind(direction.as_str())
        .bind(message_type.as_str())
        .bind(&content)
        .bind(now)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to post task message: {e}")))?;

        // Broadcast real-time notification
        let summary = content
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("Task update")
            .to_string();

        let _ = self.notify_tx.send(TaskNotification {
            task_id,
            message_type,
            summary,
        });

        Ok(TaskMessage {
            id,
            task_id,
            direction,
            message_type,
            content,
            acknowledged: false,
            acknowledged_at: None,
            created_at: now,
        })
    }

    /// Get unacknowledged messages for tasks in a session.
    /// This is what the agent loop calls to check for pending updates.
    pub async fn pending_messages_for_session(&self, session_id: Uuid) -> Result<Vec<TaskMessage>> {
        let rows = sqlx::query_as::<_, TaskMessageRow>(
            r#"SELECT tm.*
               FROM task_messages tm
               JOIN tasks t ON t.id = tm.task_id
               WHERE t.session_id = $1
                 AND tm.acknowledged = false
                 AND tm.direction = 'agent_to_amos'
               ORDER BY tm.created_at ASC
               LIMIT 500"#,
        )
        .bind(session_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to fetch messages: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_message()).collect())
    }

    /// Acknowledge messages (mark as read).
    pub async fn acknowledge_messages(&self, message_ids: &[Uuid]) -> Result<()> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let now = Utc::now();
        sqlx::query(
            r#"UPDATE task_messages
               SET acknowledged = true, acknowledged_at = $1
               WHERE id = ANY($2)"#,
        )
        .bind(now)
        .bind(message_ids)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to acknowledge messages: {e}")))?;

        Ok(())
    }

    /// Get messages for a specific task.
    pub async fn messages_for_task(&self, task_id: Uuid) -> Result<Vec<TaskMessage>> {
        let rows = sqlx::query_as::<_, TaskMessageRow>(
            r#"SELECT * FROM task_messages
               WHERE task_id = $1
               ORDER BY created_at ASC
               LIMIT 500"#,
        )
        .bind(task_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to fetch task messages: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_message()).collect())
    }

    // ── Bounty helpers (external tasks) ─────────────────────────────

    /// List available bounties that external agents can claim.
    pub async fn available_bounties(&self, limit: Option<i64>) -> Result<Vec<Task>> {
        let lim = limit.unwrap_or(50);
        let rows = sqlx::query_as::<_, TaskRow>(
            r#"SELECT * FROM tasks
               WHERE category = 'external' AND status = 'pending'
               ORDER BY priority ASC, created_at ASC
               LIMIT $1"#,
        )
        .bind(lim)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to list bounties: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_task()).collect())
    }

    /// Assign a bounty to an external agent.
    pub async fn claim_bounty(&self, task_id: Uuid, agent_id: Uuid) -> Result<Task> {
        let now = Utc::now();
        let row = sqlx::query_as::<_, TaskRow>(
            r#"UPDATE tasks
               SET status = 'assigned',
                   assigned_to = $1,
                   assigned_at = $2,
                   updated_at = $2
               WHERE id = $3
                 AND category = 'external'
                 AND status = 'pending'
               RETURNING *"#,
        )
        .bind(agent_id)
        .bind(now)
        .bind(task_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to claim bounty: {e}")))?
        .ok_or_else(|| {
            AmosError::Validation(
                "Bounty is not available (already claimed or not external/pending)".to_string(),
            )
        })?;

        Ok(row.into_task())
    }
}

// ── sqlx row mapping ────────────────────────────────────────────────────

/// Internal row type for sqlx deserialization.
#[derive(sqlx::FromRow)]
struct TaskRow {
    id: Uuid,
    title: String,
    description: Option<String>,
    context: JsonValue,
    category: String,
    task_type: Option<String>,
    priority: i32,
    status: String,
    assigned_to: Option<Uuid>,
    session_id: Option<Uuid>,
    parent_task_id: Option<Uuid>,
    result: Option<JsonValue>,
    error_message: Option<String>,
    reward_tokens: i64,
    reward_claimed: bool,
    deadline_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    assigned_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

impl TaskRow {
    fn into_task(self) -> Task {
        Task {
            id: self.id,
            title: self.title,
            description: self.description,
            context: self.context,
            category: TaskCategory::from_str(&self.category).unwrap_or(TaskCategory::Internal),
            task_type: self.task_type,
            priority: self.priority,
            status: TaskStatus::from_str(&self.status).unwrap_or(TaskStatus::Pending),
            assigned_to: self.assigned_to,
            session_id: self.session_id,
            parent_task_id: self.parent_task_id,
            result: self.result,
            error_message: self.error_message,
            reward_tokens: self.reward_tokens,
            reward_claimed: self.reward_claimed,
            deadline_at: self.deadline_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            assigned_at: self.assigned_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TaskMessageRow {
    id: Uuid,
    task_id: Uuid,
    direction: String,
    message_type: String,
    content: JsonValue,
    acknowledged: bool,
    acknowledged_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl TaskMessageRow {
    fn into_message(self) -> TaskMessage {
        TaskMessage {
            id: self.id,
            task_id: self.task_id,
            direction: MessageDirection::from_str(&self.direction)
                .unwrap_or(MessageDirection::AgentToAmos),
            message_type: MessageType::from_str(&self.message_type)
                .unwrap_or(MessageType::StatusUpdate),
            content: self.content,
            acknowledged: self.acknowledged,
            acknowledged_at: self.acknowledged_at,
            created_at: self.created_at,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── TaskCategory ────────────────────────────────────────────────

    #[test]
    fn task_category_roundtrip() {
        assert_eq!(
            TaskCategory::from_str("internal"),
            Some(TaskCategory::Internal)
        );
        assert_eq!(
            TaskCategory::from_str("external"),
            Some(TaskCategory::External)
        );
        assert_eq!(TaskCategory::from_str("bogus"), None);
        assert_eq!(TaskCategory::Internal.as_str(), "internal");
        assert_eq!(TaskCategory::External.as_str(), "external");
    }

    #[test]
    fn task_category_display() {
        assert_eq!(format!("{}", TaskCategory::Internal), "internal");
        assert_eq!(format!("{}", TaskCategory::External), "external");
    }

    #[test]
    fn task_category_serde() {
        let serialized = serde_json::to_string(&TaskCategory::External).unwrap();
        assert_eq!(serialized, "\"external\"");
        let deserialized: TaskCategory = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, TaskCategory::External);
    }

    // ── TaskStatus ──────────────────────────────────────────────────

    #[test]
    fn task_status_roundtrip() {
        let statuses = [
            ("pending", TaskStatus::Pending),
            ("assigned", TaskStatus::Assigned),
            ("running", TaskStatus::Running),
            ("completed", TaskStatus::Completed),
            ("failed", TaskStatus::Failed),
            ("cancelled", TaskStatus::Cancelled),
        ];
        for (s, expected) in &statuses {
            assert_eq!(TaskStatus::from_str(s), Some(*expected), "from_str({s})");
            assert_eq!(expected.as_str(), *s, "as_str({expected:?})");
        }
        assert_eq!(TaskStatus::from_str("unknown"), None);
    }

    #[test]
    fn task_status_is_terminal() {
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::Assigned.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
    }

    #[test]
    fn task_status_serde() {
        let serialized = serde_json::to_string(&TaskStatus::Running).unwrap();
        assert_eq!(serialized, "\"running\"");
        let deserialized: TaskStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, TaskStatus::Running);
    }

    // ── MessageDirection / MessageType ──────────────────────────────

    #[test]
    fn message_direction_roundtrip() {
        assert_eq!(
            MessageDirection::from_str("agent_to_amos"),
            Some(MessageDirection::AgentToAmos)
        );
        assert_eq!(
            MessageDirection::from_str("amos_to_agent"),
            Some(MessageDirection::AmosToAgent)
        );
        assert_eq!(
            MessageDirection::from_str("amos_to_user"),
            Some(MessageDirection::AmosToUser)
        );
        assert_eq!(MessageDirection::from_str("bad"), None);
    }

    #[test]
    fn message_type_roundtrip() {
        let types = [
            ("status_update", MessageType::StatusUpdate),
            ("question", MessageType::Question),
            ("result", MessageType::Result),
            ("error", MessageType::Error),
            ("progress", MessageType::Progress),
            ("approval_request", MessageType::ApprovalRequest),
        ];
        for (s, expected) in &types {
            assert_eq!(MessageType::from_str(s), Some(*expected));
            assert_eq!(expected.as_str(), *s);
        }
    }

    // ── Task serde ──────────────────────────────────────────────────

    #[test]
    fn task_serde_roundtrip() {
        let task = Task {
            id: Uuid::new_v4(),
            title: "Research competitors".to_string(),
            description: Some("Find top 5 competitors and their pricing".to_string()),
            context: json!({"industry": "SaaS"}),
            category: TaskCategory::Internal,
            task_type: Some("research".to_string()),
            priority: 3,
            status: TaskStatus::Running,
            assigned_to: None,
            session_id: Some(Uuid::new_v4()),
            parent_task_id: None,
            result: None,
            error_message: None,
            reward_tokens: 0,
            reward_claimed: false,
            deadline_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assigned_at: None,
            started_at: Some(Utc::now()),
            completed_at: None,
        };

        let serialized = serde_json::to_string(&task).unwrap();
        let deserialized: Task = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.title, "Research competitors");
        assert_eq!(deserialized.category, TaskCategory::Internal);
        assert_eq!(deserialized.status, TaskStatus::Running);
        assert_eq!(deserialized.priority, 3);
    }

    #[test]
    fn task_message_serde_roundtrip() {
        let msg = TaskMessage {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            direction: MessageDirection::AgentToAmos,
            message_type: MessageType::Progress,
            content: json!({"text": "Found 3 of 5 competitors", "percent": 60}),
            acknowledged: false,
            acknowledged_at: None,
            created_at: Utc::now(),
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: TaskMessage = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.direction, MessageDirection::AgentToAmos);
        assert_eq!(deserialized.message_type, MessageType::Progress);
        assert!(!deserialized.acknowledged);
    }

    // ── CreateTaskParams ────────────────────────────────────────────

    #[test]
    fn create_task_params_minimal() {
        let params = CreateTaskParams {
            title: "Simple task".to_string(),
            description: None,
            context: None,
            category: TaskCategory::Internal,
            task_type: None,
            priority: None,
            session_id: None,
            parent_task_id: None,
            reward_tokens: None,
            deadline_at: None,
        };

        assert_eq!(params.title, "Simple task");
        assert_eq!(params.category, TaskCategory::Internal);
    }

    // ── TaskNotification ────────────────────────────────────────────

    #[test]
    fn task_notification_broadcast() {
        let (tx, mut rx) = broadcast::channel::<TaskNotification>(16);
        let task_id = Uuid::new_v4();

        tx.send(TaskNotification {
            task_id,
            message_type: MessageType::Result,
            summary: "Task complete".to_string(),
        })
        .unwrap();

        let notif = rx.try_recv().unwrap();
        assert_eq!(notif.task_id, task_id);
        assert_eq!(notif.message_type, MessageType::Result);
        assert_eq!(notif.summary, "Task complete");
    }
}
