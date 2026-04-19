//! Google Vertex AI provider.
//!
//! Supports Claude models on Vertex AI (Anthropic models hosted on Google Cloud).
//! Uses the Anthropic Messages API format but authenticates via Google Cloud
//! credentials.
//!
//! ## Configuration
//!
//! ```bash
//! --model-provider vertex
//! --api-base https://us-east5-aiplatform.googleapis.com
//! --api-key <google-access-token>
//! --model-id claude-sonnet-4@20250514
//! ```
//!
//! The api_base should be the regional endpoint. The api_key should be a
//! Google Cloud access token (from `gcloud auth print-access-token`).

use crate::provider::{ModelProvider, StreamEvent, TokenUsage};
use amos_core::{
    types::{ContentBlock, Message, Role},
    AmosError, Result,
};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::error;

/// Google Vertex AI provider for Claude models.
pub struct VertexProvider {
    /// Regional endpoint (e.g., "https://us-east5-aiplatform.googleapis.com").
    api_base: String,
    /// Google Cloud access token.
    access_token: String,
    /// Project ID.
    project_id: String,
    /// Region (e.g., "us-east5").
    region: String,
    /// Default model ID.
    model_id: String,
    http_client: Client,
}

impl VertexProvider {
    pub fn new(api_base: String, access_token: String, model_id: String) -> Result<Self> {
        // Parse project_id and region from api_base or env vars
        let project_id = std::env::var("GOOGLE_CLOUD_PROJECT")
            .or_else(|_| std::env::var("VERTEX_PROJECT_ID"))
            .map_err(|_| {
                AmosError::Config(
                    "Vertex AI requires GOOGLE_CLOUD_PROJECT or VERTEX_PROJECT_ID env var"
                        .to_string(),
                )
            })?;

        let region = std::env::var("VERTEX_REGION").unwrap_or_else(|_| {
            // Try to extract from api_base
            extract_region(&api_base).unwrap_or_else(|| "us-east5".to_string())
        });

        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AmosError::Internal(format!("HTTP client error: {e}")))?;

        Ok(Self {
            api_base,
            access_token,
            project_id,
            region,
            model_id,
            http_client,
        })
    }

    /// Build the Vertex AI endpoint URL for Claude models.
    fn endpoint_url(&self, model_id: &str) -> String {
        format!(
            "{}/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:streamRawPredict",
            self.api_base, self.project_id, self.region, model_id
        )
    }
}

/// Extract region from a Vertex AI endpoint URL.
fn extract_region(url: &str) -> Option<String> {
    // Pattern: https://{region}-aiplatform.googleapis.com
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let region_part = host.split("-aiplatform").next()?;
    if region_part.is_empty() {
        None
    } else {
        Some(region_part.to_string())
    }
}

#[async_trait]
impl ModelProvider for VertexProvider {
    async fn converse_stream(
        &self,
        model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let effective_model = if model_id.is_empty() {
            &self.model_id
        } else {
            model_id
        };

        let url = self.endpoint_url(effective_model);

        // Build Anthropic-format request body (Vertex uses same format for Claude)
        let anthropic_messages = convert_messages(messages);
        let mut body = json!({
            "anthropic_version": "vertex-2023-10-16",
            "max_tokens": 8192,
            "system": system_prompt,
            "messages": anthropic_messages,
            "stream": true,
        });

        if !tools.is_empty() {
            // Convert to Anthropic-on-Vertex tool format. The schema may
            // arrive either as raw (agent-local tools) or wrapped in
            // Bedrock's `{json: ...}` envelope (harness tools). Vertex /
            // Anthropic want the raw schema — unwrap via the shared helper.
            let anthropic_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.get("name").and_then(|v| v.as_str()).unwrap_or_default(),
                        "description": t.get("description").and_then(|v| v.as_str()).unwrap_or_default(),
                        "input_schema": crate::tools::extract_tool_schema(t),
                    })
                })
                .collect();
            body["tools"] = json!(anthropic_tools);
        }

        let (tx, rx) = mpsc::channel(100);

        let http = self.http_client.clone();
        let token = self.access_token.clone();

        tokio::spawn(async move {
            let response = http
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    // Parse SSE stream (same as Anthropic format)
                    let body_text = resp.text().await.unwrap_or_default();
                    crate::anthropic::parse_anthropic_sse_text(&body_text, &tx).await;
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    error!("Vertex AI error {}: {}", status, body);
                    let _ = tx
                        .send(StreamEvent::Error(format!(
                            "Vertex AI error {status}: {body}"
                        )))
                        .await;
                }
                Err(e) => {
                    error!("Vertex AI request error: {e}");
                    let _ = tx
                        .send(StreamEvent::Error(format!("Vertex AI request error: {e}")))
                        .await;
                }
            }
        });

        Ok(rx)
    }

    async fn converse(
        &self,
        model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<(Message, TokenUsage)> {
        let mut rx = self
            .converse_stream(model_id, system_prompt, messages, tools)
            .await?;

        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_text = String::new();
        let mut usage = TokenUsage::default();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta(text) => current_text.push_str(&text),
                StreamEvent::ToolUse { id, name, input } => {
                    if !current_text.is_empty() {
                        content_blocks.push(ContentBlock::Text {
                            text: std::mem::take(&mut current_text),
                        });
                    }
                    content_blocks.push(ContentBlock::ToolUse { id, name, input });
                }
                StreamEvent::TokenUsage(u) => usage = u,
                StreamEvent::Stop => break,
                StreamEvent::Error(e) => {
                    return Err(AmosError::Internal(format!("Vertex AI stream error: {e}")));
                }
            }
        }

        if !current_text.is_empty() {
            content_blocks.push(ContentBlock::Text { text: current_text });
        }

        Ok((
            Message {
                role: Role::Assistant,
                content: content_blocks,
                tool_use_id: None,
                timestamp: Utc::now(),
            },
            usage,
        ))
    }

    fn provider_name(&self) -> &str {
        "vertex"
    }
}

/// Convert internal messages to Anthropic API format.
fn convert_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User | Role::Tool => "user",
                Role::Assistant => "assistant",
                Role::System => "user", // System messages go in the system field
            };

            let content: Vec<serde_json::Value> = msg
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => json!({"type": "text", "text": text}),
                    ContentBlock::ToolUse { id, name, input } => {
                        json!({"type": "tool_use", "id": id, "name": name, "input": input})
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error,
                        })
                    }
                    ContentBlock::Image { source } => {
                        json!({
                            "type": "image",
                            "source": {
                                "type": source.source_type,
                                "media_type": source.media_type,
                                "data": source.data,
                            }
                        })
                    }
                    ContentBlock::Document { source } => {
                        json!({
                            "type": "document",
                            "source": {
                                "type": "base64",
                                "media_type": format!("application/{}", source.format),
                                "data": source.data,
                            }
                        })
                    }
                })
                .collect();

            json!({"role": role, "content": content})
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_region() {
        assert_eq!(
            extract_region("https://us-east5-aiplatform.googleapis.com"),
            Some("us-east5".to_string())
        );
        assert_eq!(
            extract_region("https://europe-west4-aiplatform.googleapis.com"),
            Some("europe-west4".to_string())
        );
    }

    #[test]
    fn test_convert_messages() {
        let msgs = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
            tool_use_id: None,
            timestamp: Utc::now(),
        }];
        let converted = convert_messages(&msgs);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "user");
    }
}
