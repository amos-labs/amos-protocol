//! System tools for file and process operations

use super::{Tool, ToolCategory, ToolResult};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::process::Command;
use tokio::fs;

/// Read a file from the filesystem
pub struct ReadFileTool;

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file from the filesystem"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("path is required".to_string()))?;

        // Security: canonicalize to resolve symlinks and ..
        let canonical = tokio::fs::canonicalize(path).await.map_err(|e| {
            amos_core::AmosError::Internal(format!("Failed to resolve path: {}", e))
        })?;
        let canonical_str = canonical.to_string_lossy();

        // Block secrets/credentials paths (defense-in-depth — container is the real sandbox)
        let blocked_prefixes = ["/proc/self/environ", "/proc/1/environ", "/etc/shadow"];
        if blocked_prefixes
            .iter()
            .any(|p| canonical_str.starts_with(p))
        {
            return Ok(ToolResult::error(
                "Access denied: Cannot read secrets/credentials files".to_string(),
            ));
        }

        // Block sensitive credential directories
        let blocked_components = [".ssh", ".gnupg", ".aws"];
        if canonical.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            blocked_components.iter().any(|b| s == *b)
        }) {
            return Ok(ToolResult::error(
                "Access denied: Cannot read credential directories".to_string(),
            ));
        }

        // SECURITY: Read from the canonicalized path, not the original, to prevent
        // TOCTOU race conditions where a symlink is changed between check and read.
        let content = fs::read_to_string(&canonical)
            .await
            .map_err(|e| amos_core::AmosError::Internal(format!("Failed to read file: {}", e)))?;

        Ok(ToolResult::success(json!({
            "path": canonical_str,
            "content": content,
            "size": content.len()
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }
}

/// Execute a bash command.
///
/// Security model: the container is the sandbox, not the command blocklist.
/// Network-level isolation (iptables blocking metadata endpoint + internal IPs)
/// prevents attacks on external systems. Sensitive path restrictions prevent
/// secrets exfiltration. Everything else is allowed — the agent needs to be
/// able to install packages, run scripts, use curl, etc.
pub struct BashTool;

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command. You have full access to the container environment including apt, pip, curl, python, node, etc. Network access to external APIs is available. Use this tool freely to accomplish user tasks."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for command execution"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 120, max: 600)",
                    "default": 120
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("command is required".to_string()))?
            .to_string();

        let timeout_secs = params
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120)
            .min(600);

        // ── Hard blocks: secrets exfiltration prevention ──────────────────
        // These cannot be bypassed regardless of user intent. The container's
        // iptables rules block the metadata endpoint at the network level,
        // but we also block it here as defense-in-depth.

        let cmd_lower = command.to_lowercase();

        // Block access to secrets/credentials paths
        const BLOCKED_PATHS: &[&str] = &[
            "/proc/self/environ", // env var exfiltration
            "/proc/1/environ",
            "/etc/shadow",
            "169.254.169.254", // AWS metadata (defense-in-depth, also blocked by iptables)
            "169.254.170.2",   // ECS credential endpoint
        ];
        for blocked in BLOCKED_PATHS {
            if cmd_lower.contains(blocked) {
                return Ok(ToolResult::error(format!(
                    "Blocked: Access to '{}' is not allowed for security reasons",
                    blocked
                )));
            }
        }

        // Block output redirection to system paths
        if cmd_lower.contains("> /proc/") || cmd_lower.contains("> /sys/") {
            return Ok(ToolResult::error(
                "Blocked: Redirecting output to system paths is not allowed".to_string(),
            ));
        }

        // Block iptables/ip6tables modification (protect our own firewall rules)
        if cmd_lower.contains("iptables") || cmd_lower.contains("ip6tables") {
            return Ok(ToolResult::error(
                "Blocked: Firewall modification is not allowed".to_string(),
            ));
        }

        // ── Execute ──────────────────────────────────────────────────────

        let timeout = std::time::Duration::from_secs(timeout_secs);
        let output = match tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || Command::new("sh").arg("-c").arg(command).output()),
        )
        .await
        {
            Ok(join_result) => join_result
                .map_err(|e| amos_core::AmosError::Internal(format!("Task join error: {}", e)))?
                .map_err(|e| {
                    amos_core::AmosError::Internal(format!("Command execution failed: {}", e))
                })?,
            Err(_) => {
                return Ok(ToolResult::error(format!(
                    "Command timed out after {} seconds",
                    timeout_secs
                )));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Limit output size to prevent context window overflow
        let max_output = 50 * 1024; // 50KB (increased from 10KB for real utility)
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

        Ok(ToolResult::success(json!({
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": output.status.code(),
            "success": output.status.success(),
            "truncated": stdout_truncated || stderr_truncated
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }
}
