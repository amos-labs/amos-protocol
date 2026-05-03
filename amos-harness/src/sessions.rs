//! Session and message persistence for multi-turn conversations.
//!
//! Uses the existing `sessions` and `messages` tables to store conversation
//! history so that:
//! 1. Multi-turn conversations work (each new message continues the session)
//! 2. Page refresh restores the conversation from the server
//! 3. The sidebar can list recent conversations

use amos_core::{
    types::{ContentBlock, Message, Role},
    Result,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// A persisted chat session (maps to the `sessions` table).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Option<String>,
    pub title: Option<String>,
    pub model_id: String,
    pub status: String,
    pub message_count: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

/// Compact session info for sidebar listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: Uuid,
    pub title: Option<String>,
    pub message_count: i32,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Internal row type for the subset query (derives FromRow).
#[derive(Debug, Clone, sqlx::FromRow)]
struct SessionSummaryRow {
    pub id: Uuid,
    pub title: Option<String>,
    pub message_count: i32,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// A persisted message row (maps to the `messages` table).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: serde_json::Value,
    pub model_id: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub tool_use_id: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub sequence_number: i32,
}

// ═══════════════════════════════════════════════════════════════════════════
// SESSION CRUD
// ═══════════════════════════════════════════════════════════════════════════

/// Create a new session, optionally with a title derived from the first user message.
pub async fn create_session(pool: &PgPool, title: Option<&str>) -> Result<Session> {
    let row = sqlx::query_as::<_, Session>(
        r#"
        INSERT INTO sessions (title)
        VALUES ($1)
        RETURNING *
        "#,
    )
    .bind(title)
    .fetch_one(pool)
    .await
    .map_err(|e| amos_core::AmosError::Internal(format!("create_session: {e}")))?;

    Ok(row)
}

/// Get a session by ID.
pub async fn get_session(pool: &PgPool, session_id: Uuid) -> Result<Option<Session>> {
    let row = sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE id = $1")
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("get_session: {e}")))?;

    Ok(row)
}

/// List recent sessions, ordered by last activity (most recent first).
pub async fn list_sessions(pool: &PgPool, limit: i64) -> Result<Vec<SessionSummary>> {
    let rows = sqlx::query_as::<_, SessionSummaryRow>(
        r#"
        SELECT id, title, message_count, last_activity_at, created_at
        FROM sessions
        WHERE status = 'active'
        ORDER BY last_activity_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| amos_core::AmosError::Internal(format!("list_sessions: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| SessionSummary {
            id: r.id,
            title: r.title,
            message_count: r.message_count,
            last_activity_at: r.last_activity_at,
            created_at: r.created_at,
        })
        .collect())
}

/// Delete a session (cascades to messages).
pub async fn delete_session(pool: &PgPool, session_id: Uuid) -> Result<bool> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("delete_session: {e}")))?;

    Ok(result.rows_affected() > 0)
}

/// Update the session title (auto-generated from the first message).
pub async fn update_session_title(pool: &PgPool, session_id: Uuid, title: &str) -> Result<()> {
    sqlx::query("UPDATE sessions SET title = $1, updated_at = NOW() WHERE id = $2")
        .bind(title)
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("update_session_title: {e}")))?;

    Ok(())
}

/// Touch the session's last_activity_at and update message count / token stats.
/// Stamp the most recent assistant message in a session with model id +
/// per-call token usage. The agent emits these in its `agent_end` SSE event
/// after the LLM stream finishes; we capture them in `agent_proxy` and call
/// here to backfill the row that `save_messages` just inserted.
///
/// Why post-insert update instead of binding at insert time: `save_messages`
/// takes a `&[Message]` from `amos_core::types`, which doesn't carry token
/// fields (those are streaming-only signals). Adding them would ripple
/// through every Message consumer. A targeted UPDATE on the most recent
/// assistant row is surgical and keeps the Message type clean.
///
/// Without this, every row in `messages` ships with `model_id = NULL` and
/// `input_tokens = output_tokens = 0` — confirmed in production
/// 2026-05-02. Per-message audit trail, per-model usage breakdowns, and
/// the activity-counter feeder all stayed empty.
pub async fn stamp_last_assistant_message_usage(
    pool: &PgPool,
    session_id: Uuid,
    model_id: Option<&str>,
    input_tokens: i64,
    output_tokens: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE messages
        SET model_id = $2, input_tokens = $3, output_tokens = $4
        WHERE id = (
            SELECT id FROM messages
            WHERE session_id = $1 AND role = 'assistant'
            ORDER BY sequence_number DESC
            LIMIT 1
        )
        "#,
    )
    .bind(session_id)
    .bind(model_id)
    .bind(input_tokens)
    .bind(output_tokens)
    .execute(pool)
    .await
    .map_err(|e| {
        amos_core::AmosError::Internal(format!("stamp_last_assistant_message_usage: {e}"))
    })?;
    Ok(())
}

pub async fn touch_session(
    pool: &PgPool,
    session_id: Uuid,
    additional_messages: i32,
    input_tokens: i64,
    output_tokens: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE sessions
        SET message_count = message_count + $2,
            total_input_tokens = total_input_tokens + $3,
            total_output_tokens = total_output_tokens + $4,
            last_activity_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(additional_messages)
    .bind(input_tokens)
    .bind(output_tokens)
    .execute(pool)
    .await
    .map_err(|e| amos_core::AmosError::Internal(format!("touch_session: {e}")))?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// MESSAGE PERSISTENCE
// ═══════════════════════════════════════════════════════════════════════════

/// Save a batch of `Message` objects to the database for a session.
///
/// `start_seq` is the sequence number to begin numbering from (typically the
/// count of existing messages in the session).
pub async fn save_messages(
    pool: &PgPool,
    session_id: Uuid,
    messages: &[Message],
    start_seq: i32,
) -> Result<()> {
    // Use a transaction for atomicity
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("save_messages begin: {e}")))?;

    for (i, msg) in messages.iter().enumerate() {
        let seq = start_seq + i as i32;
        let role_str = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        };

        // Serialize the content blocks as JSONB
        let content_json = serde_json::to_value(&msg.content)
            .map_err(|e| amos_core::AmosError::Internal(format!("serialize content: {e}")))?;

        sqlx::query(
            r#"
            INSERT INTO messages (session_id, role, content, tool_use_id, sequence_number, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(session_id)
        .bind(role_str)
        .bind(&content_json)
        .bind(&msg.tool_use_id)
        .bind(seq)
        .bind(msg.timestamp)
        .execute(&mut *tx)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("save_messages insert: {e}")))?;
    }

    tx.commit()
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("save_messages commit: {e}")))?;

    Ok(())
}

/// Load all messages for a session, ordered by sequence number.
/// Returns the domain `Message` type ready for feeding into the AgentLoop.
pub async fn load_messages(pool: &PgPool, session_id: Uuid) -> Result<Vec<Message>> {
    let rows = sqlx::query_as::<_, MessageRow>(
        r#"
        SELECT * FROM messages
        WHERE session_id = $1
        ORDER BY sequence_number ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| amos_core::AmosError::Internal(format!("load_messages: {e}")))?;

    let mut messages = Vec::with_capacity(rows.len());
    for row in rows {
        let role = match row.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            "tool" => Role::Tool,
            other => {
                tracing::warn!("Unknown role in DB: {other}, defaulting to User");
                Role::User
            }
        };

        let content: Vec<ContentBlock> = serde_json::from_value(row.content.clone())
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to deserialize content blocks: {e}, using raw text");
                // Fallback: treat the whole value as a text block
                vec![ContentBlock::Text {
                    text: row.content.to_string(),
                }]
            });

        messages.push(Message {
            role,
            content,
            tool_use_id: row.tool_use_id,
            timestamp: row.created_at,
        });
    }

    Ok(messages)
}

/// Get the current message count for a session (used as start_seq for new saves).
pub async fn message_count(pool: &PgPool, session_id: Uuid) -> Result<i32> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE session_id = $1")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("message_count: {e}")))?;

    Ok(count.0 as i32)
}
