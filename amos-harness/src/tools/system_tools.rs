//! System tools for file and process operations

use super::{Tool, ToolCategory, ToolResult};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::process::Command;
use std::sync::Arc;
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
///
/// Destructive commands (rm -rf, kill, DROP TABLE, etc.) require explicit
/// user confirmation before executing. The tool returns a `requires_confirmation`
/// response with a token; the frontend shows approve/deny buttons. The user
/// can also pass `confirmed: "<token>"` to bypass the gate for a specific command.
pub struct BashTool {
    pending_confirmations: Arc<crate::state::PendingConfirmations>,
}

impl BashTool {
    pub fn new(pending_confirmations: Arc<crate::state::PendingConfirmations>) -> Self {
        Self {
            pending_confirmations,
        }
    }
}

/// Check if a command looks destructive and needs user confirmation.
///
/// Returns `Some(warning)` if the command is destructive, `None` if safe.
fn classify_destructive(cmd: &str) -> Option<String> {
    let cmd_lower = cmd.to_lowercase();
    let cmd_trimmed = cmd_lower.trim();

    // ── File/directory deletion ──────────────────────────────────────
    // rm with -r, -f, or -rf flags (not bare `rm file.txt`)
    if cmd_trimmed.starts_with("rm ")
        || cmd_lower.contains("| rm ")
        || cmd_lower.contains("&& rm ")
        || cmd_lower.contains("; rm ")
    {
        // Check for recursive/force flags or wildcard patterns
        if cmd_lower.contains(" -r")
            || cmd_lower.contains(" -f")
            || cmd_lower.contains(" --recursive")
            || cmd_lower.contains(" --force")
            || cmd_lower.contains(" *")
            || cmd_lower.contains(" /")
        {
            return Some("This command will delete files or directories.".to_string());
        }
    }

    // rmdir (less dangerous but still destructive)
    if cmd_lower.contains("rmdir ") {
        return Some("This command will remove directories.".to_string());
    }

    // ── Process termination ─────────────────────────────────────────
    for pattern in &["kill ", "kill -", "killall ", "pkill ", "xargs kill"] {
        if cmd_lower.contains(pattern) {
            return Some("This command will terminate running processes.".to_string());
        }
    }

    // ── Database destructive operations ─────────────────────────────
    for pattern in &[
        "drop table",
        "drop database",
        "drop schema",
        "drop index",
        "truncate ",
        "delete from",
    ] {
        if cmd_lower.contains(pattern) {
            return Some("This command will modify or destroy database objects.".to_string());
        }
    }

    // ── Disk/filesystem destructive operations ──────────────────────
    for pattern in &["mkfs", "dd if=", "fdisk", "parted", "wipefs"] {
        if cmd_lower.contains(pattern) {
            return Some("This command will modify disk or filesystem structures.".to_string());
        }
    }

    // ── System/service management ───────────────────────────────────
    for pattern in &[
        "systemctl stop",
        "systemctl disable",
        "systemctl restart",
        "service stop",
        "shutdown",
        "reboot",
        "init 0",
        "init 6",
        "halt",
        "poweroff",
    ] {
        if cmd_lower.contains(pattern) {
            return Some("This command will affect system services or power state.".to_string());
        }
    }

    // ── Dangerous overwrites ────────────────────────────────────────
    // `mv` to root or overwriting critical paths
    if cmd_lower.contains("mv ") && cmd_lower.contains(" /") {
        // mv something to / or a top-level directory
        if cmd_lower.contains(" /bin")
            || cmd_lower.contains(" /usr")
            || cmd_lower.contains(" /etc")
            || cmd_lower.contains(" /var")
            || cmd_lower.contains(" /home")
        {
            return Some("This command moves files to a system directory.".to_string());
        }
    }

    // chmod -R / chown -R (recursive permission changes)
    // Note: -R becomes -r after lowercasing
    if (cmd_lower.contains("chmod ") || cmd_lower.contains("chown ")) && cmd_lower.contains(" -r") {
        return Some("This command recursively changes file permissions or ownership.".to_string());
    }

    // ── Package removal ─────────────────────────────────────────────
    for pattern in &[
        "apt remove",
        "apt-get remove",
        "apt purge",
        "apt-get purge",
        "apt autoremove",
        "yum remove",
        "dnf remove",
        "pip uninstall",
        "npm uninstall -g",
    ] {
        if cmd_lower.contains(pattern) {
            return Some("This command will uninstall packages.".to_string());
        }
    }

    // ── Git destructive operations ──────────────────────────────────
    // Note: git branch -D is case-sensitive (uppercase D = force-delete),
    // so we check against the original command, not cmd_lower.
    for pattern in &[
        "git reset --hard",
        "git clean -f",
        "git push --force",
        "git push -f ",
    ] {
        if cmd_lower.contains(pattern) {
            return Some("This git command is destructive and may cause data loss.".to_string());
        }
    }
    if cmd.contains("git branch -D") {
        return Some("This git command is destructive and may cause data loss.".to_string());
    }

    None
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command. You have full access to the container environment including apt, pip, curl, python, node, etc. Network access to external APIs is available. Use this tool freely to accomplish user tasks. Destructive commands (rm -rf, kill, DROP TABLE, etc.) will require user confirmation — the tool will return a confirmation token that the user must approve before execution proceeds."
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
                },
                "confirmed": {
                    "type": "string",
                    "description": "Confirmation token from a previous requires_confirmation response. Pass this to execute a command that was previously blocked for user approval."
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

        // Block shell metacharacter tricks that could bypass pattern-based detection.
        // These block piping commands into a new shell or using eval to construct commands.
        const SHELL_BYPASS_PATTERNS: &[&str] = &[
            "| bash", // pipe into bash
            "| sh",   // pipe into sh
            "|bash",  // no-space variant
            "|sh",    // no-space variant
            "| /bin/sh",
            "| /bin/bash",
            "eval ", // eval executes constructed strings
            "`",     // backtick subshell (command substitution that hides intent)
            "| zsh",
            "|zsh",
        ];
        for pattern in SHELL_BYPASS_PATTERNS {
            if cmd_lower.contains(pattern) {
                return Ok(ToolResult::error(format!(
                    "Blocked: Shell bypass pattern '{}' is not allowed for security reasons",
                    pattern.trim()
                )));
            }
        }

        // ── Destructive command confirmation gate ────────────────────────
        // If the command looks destructive, check for a confirmation token.
        // If confirmed, verify the token matches a pending entry and proceed.
        // If not confirmed, store the command and return requires_confirmation.

        let confirmed_token = params
            .get("confirmed")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(warning) = classify_destructive(&command) {
            match confirmed_token {
                Some(ref token) => {
                    // User provided a confirmation token — verify it
                    match self.pending_confirmations.take(token) {
                        Some(pending) if pending.command == command => {
                            // Token valid, command matches — fall through to execution
                            tracing::info!(command = %command, "Destructive command confirmed by user");
                        }
                        Some(_) => {
                            return Ok(ToolResult::error(
                                "Confirmation token does not match this command. Please retry."
                                    .to_string(),
                            ));
                        }
                        None => {
                            return Ok(ToolResult::error(
                                "Confirmation token is invalid or expired (5 minute limit). Please retry the command."
                                    .to_string(),
                            ));
                        }
                    }
                }
                None => {
                    // No token — store pending and ask for confirmation
                    let token = uuid::Uuid::new_v4().to_string();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    self.pending_confirmations.insert(
                        token.clone(),
                        crate::state::PendingConfirmation {
                            command: command.clone(),
                            working_dir: params
                                .get("working_dir")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            timeout_secs,
                            warning: warning.clone(),
                            created_at: now,
                        },
                    );

                    tracing::info!(command = %command, token = %token, "Destructive command blocked — awaiting confirmation");

                    return Ok(ToolResult::success_with_metadata(
                        json!({
                            "requires_confirmation": true,
                            "confirmation_token": token,
                            "command": command,
                            "warning": warning,
                            "message": "This command requires user confirmation before it can execute. The user will see an approve/deny prompt in the chat."
                        }),
                        json!({
                            "__confirmation_required": {
                                "token": token,
                                "command": command,
                                "warning": warning
                            }
                        }),
                    ));
                }
            }
        }

        // ── Execute ──────────────────────────────────────────────────────

        // Scrub sensitive environment variables from the subprocess.
        // The agent/user bash tool should never be able to read secrets
        // like database credentials, API keys, or internal tokens.
        let scrubbed_env: Vec<(String, String)> = std::env::vars()
            .filter(|(key, _)| {
                let k = key.to_uppercase();
                // Block all AMOS internal config (DB URL, vault key, JWT secret, etc.)
                !k.starts_with("AMOS__")
                    // Block sidecar secret used for agent trust elevation
                    && k != "AMOS_SIDECAR_SECRET"
                    // Block API keys and tokens
                    && !k.contains("SECRET")
                    && !k.contains("API_KEY")
                    && !k.contains("STRIPE")
                    && !k.contains("TOKEN")
                    && !k.contains("PASSWORD")
                    && !k.contains("CREDENTIAL")
                    // Block ALL AWS env vars (access key, session, credential URIs)
                    && !k.starts_with("AWS_")
                    // Block internal service URLs
                    && k != "AGENT_URL"
                    && !k.contains("DATABASE_URL")
                    && !k.contains("DB_PASSWORD")
                    && !k.contains("REDIS_URL")
            })
            .collect();

        // Run the subprocess as the `sandbox` user (uid 1001) for process isolation.
        // This prevents the subprocess from reading /proc/1/environ (owned by uid 1000/amos)
        // even if they bypass the string-based command blocklist.
        let sandbox_uid = 1001u32;
        let sandbox_gid = 1001u32;

        let timeout = std::time::Duration::from_secs(timeout_secs);
        let output = match tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || {
                use std::os::unix::process::CommandExt;
                Command::new("sh")
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_destructive ─────────────────────────────────────────

    #[test]
    fn safe_commands_pass_through() {
        assert!(classify_destructive("ls -la").is_none());
        assert!(classify_destructive("cat /etc/hosts").is_none());
        assert!(classify_destructive("echo hello").is_none());
        assert!(classify_destructive("python3 script.py").is_none());
        assert!(classify_destructive("curl https://example.com").is_none());
        assert!(classify_destructive("apt update").is_none());
        assert!(classify_destructive("pip install requests").is_none());
        assert!(classify_destructive("npm install").is_none());
        assert!(classify_destructive("git status").is_none());
        assert!(classify_destructive("git commit -m 'test'").is_none());
        assert!(classify_destructive("git push origin main").is_none());
        assert!(classify_destructive("mv file.txt file2.txt").is_none());
        assert!(classify_destructive("chmod 644 file.txt").is_none());
    }

    #[test]
    fn rm_rf_is_destructive() {
        assert!(classify_destructive("rm -rf /tmp/data").is_some());
        assert!(classify_destructive("rm -f file.txt").is_some());
        assert!(classify_destructive("rm -r directory/").is_some());
        assert!(classify_destructive("rm --recursive --force /workspace/old").is_some());
        assert!(classify_destructive("rm *").is_some());
    }

    #[test]
    fn bare_rm_is_not_destructive() {
        // Simple `rm file.txt` without flags is allowed without confirmation
        assert!(classify_destructive("rm file.txt").is_none());
    }

    #[test]
    fn piped_rm_is_destructive() {
        assert!(classify_destructive("find . -name '*.tmp' | rm -f").is_some());
        assert!(classify_destructive("echo x && rm -rf /tmp").is_some());
        assert!(classify_destructive("ls; rm -rf /workspace").is_some());
    }

    #[test]
    fn kill_commands_are_destructive() {
        assert!(classify_destructive("kill 1234").is_some());
        assert!(classify_destructive("kill -9 1234").is_some());
        assert!(classify_destructive("killall nginx").is_some());
        assert!(classify_destructive("pkill python").is_some());
    }

    #[test]
    fn database_ops_are_destructive() {
        assert!(classify_destructive("psql -c 'DROP TABLE users'").is_some());
        assert!(classify_destructive("mysql -e 'TRUNCATE orders'").is_some());
        assert!(classify_destructive("sqlite3 db.sqlite 'DELETE FROM sessions'").is_some());
        assert!(classify_destructive("DROP DATABASE production").is_some());
    }

    #[test]
    fn disk_ops_are_destructive() {
        assert!(classify_destructive("mkfs.ext4 /dev/sda1").is_some());
        assert!(classify_destructive("dd if=/dev/zero of=/dev/sda").is_some());
    }

    #[test]
    fn system_ops_are_destructive() {
        assert!(classify_destructive("shutdown -h now").is_some());
        assert!(classify_destructive("reboot").is_some());
        assert!(classify_destructive("systemctl stop nginx").is_some());
    }

    #[test]
    fn git_destructive_ops_are_flagged() {
        assert!(classify_destructive("git reset --hard HEAD~3").is_some());
        assert!(classify_destructive("git clean -fd").is_some());
        assert!(classify_destructive("git push --force origin main").is_some());
        assert!(classify_destructive("git branch -D feature").is_some());
    }

    #[test]
    fn recursive_permission_changes_are_destructive() {
        assert!(classify_destructive("chmod -R 777 /workspace").is_some());
        assert!(classify_destructive("chown -R root:root /workspace").is_some());
    }

    #[test]
    fn package_removal_is_destructive() {
        assert!(classify_destructive("apt remove nginx").is_some());
        assert!(classify_destructive("pip uninstall flask").is_some());
    }

    #[test]
    fn rmdir_is_destructive() {
        assert!(classify_destructive("rmdir /workspace/empty").is_some());
    }

    // ── PendingConfirmations ─────────────────────────────────────────

    #[test]
    fn pending_confirmations_insert_and_take() {
        let store = crate::state::PendingConfirmations::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        store.insert(
            "test-token".to_string(),
            crate::state::PendingConfirmation {
                command: "rm -rf /tmp/data".to_string(),
                working_dir: None,
                timeout_secs: 120,
                warning: "test warning".to_string(),
                created_at: now,
            },
        );

        let entry = store.take("test-token");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().command, "rm -rf /tmp/data");

        // Second take should return None (consumed)
        assert!(store.take("test-token").is_none());
    }

    #[test]
    fn pending_confirmations_expire() {
        let store = crate::state::PendingConfirmations::new();
        // Set created_at to 10 minutes ago (past the 5-min TTL)
        let old_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 600;

        store.insert(
            "expired-token".to_string(),
            crate::state::PendingConfirmation {
                command: "rm -rf /tmp/data".to_string(),
                working_dir: None,
                timeout_secs: 120,
                warning: "test".to_string(),
                created_at: old_time,
            },
        );

        assert!(store.take("expired-token").is_none());
    }

    // ── BashTool confirmation gate integration tests ─────────────────
    // These call BashTool::execute() directly and test the confirmation
    // round-trip. The gate returns BEFORE spawning a subprocess, so
    // these work on any platform (no sandbox user needed).

    fn make_bash_tool() -> (BashTool, Arc<crate::state::PendingConfirmations>) {
        let store = Arc::new(crate::state::PendingConfirmations::new());
        let tool = BashTool::new(store.clone());
        (tool, store)
    }

    #[tokio::test]
    async fn destructive_command_returns_requires_confirmation() {
        let (tool, store) = make_bash_tool();

        let result = tool
            .execute(json!({"command": "rm -rf /tmp/test"}))
            .await
            .unwrap();

        // Should succeed (it's not an error — it's a "please confirm" response)
        assert!(result.success);
        let data = result.data.unwrap();
        assert_eq!(data["requires_confirmation"], true);
        assert_eq!(data["command"], "rm -rf /tmp/test");
        assert!(data["confirmation_token"].is_string());
        assert!(data["warning"].is_string());

        // Token should be stored in the pending confirmations
        let token = data["confirmation_token"].as_str().unwrap();
        // Peek — take consumes it, so verify it exists then put it back
        let entry = store.take(token);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().command, "rm -rf /tmp/test");
    }

    #[tokio::test]
    async fn destructive_command_returns_metadata_for_frontend() {
        let (tool, _store) = make_bash_tool();

        let result = tool
            .execute(json!({"command": "kill -9 1234"}))
            .await
            .unwrap();

        assert!(result.success);
        let metadata = result.metadata.unwrap();
        let confirmation = &metadata["__confirmation_required"];
        assert!(confirmation["token"].is_string());
        assert_eq!(confirmation["command"], "kill -9 1234");
        assert!(confirmation["warning"].is_string());
    }

    #[tokio::test]
    async fn safe_command_does_not_require_confirmation() {
        let (tool, _store) = make_bash_tool();

        let result = tool.execute(json!({"command": "echo hello"})).await;

        // On macOS dev this will fail at subprocess spawn (no uid 1001),
        // but the key assertion is that it did NOT return requires_confirmation.
        // If it reaches execution, that means the confirmation gate was skipped.
        match result {
            Ok(r) => {
                // Either it executed (success) or failed at subprocess level,
                // but it should NOT have requires_confirmation
                if let Some(data) = &r.data {
                    assert!(
                        data.get("requires_confirmation").is_none()
                            || data["requires_confirmation"] == false,
                        "Safe command should not require confirmation"
                    );
                }
            }
            Err(_) => {
                // Subprocess error (expected on macOS without uid 1001) — that's fine,
                // the point is it got past the confirmation gate
            }
        }
    }

    #[tokio::test]
    async fn confirmed_token_with_wrong_command_is_rejected() {
        let (tool, store) = make_bash_tool();

        // First call: get a confirmation token for rm -rf
        let result = tool
            .execute(json!({"command": "rm -rf /tmp/test"}))
            .await
            .unwrap();
        let token = result.data.unwrap()["confirmation_token"]
            .as_str()
            .unwrap()
            .to_string();

        // Second call: try to use the token with a DIFFERENT command
        let result2 = tool
            .execute(json!({
                "command": "rm -rf /tmp/something-else",
                "confirmed": token
            }))
            .await
            .unwrap();

        // Token was consumed by the mismatch check, so store should be empty
        assert!(!result2.success);
        assert!(result2
            .error
            .unwrap()
            .contains("does not match this command"));

        // Original token should be consumed (can't retry)
        assert!(store.take(&token).is_none());
    }

    #[tokio::test]
    async fn expired_confirmation_token_is_rejected() {
        let (tool, store) = make_bash_tool();

        // Manually insert an expired token
        let token = "expired-test-token".to_string();
        let old_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 600; // 10 minutes ago

        store.insert(
            token.clone(),
            crate::state::PendingConfirmation {
                command: "rm -rf /tmp/test".to_string(),
                working_dir: None,
                timeout_secs: 120,
                warning: "test".to_string(),
                created_at: old_time,
            },
        );

        let result = tool
            .execute(json!({
                "command": "rm -rf /tmp/test",
                "confirmed": token
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("invalid or expired"));
    }

    #[tokio::test]
    async fn bogus_confirmation_token_is_rejected() {
        let (tool, _store) = make_bash_tool();

        let result = tool
            .execute(json!({
                "command": "rm -rf /tmp/test",
                "confirmed": "totally-fake-token"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("invalid or expired"));
    }

    #[tokio::test]
    async fn hard_blocks_still_apply_to_destructive_commands() {
        let (tool, _store) = make_bash_tool();

        // Even if a command is destructive, hard blocks take priority
        let result = tool
            .execute(json!({"command": "rm -rf /proc/self/environ"}))
            .await
            .unwrap();

        // Should be blocked by hard block, not by confirmation gate
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Blocked"));
    }

    #[tokio::test]
    async fn multiple_destructive_commands_get_independent_tokens() {
        let (tool, _store) = make_bash_tool();

        let r1 = tool
            .execute(json!({"command": "rm -rf /tmp/a"}))
            .await
            .unwrap();
        let r2 = tool
            .execute(json!({"command": "rm -rf /tmp/b"}))
            .await
            .unwrap();

        let token1 = r1.data.unwrap()["confirmation_token"]
            .as_str()
            .unwrap()
            .to_string();
        let token2 = r2.data.unwrap()["confirmation_token"]
            .as_str()
            .unwrap()
            .to_string();

        assert_ne!(token1, token2, "Each command should get a unique token");
    }

    #[tokio::test]
    async fn confirmation_stores_working_dir_and_timeout() {
        let (tool, store) = make_bash_tool();

        let result = tool
            .execute(json!({
                "command": "rm -rf ./build",
                "working_dir": "/workspace/project",
                "timeout_secs": 300
            }))
            .await
            .unwrap();

        let token = result.data.unwrap()["confirmation_token"]
            .as_str()
            .unwrap()
            .to_string();

        let pending = store.take(&token).unwrap();
        assert_eq!(pending.working_dir.as_deref(), Some("/workspace/project"));
        assert_eq!(pending.timeout_secs, 300);
    }
}
