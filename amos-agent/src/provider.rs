//! Model provider abstraction for the standalone agent.
//!
//! Re-exports the ModelProvider trait pattern from amos-harness but in a
//! lightweight form suitable for the agent binary. Supports Bedrock and
//! OpenAI-compatible providers.

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
// STREAM EVENTS (subset of amos-harness StreamEvent)
// ═══════════════════════════════════════════════════════════════════════════

/// Events emitted during LLM response streaming.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    TokenUsage(TokenUsage),
    Stop,
    Error(String),
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// TRAIT
// ═══════════════════════════════════════════════════════════════════════════

/// Unified interface for LLM providers.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Stream a conversation.
    async fn converse_stream(
        &self,
        model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<mpsc::Receiver<StreamEvent>>;

    /// Non-streaming conversation (collect full response).
    async fn converse(
        &self,
        model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<(Message, TokenUsage)>;

    fn provider_name(&self) -> &str;
}

// ═══════════════════════════════════════════════════════════════════════════
// OPENAI-COMPATIBLE PROVIDER
// ═══════════════════════════════════════════════════════════════════════════

/// Client for any OpenAI-compatible chat completions API.
pub struct OpenAiProvider {
    api_base: String,
    api_key: Option<String>,
    model_id: String,
    http_client: Client,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<OaiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OaiTool>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OaiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OaiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OaiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OaiFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OaiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OaiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OaiToolFunction,
}

#[derive(Debug, Serialize)]
struct OaiToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<OaiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ChunkToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ChunkToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ChunkFunction>,
}

#[derive(Debug, Deserialize)]
struct ChunkFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OaiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

impl OpenAiProvider {
    pub fn new(api_base: String, api_key: Option<String>, model_id: String) -> Result<Self> {
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

    fn convert_messages(&self, system_prompt: &str, messages: &[Message]) -> Vec<OaiMessage> {
        let mut oai = Vec::new();
        if !system_prompt.is_empty() {
            oai.push(OaiMessage {
                role: "system".to_string(),
                content: Some(system_prompt.to_string()),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        for msg in messages {
            match msg.role {
                Role::User => {
                    let mut text_parts = Vec::new();
                    let mut tool_results = Vec::new();
                    for block in &msg.content {
                        match block {
                            ContentBlock::Text { text } => text_parts.push(text.clone()),
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } => {
                                tool_results.push((tool_use_id.clone(), content.clone()));
                            }
                            _ => {}
                        }
                    }
                    for (id, content) in tool_results {
                        oai.push(OaiMessage {
                            role: "tool".to_string(),
                            content: Some(content),
                            tool_calls: None,
                            tool_call_id: Some(id),
                        });
                    }
                    if !text_parts.is_empty() {
                        oai.push(OaiMessage {
                            role: "user".to_string(),
                            content: Some(text_parts.join("\n")),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                }
                Role::Assistant => {
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();
                    for block in &msg.content {
                        match block {
                            ContentBlock::Text { text } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(OaiToolCall {
                                    id: id.clone(),
                                    call_type: "function".to_string(),
                                    function: OaiFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input).unwrap_or_default(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    oai.push(OaiMessage {
                        role: "assistant".to_string(),
                        content: if text_parts.is_empty() {
                            None
                        } else {
                            Some(text_parts.join("\n"))
                        },
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
                        },
                        tool_call_id: None,
                    });
                }
                _ => {}
            }
        }
        oai
    }

    fn convert_tools(&self, tools: &[serde_json::Value]) -> Vec<OaiTool> {
        tools
            .iter()
            .filter_map(|tool| {
                let name = tool["name"].as_str()?.to_string();
                let description = tool["description"].as_str().unwrap_or("").to_string();
                // Harness tools arrive with Bedrock's `{json: <schema>}`
                // envelope; agent-local tools arrive with the schema
                // directly. OpenAI wants the raw schema as `parameters` —
                // unwrap.
                let parameters = crate::tools::extract_tool_schema(tool);
                Some(OaiTool {
                    tool_type: "function".to_string(),
                    function: OaiToolFunction {
                        name,
                        description,
                        parameters,
                    },
                })
            })
            .collect()
    }
}

#[async_trait]
impl ModelProvider for OpenAiProvider {
    async fn converse_stream(
        &self,
        _model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let (tx, rx) = mpsc::channel(100);
        let oai_messages = self.convert_messages(system_prompt, messages);
        let oai_tools = if tools.is_empty() {
            None
        } else {
            Some(self.convert_tools(tools))
        };

        let request = ChatCompletionRequest {
            model: self.model_id.clone(),
            messages: oai_messages,
            tools: oai_tools,
            stream: true,
            max_tokens: Some(16384),
        };

        let endpoint = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));
        let mut req_builder = self.http_client.post(&endpoint);
        if let Some(ref key) = self.api_key {
            req_builder = req_builder.bearer_auth(key);
        }
        req_builder = req_builder.json(&request);

        let response = req_builder
            .send()
            .await
            .map_err(|e| AmosError::Internal(format!("API request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AmosError::Internal(format!("API error {status}: {body}")));
        }

        tokio::spawn(async move {
            if let Err(e) = parse_sse_stream(response, tx).await {
                error!("SSE parse error: {:?}", e);
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
        "openai_compatible"
    }
}

async fn parse_sse_stream(
    response: reqwest::Response,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<()> {
    use tokio_stream::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut tool_call_state: std::collections::HashMap<usize, (String, String, String)> =
        std::collections::HashMap::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AmosError::Internal(format!("Stream read error: {e}")))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_string();
            buffer = buffer[line_end + 1..].to_string();
            if line.is_empty() {
                continue;
            }

            if line == "data: [DONE]" {
                for (_idx, (id, name, args)) in tool_call_state.drain() {
                    let input = serde_json::from_str(&args).unwrap_or(serde_json::json!({}));
                    let _ = tx.send(StreamEvent::ToolUse { id, name, input }).await;
                }
                let _ = tx.send(StreamEvent::Stop).await;
                return Ok(());
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) {
                    if let Some(ref usage) = chunk.usage {
                        let _ = tx
                            .send(StreamEvent::TokenUsage(TokenUsage {
                                input_tokens: usage.prompt_tokens,
                                output_tokens: usage.completion_tokens,
                                total_tokens: usage.total_tokens,
                            }))
                            .await;
                    }
                    for choice in &chunk.choices {
                        if let Some(ref text) = choice.delta.content {
                            if !text.is_empty()
                                && tx.send(StreamEvent::TextDelta(text.clone())).await.is_err()
                            {
                                return Ok(());
                            }
                        }
                        if let Some(ref tcs) = choice.delta.tool_calls {
                            for tc in tcs {
                                let entry = tool_call_state.entry(tc.index).or_insert_with(|| {
                                    (
                                        tc.id.clone().unwrap_or_default(),
                                        String::new(),
                                        String::new(),
                                    )
                                });
                                if let Some(ref id) = tc.id {
                                    if !id.is_empty() {
                                        entry.0 = id.clone();
                                    }
                                }
                                if let Some(ref func) = tc.function {
                                    if let Some(ref name) = func.name {
                                        entry.1 = name.clone();
                                    }
                                    if let Some(ref args) = func.arguments {
                                        entry.2.push_str(args);
                                    }
                                }
                            }
                        }
                        if let Some(ref reason) = choice.finish_reason {
                            if reason == "tool_calls" {
                                for (_idx, (id, name, args)) in tool_call_state.drain() {
                                    let input = serde_json::from_str(&args)
                                        .unwrap_or(serde_json::json!({}));
                                    let _ = tx.send(StreamEvent::ToolUse { id, name, input }).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    for (_idx, (id, name, args)) in tool_call_state.drain() {
        let input = serde_json::from_str(&args).unwrap_or(serde_json::json!({}));
        let _ = tx.send(StreamEvent::ToolUse { id, name, input }).await;
    }
    let _ = tx.send(StreamEvent::Stop).await;
    Ok(())
}

/// No-op provider used as a placeholder when no default API key is configured.
/// Returns an error if called directly — BYOK per-request providers should be
/// used instead.
pub struct NoOpProvider;

#[async_trait]
impl ModelProvider for NoOpProvider {
    async fn converse_stream(
        &self,
        _model_id: &str,
        _system_prompt: &str,
        _messages: &[amos_core::types::Message],
        _tools: &[serde_json::Value],
    ) -> Result<tokio::sync::mpsc::Receiver<StreamEvent>> {
        Err(AmosError::Config(
            "No default LLM provider configured. Please configure a provider in Settings → LLM Providers."
                .to_string(),
        ))
    }

    async fn converse(
        &self,
        _model_id: &str,
        _system_prompt: &str,
        _messages: &[amos_core::types::Message],
        _tools: &[serde_json::Value],
    ) -> Result<(amos_core::types::Message, TokenUsage)> {
        Err(AmosError::Config(
            "No default LLM provider configured. Please configure a provider in Settings → LLM Providers."
                .to_string(),
        ))
    }

    fn provider_name(&self) -> &str {
        "none"
    }
}

/// Create a provider from config.
pub fn create_provider(
    provider_type: &str,
    model_id: &str,
    api_base: Option<&str>,
    api_key: Option<&str>,
) -> Result<Box<dyn ModelProvider>> {
    match provider_type {
        "anthropic" => {
            let base = api_base.unwrap_or("https://api.anthropic.com/v1");
            let key = api_key.ok_or_else(|| {
                AmosError::Config("Anthropic provider requires an API key".to_string())
            })?;
            Ok(Box::new(crate::anthropic::AnthropicProvider::new(
                base.to_string(),
                key.to_string(),
                model_id.to_string(),
            )?))
        }
        "openai" | "ollama" | "vllm" => {
            let base = api_base.unwrap_or("https://api.openai.com/v1");
            Ok(Box::new(OpenAiProvider::new(
                base.to_string(),
                api_key.map(|s| s.to_string()),
                model_id.to_string(),
            )?))
        }
        "bedrock" => Ok(Box::new(crate::bedrock::BedrockProvider::from_env()?)),
        "vertex" | "google-cloud" => {
            let base = api_base.unwrap_or("https://us-east5-aiplatform.googleapis.com");
            let token = api_key.ok_or_else(|| {
                AmosError::Config(
                    "Vertex AI provider requires an access token (api_key). Use `gcloud auth print-access-token`.".to_string(),
                )
            })?;
            Ok(Box::new(crate::vertex::VertexProvider::new(
                base.to_string(),
                token.to_string(),
                model_id.to_string(),
            )?))
        }
        _ => Err(AmosError::Config(format!(
            "Unknown model provider: '{}'. Supported: anthropic, openai, ollama, vllm, bedrock, vertex",
            provider_type
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_provider_openai() {
        let provider = create_provider(
            "openai",
            "gpt-4",
            Some("https://api.openai.com/v1"),
            Some("test-key"),
        );
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().provider_name(), "openai_compatible");
    }

    #[test]
    fn test_create_provider_bedrock() {
        // Bedrock provider creation depends on AWS credentials being available.
        // In CI without credentials, it will fail with a config error.
        // This test just verifies the code path runs without panicking.
        let _result = create_provider("bedrock", "claude-3", None, None);
    }

    #[test]
    fn test_create_provider_unknown() {
        let result = create_provider("unknown", "model", None, None);
        assert!(result.is_err());
    }
}
