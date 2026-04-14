//! Confirmation endpoint for destructive bash commands.
//!
//! When the bash tool detects a destructive command (rm -rf, kill, DROP TABLE, etc.),
//! it stores the command in `PendingConfirmations` and returns a confirmation token
//! to the frontend. The frontend renders approve/deny buttons. When the user
//! approves, this endpoint executes the stored command and returns the result.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

/// Build confirmation routes.
///
/// Nested under `/api/v1/tools` in `build_routes()`.
pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new().route("/confirm", post(confirm_command))
}

#[derive(Deserialize)]
struct ConfirmRequest {
    /// The confirmation token from the requires_confirmation response
    token: String,
    /// Whether the user approved the command
    approved: bool,
}

/// `POST /api/v1/tools/confirm` — Execute or reject a pending destructive command.
///
/// The frontend calls this when the user clicks approve or deny on a destructive
/// command confirmation prompt. If approved, the command is executed with the same
/// sandbox isolation as a normal bash call. If denied, the pending entry is removed.
async fn confirm_command(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfirmRequest>,
) -> impl IntoResponse {
    if !req.approved {
        // User denied — remove the pending entry (if it still exists)
        let _ = state.pending_confirmations.take(&req.token);
        info!(token = %req.token, "User denied destructive command");
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "denied",
                "message": "Command was denied by user."
            })),
        )
            .into_response();
    }

    // User approved — look up and execute the pending command
    let pending = match state.pending_confirmations.take(&req.token) {
        Some(p) => p,
        None => {
            warn!(token = %req.token, "Confirmation token not found or expired");
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "status": "error",
                    "message": "Confirmation token not found or expired (5 minute limit). Please retry the command."
                })),
            )
                .into_response();
        }
    };

    info!(command = %pending.command, token = %req.token, "Executing user-confirmed destructive command");

    // Execute with the same sandbox isolation as BashTool
    let command = pending.command.clone();
    let timeout_secs = pending.timeout_secs;

    // Scrub sensitive environment variables (same logic as BashTool)
    let scrubbed_env: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| {
            let k = key.to_uppercase();
            !k.starts_with("AMOS__")
                && k != "AMOS_SIDECAR_SECRET"
                && !k.contains("SECRET")
                && !k.contains("API_KEY")
                && !k.contains("STRIPE")
                && !k.contains("TOKEN")
                && !k.contains("PASSWORD")
                && !k.contains("CREDENTIAL")
                && !k.starts_with("AWS_")
                && k != "AGENT_URL"
                && !k.contains("DATABASE_URL")
                && !k.contains("DB_PASSWORD")
                && !k.contains("REDIS_URL")
        })
        .collect();

    let sandbox_uid = 1001u32;
    let sandbox_gid = 1001u32;

    let timeout = std::time::Duration::from_secs(timeout_secs);
    let output = match tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || {
            use std::os::unix::process::CommandExt;
            std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .env_clear()
                .envs(scrubbed_env)
                .uid(sandbox_uid)
                .gid(sandbox_gid)
                .output()
        }),
    )
    .await
    {
        Ok(Ok(Ok(output))) => output,
        Ok(Ok(Err(e))) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Command execution failed: {}", e)
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Task join error: {}", e)
                })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "timeout",
                    "message": format!("Command timed out after {} seconds", timeout_secs)
                })),
            )
                .into_response();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Truncate output (same limits as BashTool)
    let max_output = 50 * 1024;
    let stdout_truncated = stdout.len() > max_output;
    let stderr_truncated = stderr.len() > max_output;
    let stdout = if stdout_truncated {
        let boundary = stdout.floor_char_boundary(max_output);
        format!(
            "{}...\n[truncated — {} bytes total]",
            &stdout[..boundary],
            stdout.len()
        )
    } else {
        stdout
    };
    let stderr = if stderr_truncated {
        let boundary = stderr.floor_char_boundary(max_output);
        format!(
            "{}...\n[truncated — {} bytes total]",
            &stderr[..boundary],
            stderr.len()
        )
    } else {
        stderr
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "executed",
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": output.status.code(),
            "success": output.status.success(),
            "truncated": stdout_truncated || stderr_truncated
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_request_deserializes_approve() {
        let json = r#"{"token": "abc-123", "approved": true}"#;
        let req: ConfirmRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.token, "abc-123");
        assert!(req.approved);
    }

    #[test]
    fn confirm_request_deserializes_deny() {
        let json = r#"{"token": "abc-123", "approved": false}"#;
        let req: ConfirmRequest = serde_json::from_str(json).unwrap();
        assert!(!req.approved);
    }

    #[test]
    fn confirm_request_rejects_missing_token() {
        let json = r#"{"approved": true}"#;
        let result = serde_json::from_str::<ConfirmRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn confirm_request_rejects_missing_approved() {
        let json = r#"{"token": "abc-123"}"#;
        let result = serde_json::from_str::<ConfirmRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn confirm_request_rejects_empty_json() {
        let json = r#"{}"#;
        let result = serde_json::from_str::<ConfirmRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn pending_confirmations_deny_removes_entry() {
        let store = crate::state::PendingConfirmations::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        store.insert(
            "deny-token".to_string(),
            crate::state::PendingConfirmation {
                command: "rm -rf /tmp/data".to_string(),
                working_dir: None,
                timeout_secs: 120,
                warning: "test".to_string(),
                created_at: now,
            },
        );

        // Simulate deny: take removes the entry
        let entry = store.take("deny-token");
        assert!(entry.is_some());

        // Verify it's gone
        assert!(store.take("deny-token").is_none());
    }

    #[test]
    fn pending_confirmations_prune_cleans_expired() {
        let store = crate::state::PendingConfirmations::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Insert one expired and one fresh
        store.insert(
            "old".to_string(),
            crate::state::PendingConfirmation {
                command: "rm -rf /old".to_string(),
                working_dir: None,
                timeout_secs: 120,
                warning: "test".to_string(),
                created_at: now - 600, // 10 minutes ago
            },
        );
        store.insert(
            "fresh".to_string(),
            crate::state::PendingConfirmation {
                command: "rm -rf /fresh".to_string(),
                working_dir: None,
                timeout_secs: 120,
                warning: "test".to_string(),
                created_at: now,
            },
        );

        store.prune_expired();

        // Old should be gone, fresh should remain
        assert!(store.take("old").is_none());
        assert!(store.take("fresh").is_some());
    }
}
