//! Harness client - HTTP client for communicating with the AMOS Harness.
//!
//! The agent communicates with the harness using the same protocol as any
//! external agent. This module handles:
//! - Agent registration (POST /api/v1/agents/register)
//! - Heartbeat (POST /api/v1/agents/{id}/heartbeat)
//! - Task polling (GET /api/v1/agents/{id}/tasks)
//! - Result reporting (POST /api/v1/agents/{id}/tasks/{task_id}/result)
//! - Tool execution (POST /api/v1/agents/{id}/tools/execute)

use amos_core::{AmosError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Agent registration request.
#[derive(Debug, Serialize)]
pub struct RegisterRequest {
    pub name: String,
    pub capabilities: Vec<String>,
    pub agent_card_url: Option<String>,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidecar_secret: Option<String>,
}

/// Agent registration response.
#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    pub agent_id: String,
    pub token: String,
    pub harness_tools: Vec<HarnessTool>,
}

/// A tool available from the harness.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HarnessTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Task assignment from the harness.
#[derive(Debug, Deserialize)]
pub struct TaskAssignment {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub context: serde_json::Value,
    pub priority: u8,
}

/// Tool execution request to the harness.
#[derive(Debug, Serialize)]
pub struct ToolExecutionRequest {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub task_id: Option<String>,
}

/// Tool execution response from the harness.
#[derive(Debug, Deserialize)]
pub struct ToolExecutionResponse {
    pub content: String,
    pub is_error: bool,
    pub duration_ms: u64,
    /// Optional metadata from the tool (e.g. canvas actions, site preview info)
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Task result report.
#[derive(Debug, Serialize)]
pub struct TaskResult {
    pub status: String, // "completed" or "failed"
    pub output: serde_json::Value,
    pub error: Option<String>,
}

/// Client for communicating with the AMOS Harness.
pub struct HarnessClient {
    base_url: String,
    agent_id: Option<String>,
    token: Option<String>,
    http: Client,
    /// Cached list of tools available from the harness.
    pub harness_tools: Vec<HarnessTool>,
}

impl HarnessClient {
    /// Create a new harness client.
    pub fn new(base_url: &str, token: Option<String>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent_id: None,
            token,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("Failed to build HTTP client"),
            harness_tools: Vec::new(),
        }
    }

    /// Register the agent with the harness.
    pub async fn register(&mut self, name: &str, card_url: Option<&str>) -> Result<()> {
        let sidecar_secret = std::env::var("AMOS_SIDECAR_SECRET").ok().filter(|s| !s.is_empty());

        let req = RegisterRequest {
            name: name.to_string(),
            capabilities: vec![
                "chat".to_string(),
                "task_execution".to_string(),
                "tool_use".to_string(),
            ],
            agent_card_url: card_url.map(|s| s.to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
            sidecar_secret,
        };

        let url = format!("{}/api/v1/agents/register", self.base_url);
        debug!("Registering agent at {}", url);

        let mut request = self.http.post(&url).json(&req);
        if let Some(ref token) = self.token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to register with harness: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AmosError::Internal(format!(
                "Registration failed ({status}): {body}"
            )));
        }

        let reg: RegisterResponse = response.json().await.map_err(|e| {
            AmosError::Internal(format!("Failed to parse registration response: {e}"))
        })?;

        info!(
            agent_id = %reg.agent_id,
            tools = reg.harness_tools.len(),
            "Registered with harness"
        );

        self.agent_id = Some(reg.agent_id);
        self.token = Some(reg.token);
        self.harness_tools = reg.harness_tools;

        Ok(())
    }

    /// Send heartbeat to the harness.
    pub async fn heartbeat(&self) -> Result<()> {
        let agent_id = self
            .agent_id
            .as_ref()
            .ok_or_else(|| AmosError::Internal("Not registered".to_string()))?;

        let url = format!("{}/api/v1/agents/{}/heartbeat", self.base_url, agent_id);
        let mut request = self.http.post(&url);
        if let Some(ref token) = self.token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AmosError::Internal(format!("Heartbeat failed: {e}")))?;

        if !response.status().is_success() {
            warn!("Heartbeat failed: {}", response.status());
        }

        Ok(())
    }

    /// Poll for available tasks.
    pub async fn poll_tasks(&self) -> Result<Vec<TaskAssignment>> {
        let agent_id = self
            .agent_id
            .as_ref()
            .ok_or_else(|| AmosError::Internal("Not registered".to_string()))?;

        let url = format!("{}/api/v1/agents/{}/tasks", self.base_url, agent_id);
        let mut request = self.http.get(&url);
        if let Some(ref token) = self.token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AmosError::Internal(format!("Task poll failed: {e}")))?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let tasks: Vec<TaskAssignment> = response
            .json()
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to parse tasks: {e}")))?;

        Ok(tasks)
    }

    /// Execute a tool on the harness.
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        task_id: Option<&str>,
    ) -> Result<ToolExecutionResponse> {
        let agent_id = self
            .agent_id
            .as_ref()
            .ok_or_else(|| AmosError::Internal("Not registered".to_string()))?;

        let url = format!("{}/api/v1/agents/{}/tools/execute", self.base_url, agent_id);
        let req = ToolExecutionRequest {
            tool_name: tool_name.to_string(),
            input,
            task_id: task_id.map(|s| s.to_string()),
        };

        // Retry on connection-level errors with exponential backoff.
        // The first harness call in a conversation sometimes fails at the TCP
        // level (cold-start / keep-alive race). We retry up to 3 times.
        const RETRY_DELAYS_MS: &[u64] = &[500, 1000, 2000];

        let send_request =
            |client: &Client, url: &str, token: Option<&str>, req: &ToolExecutionRequest| {
                let mut builder = client.post(url).json(req);
                if let Some(t) = token {
                    builder = builder.bearer_auth(t);
                }
                builder
            };

        // Try the initial request, then retry with exponential backoff on transient errors.
        let response = {
            let mut delays_iter = RETRY_DELAYS_MS.iter();
            let mut attempt = 0u32;

            loop {
                match send_request(&self.http, &url, self.token.as_deref(), &req)
                    .send()
                    .await
                {
                    Ok(resp) => break resp,
                    Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
                        if let Some(&delay) = delays_iter.next() {
                            attempt += 1;
                            warn!(
                                tool = tool_name,
                                error = %e,
                                attempt,
                                max_retries = RETRY_DELAYS_MS.len(),
                                "Harness tool request failed, retrying in {delay}ms"
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        } else {
                            return Err(AmosError::Internal(format!(
                                "Tool execution request failed after {} retries: {e}",
                                RETRY_DELAYS_MS.len()
                            )));
                        }
                    }
                    Err(e) => {
                        return Err(AmosError::Internal(format!(
                            "Tool execution request failed: {e}"
                        )));
                    }
                }
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AmosError::Internal(format!(
                "Tool execution failed ({status}): {body}"
            )));
        }

        response
            .json()
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to parse tool result: {e}")))
    }

    /// Report task result to the harness.
    pub async fn report_result(&self, task_id: &str, result: TaskResult) -> Result<()> {
        let agent_id = self
            .agent_id
            .as_ref()
            .ok_or_else(|| AmosError::Internal("Not registered".to_string()))?;

        let url = format!(
            "{}/api/v1/agents/{}/tasks/{}/result",
            self.base_url, agent_id, task_id
        );

        let mut request = self.http.post(&url).json(&result);
        if let Some(ref token) = self.token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to report result: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Result report failed ({status}): {body}");
        }

        Ok(())
    }

    /// Get the harness tools as JSON schema for the LLM.
    pub fn harness_tool_schemas(&self) -> Vec<serde_json::Value> {
        self.harness_tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": format!("harness_{}", t.name),
                    "description": format!("[Harness Tool] {}", t.description),
                    "inputSchema": t.input_schema,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_client_creation() {
        let client = HarnessClient::new("http://localhost:3000", None);
        assert_eq!(client.base_url, "http://localhost:3000");
        assert!(client.agent_id.is_none());
        assert!(client.harness_tools.is_empty());
    }

    #[test]
    fn test_harness_tool_schemas() {
        let mut client = HarnessClient::new("http://localhost:3000", None);
        client.harness_tools = vec![HarnessTool {
            name: "get_time".to_string(),
            description: "Get current time".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }];

        let schemas = client.harness_tool_schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0]["name"], "harness_get_time");
    }
}
