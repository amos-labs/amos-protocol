//! Automation engine — evaluates triggers, runs actions, manages cron loop.

use super::{ActionType, Automation, AutomationRun, TriggerEvent, TriggerType};
use crate::schema::SchemaEngine;
use crate::ses::{EmailMessage, SesClient};
use crate::task_queue::{CreateTaskParams, TaskCategory, TaskQueue};
use amos_core::{AmosError, Result};
use chrono::Utc;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Core automation engine.
pub struct AutomationEngine {
    db_pool: PgPool,
    task_queue: Arc<TaskQueue>,
    http_client: reqwest::Client,
    /// Optional SES client for email-channel notifications.
    email_client: Option<Arc<SesClient>>,
    /// Sender for self-emitted events (used by action chaining). Populated
    /// by `create_event_channel()`. `try_send` is non-async to avoid an
    /// async self-reference cycle with `execute_action`.
    event_tx: Arc<parking_lot::RwLock<Option<mpsc::Sender<TriggerEvent>>>>,
}

impl AutomationEngine {
    pub fn new(db_pool: PgPool, task_queue: Arc<TaskQueue>, http_client: reqwest::Client) -> Self {
        Self {
            db_pool,
            task_queue,
            http_client,
            email_client: None,
            event_tx: Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    /// Attach an SES email client. When set, `SendNotification` actions with
    /// `channel: "email"` are delivered via SES instead of being logged.
    pub fn with_email_client(mut self, email_client: Option<Arc<SesClient>>) -> Self {
        self.email_client = email_client;
        self
    }

    /// Share the parent engine's self-emit event sender so cloned sub-engines
    /// can fire chain events back through the same channel.
    pub fn with_event_tx(
        mut self,
        event_tx: Arc<parking_lot::RwLock<Option<mpsc::Sender<TriggerEvent>>>>,
    ) -> Self {
        self.event_tx = event_tx;
        self
    }

    /// Accessor for the shared self-emit sender holder.
    pub fn event_tx_handle(&self) -> Arc<parking_lot::RwLock<Option<mpsc::Sender<TriggerEvent>>>> {
        self.event_tx.clone()
    }

    /// Create an event channel and spawn a background task that drains it.
    /// Returns the sender that `SchemaEngine` can use to fire events without
    /// creating an async type cycle. Also stores a clone of the sender
    /// internally so `execute_action` can emit chain events back into the
    /// same loop.
    pub fn create_event_channel(self: &Arc<Self>) -> mpsc::Sender<TriggerEvent> {
        let (tx, mut rx) = mpsc::channel::<TriggerEvent>(1024);
        *self.event_tx.write() = Some(tx.clone());

        let engine = self.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                engine.fire_event(event).await;
            }
        });
        tx
    }

    // ─── Event firing ───────────────────────────────────────────────────

    /// Main entry point: find matching automations and execute them.
    pub async fn fire_event(&self, event: TriggerEvent) {
        let automations = match self.find_matching_automations(&event).await {
            Ok(list) => list,
            Err(e) => {
                tracing::error!(
                    "Failed to query automations for event {:?}: {}",
                    event.event_type,
                    e
                );
                return;
            }
        };

        for automation in automations {
            let trigger_data = event.data.clone();
            let db_pool = self.db_pool.clone();
            let task_queue = self.task_queue.clone();
            let http_client = self.http_client.clone();
            let email_client = self.email_client.clone();
            let event_tx = self.event_tx.clone();

            tokio::spawn(async move {
                let engine = AutomationEngine::new(db_pool, task_queue, http_client)
                    .with_email_client(email_client)
                    .with_event_tx(event_tx);
                engine.execute_action(&automation, trigger_data).await;
            });
        }
    }

    /// Find automations that match the given event.
    async fn find_matching_automations(&self, event: &TriggerEvent) -> Result<Vec<Automation>> {
        let trigger_str = event.event_type.as_str();

        let rows = sqlx::query(
            r#"SELECT id, name, description, enabled, trigger_type, trigger_config,
                      condition, action_type, action_config, created_at, updated_at
               FROM automations
               WHERE trigger_type = $1 AND enabled = true"#,
        )
        .bind(trigger_str)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to query automations: {}", e)))?;

        let mut matching = Vec::new();
        for row in &rows {
            let automation = automation_from_row(row)?;

            // For record triggers, check collection match
            if matches!(
                event.event_type,
                TriggerType::RecordCreated
                    | TriggerType::RecordUpdated
                    | TriggerType::RecordDeleted
            ) {
                let config_collection = automation
                    .trigger_config
                    .get("collection")
                    .and_then(|v| v.as_str());
                if let (Some(config_col), Some(event_col)) = (config_collection, &event.collection)
                {
                    if config_col != event_col {
                        continue;
                    }
                }
            }

            // For automation_completed chaining: match on the source
            // automation_id in trigger_config. If not specified, match any
            // completed automation (useful for a catch-all "audit every run").
            if matches!(event.event_type, TriggerType::AutomationCompleted) {
                if let Some(required) = automation
                    .trigger_config
                    .get("automation_id")
                    .and_then(|v| v.as_str())
                {
                    let source = event
                        .data
                        .get("source_automation_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if required != source {
                        continue;
                    }
                }
            }

            // Evaluate optional condition (simple JSONB field match)
            if let Some(condition) = &automation.condition {
                if !evaluate_condition(condition, &event.data) {
                    continue;
                }
            }

            matching.push(automation);
        }

        Ok(matching)
    }

    /// Execute the action for a matched automation.
    async fn execute_action(&self, automation: &Automation, trigger_data: JsonValue) {
        let start = Instant::now();

        // Apply {{trigger.field}} substitution to the whole action_config up
        // front so every action type gets dynamic data bindings without
        // each handler needing to remember to call substitute_template.
        let resolved_config = substitute_template(&automation.action_config, &trigger_data);
        let mut resolved = automation.clone();
        resolved.action_config = resolved_config;
        let automation = &resolved;

        let result = match automation.action_type {
            ActionType::CreateRecord => self.action_create_record(automation, &trigger_data).await,
            ActionType::UpdateRecord => self.action_update_record(automation, &trigger_data).await,
            ActionType::CallWebhook => self.action_call_webhook(automation, &trigger_data).await,
            ActionType::RunAgentTask => self.action_run_agent_task(automation, &trigger_data).await,
            ActionType::SendNotification => {
                self.action_send_notification(automation, &trigger_data)
                    .await
            }
            ActionType::CreateBounty => self.action_create_bounty(automation, &trigger_data).await,
        };

        let duration_ms = start.elapsed().as_millis() as i32;

        match result {
            Ok(result_data) => {
                tracing::info!(
                    automation_id = %automation.id,
                    automation_name = %automation.name,
                    duration_ms,
                    "Automation executed successfully"
                );
                let _ = self
                    .log_run(
                        automation.id,
                        &trigger_data,
                        "success",
                        Some(result_data.clone()),
                        None,
                        duration_ms,
                    )
                    .await;

                // Fire automation_completed event so any chained automations
                // can run. Cap chain depth at 5 to prevent infinite loops.
                let depth = trigger_data
                    .get("_chain_depth")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if depth < 5 {
                    // Emit chain event via the shared sender. try_send is
                    // non-async, breaking the self-reference cycle that a
                    // direct call to fire_event would create.
                    if let Some(tx) = self.event_tx.read().clone() {
                        let chain_event = TriggerEvent {
                            event_type: TriggerType::AutomationCompleted,
                            collection: None,
                            record_id: None,
                            data: json!({
                                "source_automation_id": automation.id.to_string(),
                                "source_automation_name": automation.name,
                                "result": result_data,
                                "trigger_data": trigger_data,
                                "_chain_depth": depth + 1,
                            }),
                        };
                        if let Err(e) = tx.try_send(chain_event) {
                            tracing::warn!(
                                automation_id = %automation.id,
                                error = %e,
                                "Failed to emit automation_completed chain event"
                            );
                        }
                    } else {
                        tracing::debug!(
                            "Skipping chain event emit: engine was not started via create_event_channel"
                        );
                    }
                } else {
                    tracing::warn!(
                        automation_id = %automation.id,
                        "Chain depth limit (5) reached — not firing automation_completed event"
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    automation_id = %automation.id,
                    automation_name = %automation.name,
                    error = %e,
                    "Automation execution failed"
                );
                let err_str = e.to_string();
                let _ = self
                    .log_run(
                        automation.id,
                        &trigger_data,
                        "error",
                        None,
                        Some(err_str.clone()),
                        duration_ms,
                    )
                    .await;

                // Outbound webhook failures are retried with exponential backoff.
                // Other action types (create_record, send_notification, etc.) are
                // not retried — they either have no idempotency guarantees
                // (email) or are local DB ops that should not fail transiently.
                if matches!(automation.action_type, ActionType::CallWebhook) {
                    if let Err(enqueue_err) = self
                        .enqueue_retry(automation.id, "call_webhook", &trigger_data, &err_str, 1)
                        .await
                    {
                        tracing::error!(
                            automation_id = %automation.id,
                            error = %enqueue_err,
                            "Failed to enqueue webhook retry"
                        );
                    }
                }
            }
        }
    }

    // ─── Actions ────────────────────────────────────────────────────────

    async fn action_create_record(
        &self,
        automation: &Automation,
        _trigger_data: &JsonValue,
    ) -> Result<JsonValue> {
        let collection = automation
            .action_config
            .get("collection")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AmosError::Validation(
                    "create_record action requires 'collection' in action_config".to_string(),
                )
            })?;

        // action_config is already template-substituted by execute_action's top-level call.
        let data = automation
            .action_config
            .get("data_template")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let engine = SchemaEngine::new(self.db_pool.clone());
        let record = engine.create_record(collection, data).await?;

        Ok(json!({
            "action": "create_record",
            "record_id": record.id.to_string(),
            "collection": collection,
        }))
    }

    async fn action_update_record(
        &self,
        automation: &Automation,
        trigger_data: &JsonValue,
    ) -> Result<JsonValue> {
        let record_id_str = automation
            .action_config
            .get("record_id")
            .and_then(|v| v.as_str())
            // Allow template substitution for record_id
            .or_else(|| trigger_data.get("record_id").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                AmosError::Validation(
                    "update_record action requires 'record_id' in action_config or trigger data"
                        .to_string(),
                )
            })?;

        let record_id = Uuid::parse_str(record_id_str)
            .map_err(|_| AmosError::Validation(format!("Invalid UUID: {}", record_id_str)))?;

        // action_config is already template-substituted by execute_action's top-level call.
        let data = automation
            .action_config
            .get("data_template")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let engine = SchemaEngine::new(self.db_pool.clone());
        let record = engine.update_record(record_id, data).await?;

        Ok(json!({
            "action": "update_record",
            "record_id": record.id.to_string(),
        }))
    }

    async fn action_call_webhook(
        &self,
        automation: &Automation,
        trigger_data: &JsonValue,
    ) -> Result<JsonValue> {
        let url = automation
            .action_config
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AmosError::Validation(
                    "call_webhook action requires 'url' in action_config".to_string(),
                )
            })?;

        let method = automation
            .action_config
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST");

        let mut request = match method.to_uppercase().as_str() {
            "GET" => self.http_client.get(url),
            "PUT" => self.http_client.put(url),
            "PATCH" => self.http_client.patch(url),
            "DELETE" => self.http_client.delete(url),
            _ => self.http_client.post(url),
        };

        // Add custom headers from config
        if let Some(headers) = automation
            .action_config
            .get("headers")
            .and_then(|v| v.as_object())
        {
            for (key, value) in headers {
                if let Some(val_str) = value.as_str() {
                    request = request.header(key.as_str(), val_str);
                }
            }
        }

        // Compute HMAC-SHA256 signature over the body
        let body = json!({
            "automation_id": automation.id.to_string(),
            "automation_name": automation.name,
            "trigger_data": trigger_data,
        });

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| AmosError::Internal(format!("Failed to serialize webhook body: {}", e)))?;

        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        // Use automation ID as the signing key (customer can verify)
        let mut mac = Hmac::<Sha256>::new_from_slice(automation.id.as_bytes())
            .map_err(|e| AmosError::Internal(format!("HMAC init failed: {}", e)))?;
        mac.update(&body_bytes);
        let signature = hex::encode(mac.finalize().into_bytes());

        let response = request
            .header("Content-Type", "application/json")
            .header("X-AMOS-Signature", &signature)
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| AmosError::Internal(format!("Webhook request failed: {}", e)))?;

        let status = response.status();
        let status_code = status.as_u16();

        // Treat non-2xx as a retryable failure. The execute_action wrapper
        // will enqueue this to the retry queue for exponential-backoff retries.
        if !status.is_success() {
            let body_preview = response.text().await.unwrap_or_default();
            let truncated: String = body_preview.chars().take(500).collect();
            return Err(AmosError::Internal(format!(
                "Webhook returned HTTP {}: {}",
                status_code, truncated
            )));
        }

        Ok(json!({
            "action": "call_webhook",
            "url": url,
            "status": status_code,
        }))
    }

    async fn action_run_agent_task(
        &self,
        automation: &Automation,
        _trigger_data: &JsonValue,
    ) -> Result<JsonValue> {
        let title = automation
            .action_config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(&automation.name);

        let description = automation
            .action_config
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("Automation-triggered task");

        let params = CreateTaskParams {
            title: title.to_string(),
            description: Some(description.to_string()),
            context: None,
            category: TaskCategory::Internal,
            task_type: Some("automation".to_string()),
            priority: None,
            session_id: None,
            parent_task_id: None,
            reward_tokens: None,
            deadline_at: None,
        };

        let task = self.task_queue.create_task(params).await?;

        Ok(json!({
            "action": "run_agent_task",
            "task_id": task.id.to_string(),
            "title": title,
        }))
    }

    /// Create an external bounty from automation config.
    ///
    /// The action_config should contain:
    /// - `title` (string): Bounty title
    /// - `description` (string): What needs to be done
    /// - `reward_tokens` (i64): Token reward for completion
    /// - `context` (object, optional): Additional context (e.g., tool name, content payload)
    /// - `deadline_hours` (i64, optional): Hours until deadline (default: 24)
    ///
    /// Trigger data is merged into the bounty context so the executing agent
    /// has access to the record that fired the automation.
    async fn action_create_bounty(
        &self,
        automation: &Automation,
        trigger_data: &JsonValue,
    ) -> Result<JsonValue> {
        let title = automation
            .action_config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(&automation.name);

        let description = automation
            .action_config
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("Automation-created bounty");

        let reward_tokens = automation
            .action_config
            .get("reward_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(50);

        let deadline_hours = automation
            .action_config
            .get("deadline_hours")
            .and_then(|v| v.as_i64())
            .unwrap_or(24);

        // Merge static context from action_config with dynamic trigger data
        let mut context = automation
            .action_config
            .get("context")
            .cloned()
            .unwrap_or(json!({}));

        if let Some(obj) = context.as_object_mut() {
            obj.insert("trigger_data".to_string(), trigger_data.clone());
            obj.insert(
                "automation_id".to_string(),
                json!(automation.id.to_string()),
            );
            obj.insert("automation_name".to_string(), json!(automation.name));
        }

        let deadline_at = Utc::now() + chrono::Duration::hours(deadline_hours);

        let params = CreateTaskParams {
            title: title.to_string(),
            description: Some(description.to_string()),
            context: Some(context.clone()),
            category: TaskCategory::External,
            task_type: Some("bounty".to_string()),
            priority: Some(5),
            session_id: None,
            parent_task_id: None,
            reward_tokens: Some(reward_tokens),
            deadline_at: Some(deadline_at),
        };

        let task = self.task_queue.create_task(params).await?;

        tracing::info!(
            bounty_id = %task.id,
            title = title,
            reward = reward_tokens,
            deadline = %deadline_at,
            "Bounty created from automation"
        );

        Ok(json!({
            "action": "create_bounty",
            "bounty_id": task.id.to_string(),
            "title": title,
            "reward_tokens": reward_tokens,
            "deadline_at": deadline_at.to_rfc3339(),
            "context": context,
        }))
    }

    async fn action_send_notification(
        &self,
        automation: &Automation,
        trigger_data: &JsonValue,
    ) -> Result<JsonValue> {
        let channel = automation
            .action_config
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("canvas");

        let message = automation
            .action_config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Automation triggered");

        match channel {
            "email" => self.send_email_notification(automation, trigger_data).await,
            _ => {
                // Log the notification (can be polled by canvas/frontend)
                tracing::info!(
                    automation_id = %automation.id,
                    channel,
                    message,
                    "Automation notification sent"
                );

                Ok(json!({
                    "action": "send_notification",
                    "channel": channel,
                    "message": message,
                }))
            }
        }
    }

    /// Deliver an email notification via SES.
    ///
    /// `action_config` fields:
    /// - `to` (required): string or array of recipient addresses
    /// - `subject` (required): subject line
    /// - `message` or `text`: plain-text body
    /// - `html`: HTML body (if present, overrides `message` for rich format)
    /// - `cc`, `bcc`: optional
    /// - `from`: optional override (must be SES-verified)
    /// - `reply_to`: optional
    async fn send_email_notification(
        &self,
        automation: &Automation,
        _trigger_data: &JsonValue,
    ) -> Result<JsonValue> {
        let client = self.email_client.as_ref().ok_or_else(|| {
            AmosError::Config(
                "Email channel requires AMOS__EMAIL__FROM_ADDRESS to be configured".to_string(),
            )
        })?;

        let cfg = &automation.action_config;

        let to = extract_address_list(cfg.get("to"));
        if to.is_empty() {
            return Err(AmosError::Validation(
                "Email automation requires 'to' field with at least one recipient".to_string(),
            ));
        }
        let cc = extract_address_list(cfg.get("cc"));
        let bcc = extract_address_list(cfg.get("bcc"));

        let subject = cfg
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AmosError::Validation("Email automation requires 'subject'".to_string())
            })?
            .to_string();

        let text = cfg
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| cfg.get("message").and_then(|v| v.as_str()))
            .map(|s| s.to_string());

        let html = cfg
            .get("html")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let from = cfg
            .get("from")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let reply_to = cfg
            .get("reply_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let msg = EmailMessage {
            to: to.clone(),
            cc,
            bcc,
            subject: subject.clone(),
            text,
            html,
            from,
            reply_to,
        };

        let result = client.send(msg).await?;

        tracing::info!(
            automation_id = %automation.id,
            message_id = %result.message_id,
            to = ?to,
            subject = %subject,
            "Automation email sent"
        );

        Ok(json!({
            "action": "send_notification",
            "channel": "email",
            "to": to,
            "subject": subject,
            "message_id": result.message_id,
        }))
    }

    // ─── Run logging ────────────────────────────────────────────────────

    async fn log_run(
        &self,
        automation_id: Uuid,
        trigger_data: &JsonValue,
        status: &str,
        result: Option<JsonValue>,
        error: Option<String>,
        duration_ms: i32,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO automation_runs (automation_id, trigger_data, status, result, error, duration_ms)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(automation_id)
        .bind(trigger_data)
        .bind(status)
        .bind(&result)
        .bind(&error)
        .bind(duration_ms)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to log automation run: {}", e)))?;

        Ok(())
    }

    // ─── Retry queue ────────────────────────────────────────────────────

    /// Insert a new retry queue entry (attempt=1) or advance an existing one
    /// toward the next attempt / dead_letter.
    async fn enqueue_retry(
        &self,
        automation_id: Uuid,
        action_type: &str,
        trigger_data: &JsonValue,
        error: &str,
        attempt: i32,
    ) -> Result<()> {
        let next_delay = Self::retry_delay_for_attempt(attempt);
        let next_at = Utc::now() + chrono::Duration::from_std(next_delay).unwrap_or_default();

        sqlx::query(
            r#"INSERT INTO automation_retry_queue
                  (automation_id, action_type, trigger_data, attempt, next_attempt_at,
                   last_error, status)
               VALUES ($1, $2, $3, $4, $5, $6, 'pending')"#,
        )
        .bind(automation_id)
        .bind(action_type)
        .bind(trigger_data)
        .bind(attempt)
        .bind(next_at)
        .bind(error)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to enqueue retry: {}", e)))?;

        tracing::info!(
            automation_id = %automation_id,
            attempt,
            next_attempt_at = %next_at,
            "Enqueued webhook retry"
        );
        Ok(())
    }

    /// Exponential backoff: attempt 1 → 30s, 2 → 2min, 3 → 10min, 4+ → capped at 30min.
    fn retry_delay_for_attempt(attempt: i32) -> std::time::Duration {
        let secs: u64 = match attempt {
            1 => 30,
            2 => 120,
            3 => 600,
            _ => 1800,
        };
        std::time::Duration::from_secs(secs)
    }

    /// Pull due retry entries and re-execute their actions. Called every 30s
    /// by the background worker. Successful retries → status='succeeded';
    /// failed retries under max_attempts reschedule; at max_attempts → 'dead_letter'.
    async fn process_retry_queue(&self) {
        // Atomically claim up to 10 due entries (single-instance for now —
        // when we scale to multiple harness replicas we'd use FOR UPDATE SKIP LOCKED).
        let rows = match sqlx::query(
            r#"UPDATE automation_retry_queue
                  SET status = 'in_progress', updated_at = NOW()
                WHERE id IN (
                    SELECT id FROM automation_retry_queue
                    WHERE status = 'pending' AND next_attempt_at <= NOW()
                    ORDER BY next_attempt_at ASC
                    LIMIT 10
                )
                RETURNING id, automation_id, action_type, trigger_data, attempt, max_attempts"#,
        )
        .fetch_all(&self.db_pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to claim retry queue entries: {}", e);
                return;
            }
        };

        for row in rows {
            let retry_id: Uuid = row.get("id");
            let automation_id: Uuid = row.get("automation_id");
            let action_type: String = row.get("action_type");
            let trigger_data: JsonValue = row.get("trigger_data");
            let attempt: i32 = row.get("attempt");
            let max_attempts: i32 = row.get("max_attempts");

            // Reload the automation in case its config changed.
            let automation = match self.get_automation(automation_id).await {
                Ok(a) => a,
                Err(e) => {
                    // Automation was deleted or is unreachable — mark dead_letter.
                    let _ = sqlx::query(
                        "UPDATE automation_retry_queue SET status='dead_letter', last_error=$1,
                            updated_at=NOW() WHERE id=$2",
                    )
                    .bind(format!("Automation unavailable: {}", e))
                    .bind(retry_id)
                    .execute(&self.db_pool)
                    .await;
                    continue;
                }
            };

            if !automation.enabled {
                // Disabled automations should not fire — drop the retry entry quietly.
                let _ = sqlx::query(
                    "UPDATE automation_retry_queue SET status='dead_letter',
                        last_error='Automation disabled', updated_at=NOW() WHERE id=$1",
                )
                .bind(retry_id)
                .execute(&self.db_pool)
                .await;
                continue;
            }

            // Re-execute the action. Only call_webhook is currently retry-eligible.
            let result = if action_type == "call_webhook" {
                self.action_call_webhook(&automation, &trigger_data).await
            } else {
                Err(AmosError::Internal(format!(
                    "Unsupported retry action type: {}",
                    action_type
                )))
            };

            match result {
                Ok(result_data) => {
                    let _ = sqlx::query(
                        "UPDATE automation_retry_queue SET status='succeeded',
                            updated_at=NOW() WHERE id=$1",
                    )
                    .bind(retry_id)
                    .execute(&self.db_pool)
                    .await;
                    let _ = self
                        .log_run(
                            automation_id,
                            &trigger_data,
                            "success",
                            Some(result_data),
                            None,
                            0,
                        )
                        .await;
                    tracing::info!(
                        automation_id = %automation_id,
                        attempt,
                        "Webhook retry succeeded"
                    );
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let _ = self
                        .log_run(
                            automation_id,
                            &trigger_data,
                            "error",
                            None,
                            Some(err_str.clone()),
                            0,
                        )
                        .await;

                    let next_attempt = attempt + 1;
                    if next_attempt > max_attempts {
                        // Give up.
                        let _ = sqlx::query(
                            "UPDATE automation_retry_queue SET status='dead_letter',
                                last_error=$1, updated_at=NOW() WHERE id=$2",
                        )
                        .bind(&err_str)
                        .bind(retry_id)
                        .execute(&self.db_pool)
                        .await;
                        tracing::error!(
                            automation_id = %automation_id,
                            attempt,
                            "Webhook retry exhausted — moved to dead_letter"
                        );
                    } else {
                        // Reschedule.
                        let next_delay = Self::retry_delay_for_attempt(next_attempt);
                        let next_at =
                            Utc::now() + chrono::Duration::from_std(next_delay).unwrap_or_default();
                        let _ = sqlx::query(
                            "UPDATE automation_retry_queue
                                SET status='pending', attempt=$1, next_attempt_at=$2,
                                    last_error=$3, updated_at=NOW()
                                WHERE id=$4",
                        )
                        .bind(next_attempt)
                        .bind(next_at)
                        .bind(&err_str)
                        .bind(retry_id)
                        .execute(&self.db_pool)
                        .await;
                        tracing::warn!(
                            automation_id = %automation_id,
                            attempt = next_attempt,
                            next_attempt_at = %next_at,
                            "Webhook retry failed — rescheduled"
                        );
                    }
                }
            }
        }
    }

    // ─── Cron scheduling ────────────────────────────────────────────────

    /// Spawn background loops for cron-triggered automations (60s tick) and
    /// the retry queue worker (30s tick).
    pub fn start(self: Arc<Self>) {
        // Cron loop
        let cron_engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                cron_engine.check_scheduled_automations().await;
            }
        });

        // Retry worker
        let retry_engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                retry_engine.process_retry_queue().await;
            }
        });
    }

    async fn check_scheduled_automations(&self) {
        let rows = match sqlx::query(
            r#"SELECT id, name, description, enabled, trigger_type, trigger_config,
                      condition, action_type, action_config, created_at, updated_at
               FROM automations
               WHERE trigger_type = 'schedule' AND enabled = true"#,
        )
        .fetch_all(&self.db_pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Failed to query scheduled automations: {}", e);
                return;
            }
        };

        let now = Utc::now();
        for row in &rows {
            let automation = match automation_from_row(row) {
                Ok(a) => a,
                Err(e) => {
                    tracing::error!("Failed to parse automation row: {}", e);
                    continue;
                }
            };

            let cron_expr = match automation
                .trigger_config
                .get("cron")
                .and_then(|v| v.as_str())
            {
                Some(c) => c,
                None => continue,
            };

            if cron_matches(cron_expr, &now) {
                let event = TriggerEvent {
                    event_type: TriggerType::Schedule,
                    collection: None,
                    record_id: None,
                    data: json!({
                        "scheduled_at": now.to_rfc3339(),
                        "cron": cron_expr,
                    }),
                };

                let trigger_data = event.data.clone();
                let db_pool = self.db_pool.clone();
                let task_queue = self.task_queue.clone();
                let http_client = self.http_client.clone();
                let email_client = self.email_client.clone();
                let event_tx = self.event_tx.clone();
                let auto = automation.clone();

                tokio::spawn(async move {
                    let engine = AutomationEngine::new(db_pool, task_queue, http_client)
                        .with_email_client(email_client)
                        .with_event_tx(event_tx);
                    engine.execute_action(&auto, trigger_data).await;
                });
            }
        }
    }

    // ─── CRUD ───────────────────────────────────────────────────────────

    pub async fn list_automations(&self) -> Result<Vec<Automation>> {
        let rows = sqlx::query(
            r#"SELECT id, name, description, enabled, trigger_type, trigger_config,
                      condition, action_type, action_config, created_at, updated_at
               FROM automations ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to list automations: {}", e)))?;

        rows.iter().map(automation_from_row).collect()
    }

    pub async fn get_automation(&self, id: Uuid) -> Result<Automation> {
        let row = sqlx::query(
            r#"SELECT id, name, description, enabled, trigger_type, trigger_config,
                      condition, action_type, action_config, created_at, updated_at
               FROM automations WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to get automation: {}", e)))?
        .ok_or_else(|| AmosError::NotFound {
            entity: "Automation".to_string(),
            id: id.to_string(),
        })?;

        automation_from_row(&row)
    }

    pub async fn create_automation(
        &self,
        name: &str,
        description: Option<&str>,
        trigger_type: &str,
        trigger_config: JsonValue,
        condition: Option<JsonValue>,
        action_type: &str,
        action_config: JsonValue,
    ) -> Result<Automation> {
        // Validate trigger_type and action_type
        TriggerType::from_str(trigger_type).ok_or_else(|| {
            AmosError::Validation(format!("Invalid trigger_type: {}", trigger_type))
        })?;
        ActionType::from_str(action_type).ok_or_else(|| {
            AmosError::Validation(format!("Invalid action_type: {}", action_type))
        })?;

        let row = sqlx::query(
            r#"INSERT INTO automations (name, description, trigger_type, trigger_config, condition, action_type, action_config)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id, name, description, enabled, trigger_type, trigger_config,
                         condition, action_type, action_config, created_at, updated_at"#,
        )
        .bind(name)
        .bind(description)
        .bind(trigger_type)
        .bind(&trigger_config)
        .bind(&condition)
        .bind(action_type)
        .bind(&action_config)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to create automation: {}", e)))?;

        automation_from_row(&row)
    }

    pub async fn update_automation(&self, id: Uuid, updates: JsonValue) -> Result<Automation> {
        // Ensure automation exists
        self.get_automation(id).await?;

        // Build dynamic update
        let name = updates.get("name").and_then(|v| v.as_str());
        let description = updates.get("description").and_then(|v| v.as_str());
        let enabled = updates.get("enabled").and_then(|v| v.as_bool());
        let trigger_type = updates.get("trigger_type").and_then(|v| v.as_str());
        let trigger_config = updates.get("trigger_config");
        let condition = updates.get("condition");
        let action_type = updates.get("action_type").and_then(|v| v.as_str());
        let action_config = updates.get("action_config");

        if let Some(tt) = trigger_type {
            TriggerType::from_str(tt)
                .ok_or_else(|| AmosError::Validation(format!("Invalid trigger_type: {}", tt)))?;
        }
        if let Some(at) = action_type {
            ActionType::from_str(at)
                .ok_or_else(|| AmosError::Validation(format!("Invalid action_type: {}", at)))?;
        }

        let row = sqlx::query(
            r#"UPDATE automations SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                enabled = COALESCE($4, enabled),
                trigger_type = COALESCE($5, trigger_type),
                trigger_config = COALESCE($6, trigger_config),
                condition = CASE WHEN $7::boolean THEN $8 ELSE condition END,
                action_type = COALESCE($9, action_type),
                action_config = COALESCE($10, action_config),
                updated_at = NOW()
               WHERE id = $1
               RETURNING id, name, description, enabled, trigger_type, trigger_config,
                         condition, action_type, action_config, created_at, updated_at"#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(enabled)
        .bind(trigger_type)
        .bind(trigger_config.cloned())
        .bind(condition.is_some()) // flag: should we update condition?
        .bind(condition.cloned())
        .bind(action_type)
        .bind(action_config.cloned())
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to update automation: {}", e)))?;

        automation_from_row(&row)
    }

    pub async fn delete_automation(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM automations WHERE id = $1")
            .bind(id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to delete automation: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AmosError::NotFound {
                entity: "Automation".to_string(),
                id: id.to_string(),
            });
        }

        Ok(())
    }

    /// Get recent runs for an automation.
    pub async fn get_runs(&self, automation_id: Uuid, limit: i64) -> Result<Vec<AutomationRun>> {
        let rows = sqlx::query(
            r#"SELECT id, automation_id, trigger_data, status, result, error, duration_ms, created_at
               FROM automation_runs
               WHERE automation_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(automation_id)
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to get automation runs: {}", e)))?;

        rows.iter().map(run_from_row).collect()
    }
}

// ── Row helpers ──────────────────────────────────────────────────────────

fn automation_from_row(row: &sqlx::postgres::PgRow) -> Result<Automation> {
    let trigger_str: String = row.get("trigger_type");
    let trigger_type = TriggerType::from_str(&trigger_str).ok_or_else(|| {
        AmosError::Internal(format!("Unknown trigger_type in database: {}", trigger_str))
    })?;

    let action_str: String = row.get("action_type");
    let action_type = ActionType::from_str(&action_str).ok_or_else(|| {
        AmosError::Internal(format!("Unknown action_type in database: {}", action_str))
    })?;

    Ok(Automation {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        enabled: row.get("enabled"),
        trigger_type,
        trigger_config: row.get("trigger_config"),
        condition: row.get("condition"),
        action_type,
        action_config: row.get("action_config"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn run_from_row(row: &sqlx::postgres::PgRow) -> Result<AutomationRun> {
    Ok(AutomationRun {
        id: row.get("id"),
        automation_id: row.get("automation_id"),
        trigger_data: row.get("trigger_data"),
        status: row.get("status"),
        result: row.get("result"),
        error: row.get("error"),
        duration_ms: row.get("duration_ms"),
        created_at: row.get("created_at"),
    })
}

// ── Template substitution ────────────────────────────────────────────────

/// Simple template substitution: replaces `{{trigger.field}}` with values from trigger data.
fn substitute_template(template: &JsonValue, trigger_data: &JsonValue) -> JsonValue {
    match template {
        JsonValue::String(s) => {
            let mut result = s.clone();
            // Replace {{trigger.field}} patterns
            while let Some(start) = result.find("{{trigger.") {
                let rest = &result[start + 10..];
                if let Some(end) = rest.find("}}") {
                    let field = &rest[..end];
                    let replacement = trigger_data
                        .get(field)
                        .map(|v| match v {
                            JsonValue::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default();
                    result = format!("{}{}{}", &result[..start], replacement, &rest[end + 2..]);
                } else {
                    break;
                }
            }
            JsonValue::String(result)
        }
        JsonValue::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), substitute_template(v, trigger_data));
            }
            JsonValue::Object(new_map)
        }
        JsonValue::Array(arr) => JsonValue::Array(
            arr.iter()
                .map(|v| substitute_template(v, trigger_data))
                .collect(),
        ),
        other => other.clone(),
    }
}

// ── Condition evaluation ─────────────────────────────────────────────────

/// Simple condition evaluation: checks if all fields in `condition` match the trigger data
/// using JSONB containment semantics (all condition keys must exist with matching values).
fn evaluate_condition(condition: &JsonValue, data: &JsonValue) -> bool {
    let cond_obj = match condition.as_object() {
        Some(obj) => obj,
        None => return true, // Non-object condition always passes
    };

    let data_obj = match data.as_object() {
        Some(obj) => obj,
        None => return cond_obj.is_empty(),
    };

    for (key, expected) in cond_obj {
        match data_obj.get(key) {
            Some(actual) if actual == expected => continue,
            _ => return false,
        }
    }

    true
}

// ── Cron matching ────────────────────────────────────────────────────────

/// Simple 5-field cron matcher: "minute hour dom month dow"
/// Supports `*`, specific values, and comma-separated lists.
/// Parse a JSON value into a list of email addresses. Accepts either a single
/// string or an array of strings. Returns empty if the value is missing or
/// the wrong shape.
fn extract_address_list(value: Option<&JsonValue>) -> Vec<String> {
    match value {
        Some(JsonValue::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        Some(JsonValue::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn cron_matches(expr: &str, now: &chrono::DateTime<Utc>) -> bool {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        tracing::warn!("Invalid cron expression (expected 5 fields): {}", expr);
        return false;
    }

    let minute = now.format("%M").to_string().parse::<u32>().unwrap_or(0);
    let hour = now.format("%H").to_string().parse::<u32>().unwrap_or(0);
    let dom = now.format("%d").to_string().parse::<u32>().unwrap_or(0);
    let month = now.format("%m").to_string().parse::<u32>().unwrap_or(0);
    let dow = now.format("%u").to_string().parse::<u32>().unwrap_or(0); // 1=Mon .. 7=Sun

    // Cron dow: 0=Sun, 1=Mon..6=Sat. Convert chrono's 1=Mon..7=Sun.
    let cron_dow = if dow == 7 { 0 } else { dow };

    field_matches(fields[0], minute)
        && field_matches(fields[1], hour)
        && field_matches(fields[2], dom)
        && field_matches(fields[3], month)
        && field_matches(fields[4], cron_dow)
}

/// Check if a single cron field matches the given value.
/// Supports `*`, single values, comma lists, and `*/step`.
fn field_matches(field: &str, value: u32) -> bool {
    if field == "*" {
        return true;
    }

    // Handle */step (e.g., */15)
    if let Some(step_str) = field.strip_prefix("*/") {
        if let Ok(step) = step_str.parse::<u32>() {
            return step > 0 && value.is_multiple_of(step);
        }
    }

    // Comma-separated list
    for part in field.split(',') {
        if let Ok(v) = part.trim().parse::<u32>() {
            if v == value {
                return true;
            }
        }
    }

    false
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    // ── Retry backoff ───────────────────────────────────────────────

    #[test]
    fn retry_delay_grows_by_attempt() {
        assert_eq!(
            AutomationEngine::retry_delay_for_attempt(1).as_secs(),
            30,
            "first retry: 30s"
        );
        assert_eq!(
            AutomationEngine::retry_delay_for_attempt(2).as_secs(),
            120,
            "second retry: 2min"
        );
        assert_eq!(
            AutomationEngine::retry_delay_for_attempt(3).as_secs(),
            600,
            "third retry: 10min"
        );
    }

    #[test]
    fn retry_delay_caps_at_30min() {
        assert_eq!(
            AutomationEngine::retry_delay_for_attempt(10).as_secs(),
            1800,
            "delay caps at 30min for high attempts"
        );
    }

    // ── Cron matching ───────────────────────────────────────────────

    #[test]
    fn cron_star_matches_everything() {
        let now = Utc.with_ymd_and_hms(2026, 3, 24, 9, 30, 0).unwrap();
        assert!(cron_matches("* * * * *", &now));
    }

    #[test]
    fn cron_specific_minute() {
        let now = Utc.with_ymd_and_hms(2026, 3, 24, 9, 30, 0).unwrap();
        assert!(cron_matches("30 * * * *", &now));
        assert!(!cron_matches("15 * * * *", &now));
    }

    #[test]
    fn cron_specific_hour_and_minute() {
        let now = Utc.with_ymd_and_hms(2026, 3, 24, 9, 0, 0).unwrap();
        assert!(cron_matches("0 9 * * *", &now));
        assert!(!cron_matches("0 10 * * *", &now));
    }

    #[test]
    fn cron_comma_list() {
        let now = Utc.with_ymd_and_hms(2026, 3, 24, 9, 15, 0).unwrap();
        assert!(cron_matches("0,15,30,45 * * * *", &now));
        assert!(!cron_matches("0,10,20 * * * *", &now));
    }

    #[test]
    fn cron_step() {
        let now = Utc.with_ymd_and_hms(2026, 3, 24, 9, 30, 0).unwrap();
        assert!(cron_matches("*/15 * * * *", &now));
        assert!(!cron_matches("*/7 * * * *", &now)); // 30 % 7 != 0
    }

    #[test]
    fn cron_day_of_week() {
        // 2026-03-24 is a Tuesday (chrono: %u=2, cron dow=2)
        let now = Utc.with_ymd_and_hms(2026, 3, 24, 9, 0, 0).unwrap();
        assert!(cron_matches("0 9 * * 2", &now)); // Tuesday
        assert!(!cron_matches("0 9 * * 1", &now)); // Monday
    }

    #[test]
    fn cron_invalid_expr_returns_false() {
        let now = Utc::now();
        assert!(!cron_matches("bad cron", &now));
        assert!(!cron_matches("* * *", &now)); // only 3 fields
    }

    // ── Condition evaluation ────────────────────────────────────────

    #[test]
    fn condition_matches_all_fields() {
        let condition = json!({"status": "paid", "type": "order"});
        let data = json!({"status": "paid", "type": "order", "amount": 100});
        assert!(evaluate_condition(&condition, &data));
    }

    #[test]
    fn condition_fails_on_mismatch() {
        let condition = json!({"status": "paid"});
        let data = json!({"status": "pending"});
        assert!(!evaluate_condition(&condition, &data));
    }

    #[test]
    fn condition_fails_on_missing_field() {
        let condition = json!({"status": "paid"});
        let data = json!({"amount": 100});
        assert!(!evaluate_condition(&condition, &data));
    }

    #[test]
    fn empty_condition_always_passes() {
        let condition = json!({});
        let data = json!({"anything": true});
        assert!(evaluate_condition(&condition, &data));
    }

    #[test]
    fn null_condition_always_passes() {
        let condition = json!(null);
        let data = json!({"anything": true});
        assert!(evaluate_condition(&condition, &data));
    }

    // ── Template substitution ───────────────────────────────────────

    #[test]
    fn substitute_simple_string() {
        let template = json!("Order {{trigger.order_id}} was {{trigger.status}}");
        let data = json!({"order_id": "123", "status": "paid"});
        let result = substitute_template(&template, &data);
        assert_eq!(result, json!("Order 123 was paid"));
    }

    #[test]
    fn substitute_nested_object() {
        let template = json!({
            "event": "{{trigger.event_type}}",
            "details": {
                "id": "{{trigger.record_id}}"
            }
        });
        let data = json!({"event_type": "created", "record_id": "abc-123"});
        let result = substitute_template(&template, &data);
        assert_eq!(result["event"], "created");
        assert_eq!(result["details"]["id"], "abc-123");
    }

    #[test]
    fn substitute_missing_field_becomes_empty() {
        let template = json!("Hello {{trigger.name}}!");
        let data = json!({"other": "value"});
        let result = substitute_template(&template, &data);
        assert_eq!(result, json!("Hello !"));
    }

    #[test]
    fn substitute_non_string_value() {
        let template = json!("Count: {{trigger.count}}");
        let data = json!({"count": 42});
        let result = substitute_template(&template, &data);
        assert_eq!(result, json!("Count: 42"));
    }

    #[test]
    fn substitute_preserves_non_template_values() {
        let template = json!({"count": 5, "active": true, "items": [1, 2, 3]});
        let data = json!({});
        let result = substitute_template(&template, &data);
        assert_eq!(result, template);
    }
}
