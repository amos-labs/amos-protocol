//! System tools for file and process operations

use super::{Tool, ToolCategory, ToolResult};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::path::Path;
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

        // Block sensitive system directories
        let blocked_prefixes = [
            "/etc",
            "/proc",
            "/sys",
            "/dev",
            "/root",
            "/var/run",
            "/var/log",
            "/tmp",
            "/private/tmp",
            "/private/var",
        ];
        if blocked_prefixes
            .iter()
            .any(|p| canonical_str.starts_with(p))
        {
            return Ok(ToolResult::error(
                "Access denied: Cannot read files in system directories".to_string(),
            ));
        }

        // Block sensitive hidden directories anywhere in path
        let blocked_components = [".ssh", ".gnupg", ".aws", ".config", ".env"];
        if canonical.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            blocked_components.iter().any(|b| s == *b)
        }) {
            return Ok(ToolResult::error(
                "Access denied: Cannot read sensitive files".to_string(),
            ));
        }

        // Enforce allowed base directory if configured, otherwise use cwd
        let allowed_base = std::env::var("AMOS__TOOLS__ALLOWED_READ_DIR").unwrap_or_else(|_| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/".to_string())
        });
        let allowed_canonical = tokio::fs::canonicalize(&allowed_base)
            .await
            .unwrap_or_else(|_| Path::new(&allowed_base).to_path_buf());

        if !canonical.starts_with(&allowed_canonical) {
            return Ok(ToolResult::error(
                "Access denied: Path is outside allowed directory".to_string(),
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

/// Execute a bash command
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
        "Execute a shell command (with security restrictions)"
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

        // Security: reject subshell execution and backticks
        if command.contains("$(") || command.contains('`') {
            return Ok(ToolResult::error(
                "Blocked: Subshell execution is not allowed".to_string(),
            ));
        }

        // Comprehensive list of blocked commands (checked by token, not substring)
        const BLOCKED_COMMANDS: &[&str] = &[
            "rm",
            "mv",
            "cp",
            "chmod",
            "chown",
            "chroot",
            "mount",
            "umount",
            "mkfs",
            "dd",
            "kill",
            "killall",
            "pkill",
            "shutdown",
            "reboot",
            "halt",
            "poweroff",
            "systemctl",
            "service",
            "useradd",
            "userdel",
            "passwd",
            "su",
            "sudo",
            "curl",
            "wget",
            "nc",
            "ncat",
            "socat",
            "ssh",
            "scp",
            "sftp",
            "ftp",
            "telnet",
            "python",
            "python3",
            "ruby",
            "perl",
            "node",
            "php",
            "lua",
            "bash",
            "zsh",
            "csh",
            "ksh",
            "fish",
            "nohup",
            "screen",
            "tmux",
            "at",
            "crontab",
            "eval",
            "exec",
            "source",
            "docker",
            "podman",
            "kubectl",
            "apt",
            "yum",
            "dnf",
            "brew",
            "pip",
            "npm",
            "gem",
            "netcat",
            "mknod",
            "insmod",
            "modprobe",
            "iptables",
            "ip6tables",
        ];

        // Tokenize: split on whitespace and shell metacharacters
        let cmd_lower = command.to_lowercase();
        let tokens: Vec<&str> = cmd_lower
            .split(|c: char| c.is_whitespace() || ";|&<>()".contains(c))
            .filter(|t| !t.is_empty())
            .collect();

        for token in &tokens {
            // Extract basename for absolute paths (e.g. /usr/bin/curl -> curl)
            let basename = Path::new(token)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| token.to_string());

            if BLOCKED_COMMANDS.contains(&basename.as_str()) {
                return Ok(ToolResult::error(format!(
                    "Blocked: Command '{}' is not allowed",
                    basename
                )));
            }
        }

        // Block output redirection to sensitive paths
        if command.contains("> /dev/")
            || command.contains("> /etc/")
            || command.contains("> /proc/")
        {
            return Ok(ToolResult::error(
                "Blocked: Redirecting output to system paths is not allowed".to_string(),
            ));
        }

        // SECURITY: Block any command that reads from sensitive system paths.
        // This prevents secrets exfiltration via /proc/self/environ, /etc/shadow, etc.
        const BLOCKED_READ_PATHS: &[&str] = &[
            "/proc/",
            "/sys/",
            "/etc/shadow",
            "/etc/passwd",
            "/etc/sudoers",
            ".env",
            "keypair",
            "secret",
            ".pem",
            ".key",
        ];
        let cmd_lower_for_paths = command.to_lowercase();
        for blocked_path in BLOCKED_READ_PATHS {
            if cmd_lower_for_paths.contains(blocked_path) {
                return Ok(ToolResult::error(format!(
                    "Blocked: Access to '{}' paths is not allowed",
                    blocked_path
                )));
            }
        }

        // Execute command with a 30-second timeout to prevent DoS
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
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
                return Ok(ToolResult::error(
                    "Command timed out after 30 seconds".to_string(),
                ));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Limit output size to prevent exfiltration of large files
        let max_output = 10 * 1024; // 10KB
        let stdout_truncated = stdout.len() > max_output;
        let stderr_truncated = stderr.len() > max_output;
        let stdout = if stdout_truncated {
            format!("{}...\n[truncated — {} bytes total]", &stdout[..max_output], stdout.len())
        } else {
            stdout
        };
        let stderr = if stderr_truncated {
            format!("{}...\n[truncated — {} bytes total]", &stderr[..max_output], stderr.len())
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
