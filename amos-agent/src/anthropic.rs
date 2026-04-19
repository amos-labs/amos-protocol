//! Native Anthropic Messages API provider.
//!
//! Anthropic's `/v1/messages` API differs from the OpenAI chat completions format:
//! - System prompt is a top-level field, not a message
//! - Tool definitions use `input_schema` (not `parameters`)
//! - SSE events use `content_block_delta` (not `choices[].delta`)
//! - Authentication uses `x-api-key` header (not Bearer token)
//!
//! This provider handles all those differences so the agent loop stays
//! provider-agnostic.

use crate::provider::{ModelProvider, StreamEvent, TokenUsage};
use amos_core::{
    types::{ContentBlock, Message, Role},
    AmosError, Result,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::error;

// ═══════════════════════════════════════════════════════════════════════════
// REQUEST TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    #[allow(dead_code)]
    Text(String),
    Blocks(Vec<AnthropicBlock>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlock {
    Text {
        text: String,
    },
    Image {
        source: AnthropicMediaSource,
    },
    Document {
        source: AnthropicMediaSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

/// Base64 source object for Image and Document blocks in the Anthropic API.
#[derive(Debug, Serialize)]
struct AnthropicMediaSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

// ═══════════════════════════════════════════════════════════════════════════
// SSE RESPONSE TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    #[allow(dead_code)]
    index: Option<usize>,
    #[serde(default)]
    content_block: Option<SseContentBlock>,
    #[serde(default)]
    delta: Option<SseDelta>,
    #[serde(default)]
    usage: Option<SseUsage>,
    #[serde(default)]
    message: Option<SseMessageMeta>,
}

#[derive(Debug, Deserialize)]
struct SseContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    partial_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct SseMessageMeta {
    #[serde(default)]
    usage: Option<SseUsage>,
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER
// ═══════════════════════════════════════════════════════════════════════════

pub struct AnthropicProvider {
    api_base: String,
    api_key: String,
    model_id: String,
    http_client: Client,
}

impl AnthropicProvider {
    pub fn new(api_base: String, api_key: String, model_id: String) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AmosError::Internal(format!("Failed to build HTTP client: {}", e)))?;
        Ok(Self {
            api_base,
            api_key,
            model_id,
            http_client,
        })
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<AnthropicMessage> {
        let mut out = Vec::new();
        for msg in messages {
            match msg.role {
                Role::User => {
                    let mut blocks = Vec::new();
                    for block in &msg.content {
                        match block {
                            ContentBlock::Text { text } => {
                                blocks.push(AnthropicBlock::Text { text: text.clone() });
                            }
                            ContentBlock::Image { source } => {
                                blocks.push(AnthropicBlock::Image {
                                    source: AnthropicMediaSource {
                                        source_type: "base64".to_string(),
                                        media_type: source.media_type.clone(),
                                        data: source.data.clone(),
                                    },
                                });
                            }
                            ContentBlock::Document { source } => {
                                // Map format string to MIME type for the API
                                let media_type = match source.format.as_str() {
                                    "pdf" => "application/pdf".to_string(),
                                    other => format!("application/{other}"),
                                };
                                blocks.push(AnthropicBlock::Document {
                                    source: AnthropicMediaSource {
                                        source_type: "base64".to_string(),
                                        media_type,
                                        data: source.data.clone(),
                                    },
                                });
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => {
                                blocks.push(AnthropicBlock::ToolResult {
                                    tool_use_id: tool_use_id.clone(),
                                    content: content.clone(),
                                    is_error: *is_error,
                                });
                            }
                            _ => {}
                        }
                    }
                    if !blocks.is_empty() {
                        out.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Blocks(blocks),
                        });
                    }
                }
                Role::Assistant => {
                    let mut blocks = Vec::new();
                    for block in &msg.content {
                        match block {
                            ContentBlock::Text { text } => {
                                blocks.push(AnthropicBlock::Text { text: text.clone() });
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                blocks.push(AnthropicBlock::ToolUse {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                    if !blocks.is_empty() {
                        out.push(AnthropicMessage {
                            role: "assistant".to_string(),
                            content: AnthropicContent::Blocks(blocks),
                        });
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn convert_tools(&self, tools: &[serde_json::Value]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .filter_map(|tool| {
                let name = tool["name"].as_str()?.to_string();
                let description = tool["description"].as_str().unwrap_or("").to_string();
                // Harness tools arrive with Bedrock's `{json: <schema>}`
                // envelope; agent-local tools arrive with the schema
                // directly. Anthropic wants the raw schema — unwrap.
                let input_schema = crate::tools::extract_tool_schema(tool);
                Some(AnthropicTool {
                    name,
                    description,
                    input_schema,
                })
            })
            .collect()
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn converse_stream(
        &self,
        _model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let (tx, rx) = mpsc::channel(100);

        let anthropic_messages = self.convert_messages(messages);
        let anthropic_tools = if tools.is_empty() {
            None
        } else {
            Some(self.convert_tools(tools))
        };

        let request = MessagesRequest {
            model: self.model_id.clone(),
            max_tokens: 16384,
            system: if system_prompt.is_empty() {
                None
            } else {
                Some(system_prompt.to_string())
            },
            messages: anthropic_messages,
            tools: anthropic_tools,
            stream: true,
        };

        let endpoint = format!("{}/messages", self.api_base.trim_end_matches('/'));

        let response = self
            .http_client
            .post(&endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AmosError::Internal(format!("Anthropic API request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AmosError::Internal(format!(
                "Anthropic API error {status}: {body}"
            )));
        }

        tokio::spawn(async move {
            if let Err(e) = parse_anthropic_sse(response, tx).await {
                error!("Anthropic SSE parse error: {:?}", e);
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
        let mut text_parts = Vec::new();
        let mut tool_uses = Vec::new();
        let mut usage = TokenUsage::default();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta(t) => text_parts.push(t),
                StreamEvent::ToolUse { id, name, input } => tool_uses.push((id, name, input)),
                StreamEvent::TokenUsage(u) => usage = u,
                StreamEvent::Stop => break,
                StreamEvent::Error(e) => return Err(AmosError::Internal(e)),
            }
        }

        let mut content = Vec::new();
        if !text_parts.is_empty() {
            content.push(ContentBlock::Text {
                text: text_parts.join(""),
            });
        }
        for (id, name, input) in tool_uses {
            content.push(ContentBlock::ToolUse { id, name, input });
        }

        Ok((
            Message {
                role: Role::Assistant,
                content,
                tool_use_id: None,
                timestamp: chrono::Utc::now(),
            },
            usage,
        ))
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SSE PARSER
// ═══════════════════════════════════════════════════════════════════════════

/// Parse Anthropic's SSE stream format.
///
/// Anthropic SSE events:
/// - `message_start` - contains initial usage
/// - `content_block_start` - text block or tool_use block start
/// - `content_block_delta` - text_delta or input_json_delta
/// - `content_block_stop` - block finished
/// - `message_delta` - final stop_reason + usage
/// - `message_stop` - stream complete
async fn parse_anthropic_sse(
    response: reqwest::Response,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<()> {
    use tokio_stream::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    // Track current tool_use blocks being assembled
    let mut current_tool_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut current_tool_json = String::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AmosError::Internal(format!("Stream read error: {e}")))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || line.starts_with("event:") {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<SseEvent>(data) {
                    match event.event_type.as_str() {
                        "message_start" => {
                            if let Some(msg) = &event.message {
                                if let Some(u) = &msg.usage {
                                    total_input_tokens = u.input_tokens;
                                }
                            }
                        }
                        "content_block_start" => {
                            if let Some(cb) = &event.content_block {
                                if cb.block_type == "tool_use" {
                                    current_tool_id = cb.id.clone();
                                    current_tool_name = cb.name.clone();
                                    current_tool_json.clear();
                                }
                            }
                        }
                        "content_block_delta" => {
                            if let Some(delta) = &event.delta {
                                match delta.delta_type.as_str() {
                                    "text_delta" => {
                                        if let Some(text) = &delta.text {
                                            if !text.is_empty()
                                                && tx
                                                    .send(StreamEvent::TextDelta(text.clone()))
                                                    .await
                                                    .is_err()
                                            {
                                                return Ok(());
                                            }
                                        }
                                    }
                                    "input_json_delta" => {
                                        if let Some(json_frag) = &delta.partial_json {
                                            current_tool_json.push_str(json_frag);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "content_block_stop" => {
                            // If we were assembling a tool_use, emit it now
                            if let (Some(id), Some(name)) =
                                (current_tool_id.take(), current_tool_name.take())
                            {
                                let input = serde_json::from_str(&current_tool_json)
                                    .unwrap_or(serde_json::json!({}));
                                current_tool_json.clear();
                                let _ = tx.send(StreamEvent::ToolUse { id, name, input }).await;
                            }
                        }
                        "message_delta" => {
                            if let Some(u) = &event.usage {
                                total_output_tokens = u.output_tokens;
                            }
                        }
                        "message_stop" => {
                            let _ = tx
                                .send(StreamEvent::TokenUsage(TokenUsage {
                                    input_tokens: total_input_tokens,
                                    output_tokens: total_output_tokens,
                                    total_tokens: total_input_tokens + total_output_tokens,
                                }))
                                .await;
                            let _ = tx.send(StreamEvent::Stop).await;
                            return Ok(());
                        }
                        "error" => {
                            let err_msg = data.to_string();
                            let _ = tx
                                .send(StreamEvent::Error(format!("Anthropic error: {err_msg}")))
                                .await;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Stream ended without message_stop
    let _ = tx.send(StreamEvent::Stop).await;
    Ok(())
}

/// Parse Anthropic SSE from a text body (for use by Vertex AI and other providers
/// that use the same response format but collect the body as text).
pub async fn parse_anthropic_sse_text(body: &str, tx: &mpsc::Sender<StreamEvent>) {
    let mut current_tool_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut current_tool_json = String::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("event:") {
            continue;
        }

        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<SseEvent>(data) {
                match event.event_type.as_str() {
                    "message_start" => {
                        if let Some(msg) = &event.message {
                            if let Some(u) = &msg.usage {
                                total_input_tokens = u.input_tokens;
                            }
                        }
                    }
                    "content_block_start" => {
                        if let Some(cb) = &event.content_block {
                            if cb.block_type == "tool_use" {
                                current_tool_id = cb.id.clone();
                                current_tool_name = cb.name.clone();
                                current_tool_json.clear();
                            }
                        }
                    }
                    "content_block_delta" => {
                        if let Some(delta) = &event.delta {
                            match delta.delta_type.as_str() {
                                "text_delta" => {
                                    if let Some(text) = &delta.text {
                                        if !text.is_empty() {
                                            let _ =
                                                tx.send(StreamEvent::TextDelta(text.clone())).await;
                                        }
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(json_frag) = &delta.partial_json {
                                        current_tool_json.push_str(json_frag);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "content_block_stop" => {
                        if let (Some(id), Some(name)) =
                            (current_tool_id.take(), current_tool_name.take())
                        {
                            let input = serde_json::from_str(&current_tool_json)
                                .unwrap_or(serde_json::json!({}));
                            current_tool_json.clear();
                            let _ = tx.send(StreamEvent::ToolUse { id, name, input }).await;
                        }
                    }
                    "message_delta" => {
                        if let Some(u) = &event.usage {
                            total_output_tokens = u.output_tokens;
                        }
                    }
                    "message_stop" => {
                        let _ = tx
                            .send(StreamEvent::TokenUsage(TokenUsage {
                                input_tokens: total_input_tokens,
                                output_tokens: total_output_tokens,
                                total_tokens: total_input_tokens + total_output_tokens,
                            }))
                            .await;
                        let _ = tx.send(StreamEvent::Stop).await;
                        return;
                    }
                    "error" => {
                        let _ = tx
                            .send(StreamEvent::Error(format!("API error: {data}")))
                            .await;
                        return;
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = tx.send(StreamEvent::Stop).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = AnthropicProvider::new(
            "https://api.anthropic.com/v1".to_string(),
            "test-key".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        );
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().provider_name(), "anthropic");
    }

    #[test]
    fn test_convert_tools() {
        let provider = AnthropicProvider::new(
            "https://api.anthropic.com/v1".to_string(),
            "test-key".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        )
        .unwrap();

        let tools = vec![serde_json::json!({
            "name": "think",
            "description": "Think about something",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thought": {"type": "string"}
                },
                "required": ["thought"]
            }
        })];

        let converted = provider.convert_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].name, "think");
        assert_eq!(converted[0].description, "Think about something");
    }

    #[test]
    fn test_convert_messages() {
        let provider = AnthropicProvider::new(
            "https://api.anthropic.com/v1".to_string(),
            "test-key".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        )
        .unwrap();

        let messages = vec![
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "Hello".to_string(),
                }],
                tool_use_id: None,
                timestamp: chrono::Utc::now(),
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "Hi there!".to_string(),
                }],
                tool_use_id: None,
                timestamp: chrono::Utc::now(),
            },
        ];

        let converted = provider.convert_messages(&messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }
}
