use crate::provider::{ModelProvider, StreamEvent, TokenUsage};
use amos_core::{
    types::{ContentBlock, Message, Role},
    AmosError, Result,
};
use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::HeaderMap;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

type HmacSha256 = Hmac<Sha256>;

/// AWS Bedrock provider implementation
pub struct BedrockProvider {
    region: String,
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    client: reqwest::Client,
}

impl BedrockProvider {
    /// Create a new BedrockProvider with explicit credentials
    pub fn new(
        region: String,
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    ) -> Self {
        Self {
            region,
            access_key_id,
            secret_access_key,
            session_token,
            client: reqwest::Client::new(),
        }
    }

    /// Create a new BedrockProvider from environment variables and AWS config
    pub fn from_env() -> Result<Self> {
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| "us-east-1".to_string());

        // Try explicit environment variables first
        if let (Ok(access_key), Ok(secret_key)) = (
            std::env::var("AWS_ACCESS_KEY_ID"),
            std::env::var("AWS_SECRET_ACCESS_KEY"),
        ) {
            let session_token = std::env::var("AWS_SESSION_TOKEN").ok();
            return Ok(Self::new(region, access_key, secret_key, session_token));
        }

        // Try ECS container credentials (task role) via the metadata endpoint
        if let Ok(relative_uri) = std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") {
            if let Ok(creds) = load_ecs_container_credentials(&relative_uri) {
                return Ok(Self::new(region, creds.0, creds.1, creds.2));
            }
        }

        // Try loading from ~/.aws/credentials
        let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());
        if let Ok(credentials) = load_credentials_from_file(&profile) {
            return Ok(Self::new(
                region,
                credentials.0,
                credentials.1,
                credentials.2,
            ));
        }

        Err(AmosError::Config(
            "AWS credentials not found. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, configure ~/.aws/credentials, or run in ECS with a task role".to_string()
        ))
    }

    /// Sign a request using AWS SigV4
    fn sign_request(
        &self,
        method: &str,
        canonical_uri: &str,
        query_string: &str,
        headers: &HeaderMap,
        payload: &str,
        timestamp: &str,
    ) -> Result<String> {
        let date = &timestamp[0..8];
        let service = "bedrock";

        // Step 1: Create canonical request
        let mut sorted_headers: Vec<_> = headers
            .iter()
            .map(|(k, v)| (k.as_str().to_lowercase(), v.to_str().unwrap_or("").trim()))
            .collect();
        sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));

        let canonical_headers = sorted_headers
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        let signed_headers = sorted_headers
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>()
            .join(";");

        let payload_hash = hex::encode(Sha256::digest(payload.as_bytes()));

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n\n{}\n{}",
            method, canonical_uri, query_string, canonical_headers, signed_headers, payload_hash
        );

        debug!("Canonical request:\n{}", canonical_request);

        // Step 2: Create string to sign
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let credential_scope = format!("{}/{}/{}/aws4_request", date, self.region, service);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            timestamp, credential_scope, canonical_request_hash
        );

        debug!("String to sign:\n{}", string_to_sign);

        // Step 3: Calculate signature
        let k_date = hmac_sha256(
            format!("AWS4{}", self.secret_access_key).as_bytes(),
            date.as_bytes(),
        )?;
        let k_region = hmac_sha256(&k_date, self.region.as_bytes())?;
        let k_service = hmac_sha256(&k_region, service.as_bytes())?;
        let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes())?);

        // Step 4: Create authorization header
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key_id, credential_scope, signed_headers, signature
        );

        Ok(authorization)
    }

    /// Build the request body for Bedrock ConverseStream API.
    ///
    /// Public so `tests/bedrock_envelope.rs` can snapshot the exact JSON
    /// shape. The envelope is the regression surface Bedrock is strictest
    /// about (toolSpec wrapping, inputSchema.json envelope, inferenceConfig
    /// fields) — a golden-JSON contract test catches drift before deploy.
    pub fn build_request_body(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
        model_id: &str,
    ) -> Result<serde_json::Value> {
        let mut bedrock_messages = Vec::new();

        for msg in messages {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => {
                    // System messages are not sent as regular messages in Bedrock
                    warn!(
                        "System role in messages list - this should be in system_prompt parameter"
                    );
                    continue;
                }
                Role::Tool => {
                    // Tool role should not appear in bedrock messages
                    warn!("Tool role not supported for Bedrock messages");
                    continue;
                }
            };

            let mut content_blocks = Vec::new();
            for block in &msg.content {
                match block {
                    ContentBlock::Text { text } => {
                        content_blocks.push(serde_json::json!({
                            "text": text
                        }));
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        content_blocks.push(serde_json::json!({
                            "toolUse": {
                                "toolUseId": id,
                                "name": name,
                                "input": input
                            }
                        }));
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        let mut tool_result = serde_json::json!({
                            "toolUseId": tool_use_id,
                            "content": []
                        });

                        tool_result["status"] = if *is_error {
                            serde_json::json!("error")
                        } else {
                            serde_json::json!("success")
                        };

                        // Convert content string to content array
                        tool_result["content"] = serde_json::json!([{
                            "text": content
                        }]);

                        content_blocks.push(serde_json::json!({
                            "toolResult": tool_result
                        }));
                    }
                    ContentBlock::Image { .. } => {
                        warn!("Image content blocks not yet supported for Bedrock");
                    }
                    ContentBlock::Document { .. } => {
                        warn!("Document content blocks not yet supported for Bedrock");
                    }
                }
            }

            bedrock_messages.push(serde_json::json!({
                "role": role,
                "content": content_blocks
            }));
        }

        // Determine max tokens based on model
        let max_tokens = if model_id.contains("haiku") {
            4096
        } else {
            16384
        };

        let mut body = serde_json::json!({
            "messages": bedrock_messages,
            "inferenceConfig": {
                "maxTokens": max_tokens,
                "temperature": 1.0
            }
        });

        // Add system prompt if provided
        if !system_prompt.is_empty() {
            body["system"] = serde_json::json!([{
                "text": system_prompt
            }]);
        }

        // Add tools if provided.
        //
        // Bedrock's Converse API is strict about shape:
        //   1. Each tool must be wrapped in one of `toolSpec`, `systemTool`,
        //      `modelTool`, or `cachePoint`.
        //   2. The `toolSpec.inputSchema` must itself be `{"json": <schema>}`
        //      — a raw schema without the `json` envelope is rejected as
        //      "inputSchema is empty".
        //
        // Callers feed us tools in two shapes:
        //   - Agent-local tools (think, remember, plan, web_search, ...):
        //     `{name, description, inputSchema: <schema>}` — no json envelope.
        //   - Harness tools: `{name, description, inputSchema: {json: <schema>}}`
        //     — already enveloped by `get_tool_schemas` on the harness side.
        //
        // Other providers (Anthropic, OpenAI, Vertex) accept the raw form, so
        // we normalize here rather than force every caller to know Bedrock's
        // quirks. Tools already in toolSpec form pass through unchanged
        // (preserves the existing unit test).
        if !tools.is_empty() {
            let wrapped: Vec<serde_json::Value> =
                tools.iter().map(normalize_tool_for_bedrock).collect();
            body["toolConfig"] = serde_json::json!({ "tools": wrapped });
        }

        Ok(body)
    }

    /// Parse AWS Event Stream binary format
    fn parse_event_stream(&self, bytes: &[u8]) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();
        let mut offset = 0;

        while offset < bytes.len() {
            if bytes.len() - offset < 12 {
                // Not enough bytes for prelude
                break;
            }

            // Read prelude
            let total_len = u32::from_be_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) as usize;
            let headers_len = u32::from_be_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;
            let _prelude_crc = u32::from_be_bytes([
                bytes[offset + 8],
                bytes[offset + 9],
                bytes[offset + 10],
                bytes[offset + 11],
            ]);

            if offset + total_len > bytes.len() {
                // Not enough bytes for complete message
                break;
            }

            // Parse headers
            let headers_start = offset + 12;
            let headers_end = headers_start + headers_len;
            let headers = parse_headers(&bytes[headers_start..headers_end])?;

            // Parse payload
            let payload_start = headers_end;
            let payload_end = offset + total_len - 4; // Exclude message CRC
            let payload = &bytes[payload_start..payload_end];

            // Process event based on :event-type header
            if let Some(event_type) = headers.get(":event-type") {
                match event_type.as_str() {
                    "contentBlockStart" => {
                        if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(payload) {
                            debug!("contentBlockStart: {:?}", parsed);
                            if let Some(tool_use) =
                                parsed.get("start").and_then(|s| s.get("toolUse"))
                            {
                                let id = tool_use
                                    .get("toolUseId")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = tool_use
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                events.push(StreamEvent::ToolUse {
                                    id,
                                    name,
                                    input: serde_json::json!({}),
                                });
                            }
                        }
                    }
                    "contentBlockDelta" => {
                        if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(payload) {
                            if let Some(text) = parsed
                                .get("delta")
                                .and_then(|d| d.get("text"))
                                .and_then(|t| t.as_str())
                            {
                                events.push(StreamEvent::TextDelta(text.to_string()));
                            } else if let Some(tool_input) = parsed
                                .get("delta")
                                .and_then(|d| d.get("toolUse"))
                                .and_then(|t| t.get("input"))
                                .and_then(|i| i.as_str())
                            {
                                // Tool input comes as a JSON string that needs to be parsed
                                if let Ok(input_json) =
                                    serde_json::from_str::<serde_json::Value>(tool_input)
                                {
                                    // Update the last ToolUse event with the complete input
                                    if let Some(StreamEvent::ToolUse { input, .. }) =
                                        events.last_mut()
                                    {
                                        *input = input_json;
                                    }
                                }
                            }
                        }
                    }
                    "contentBlockStop" => {
                        debug!("contentBlockStop");
                    }
                    "messageStop" => {
                        events.push(StreamEvent::Stop);
                    }
                    "metadata" => {
                        if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(payload) {
                            if let Some(usage) = parsed.get("usage") {
                                let input_tokens = usage
                                    .get("inputTokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                let output_tokens = usage
                                    .get("outputTokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                events.push(StreamEvent::TokenUsage(TokenUsage {
                                    input_tokens,
                                    output_tokens,
                                    total_tokens: input_tokens + output_tokens,
                                }));
                            }
                        }
                    }
                    _ => {
                        debug!("Unknown event type: {}", event_type);
                    }
                }
            }

            offset += total_len;
        }

        Ok(events)
    }
}

#[async_trait]
impl ModelProvider for BedrockProvider {
    async fn converse_stream(
        &self,
        model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let (tx, rx) = mpsc::channel(100);

        // Percent-encode the model ID for the URL path
        let encoded_model_id = percent_encode(model_id);
        // The URL we send to reqwest — use percent-encoded model ID
        let url = format!(
            "https://bedrock-runtime.{}.amazonaws.com/model/{}/converse-stream",
            self.region, encoded_model_id
        );
        // For SigV4, the canonical URI must be URI-normalized.
        // AWS re-encodes percent-encoded chars in the canonical URI, so %3A becomes %253A.
        // We need the canonical URI to match what AWS will compute.
        let canonical_uri = format!(
            "/model/{}/converse-stream",
            percent_encode(&encoded_model_id)
        );

        // Build request body
        let body = self.build_request_body(system_prompt, messages, tools, model_id)?;
        let body_str = serde_json::to_string(&body)
            .map_err(|e| AmosError::Internal(format!("Failed to serialize request body: {}", e)))?;

        // Create timestamp
        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();

        // Build headers
        let mut headers = HeaderMap::new();
        headers.insert(
            "host",
            format!("bedrock-runtime.{}.amazonaws.com", self.region)
                .parse()
                .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
        );
        headers.insert(
            "content-type",
            "application/json"
                .parse()
                .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
        );
        headers.insert(
            "x-amz-date",
            timestamp
                .parse()
                .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
        );

        if let Some(token) = &self.session_token {
            headers.insert(
                "x-amz-security-token",
                token
                    .parse()
                    .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
            );
        }

        // Sign request
        let authorization =
            self.sign_request("POST", &canonical_uri, "", &headers, &body_str, &timestamp)?;

        headers.insert(
            "authorization",
            authorization
                .parse()
                .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
        );

        debug!("Sending request to: {}", url);
        debug!("Request body: {}", body_str);

        // Send request
        let client = self.client.clone();
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            let response = match client
                .post(&url)
                .headers(headers)
                .body(body_str)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Request failed: {}", e);
                    let _ = tx_clone
                        .send(StreamEvent::Error(format!("Request failed: {}", e)))
                        .await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let error_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                error!("Bedrock API error {}: {}", status, error_body);
                let _ = tx_clone
                    .send(StreamEvent::Error(format!(
                        "API error {}: {}",
                        status, error_body
                    )))
                    .await;
                return;
            }

            // Read streaming response
            let mut stream = response.bytes_stream();
            let mut buffer = Vec::new();

            while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
                match chunk {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);

                        // Try to parse complete events from buffer
                        // We need to be careful to only parse complete messages
                        let provider = BedrockProvider {
                            region: String::new(),
                            access_key_id: String::new(),
                            secret_access_key: String::new(),
                            session_token: None,
                            client: reqwest::Client::new(),
                        };

                        match provider.parse_event_stream(&buffer) {
                            Ok(events) => {
                                for event in events {
                                    if tx_clone.send(event).await.is_err() {
                                        return;
                                    }
                                }
                                // Clear buffer after successful parse
                                buffer.clear();
                            }
                            Err(e) => {
                                debug!("Parse error (may be incomplete): {}", e);
                                // Keep buffer for next chunk
                            }
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        let _ = tx_clone
                            .send(StreamEvent::Error(format!("Stream error: {}", e)))
                            .await;
                        return;
                    }
                }
            }

            debug!("Stream completed");
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

        let mut content_blocks = Vec::new();
        let mut current_text = String::new();
        let mut usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
        };

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta(text) => {
                    current_text.push_str(&text);
                }
                StreamEvent::ToolUse { id, name, input } => {
                    // Flush any accumulated text
                    if !current_text.is_empty() {
                        content_blocks.push(ContentBlock::Text {
                            text: std::mem::take(&mut current_text),
                        });
                    }
                    content_blocks.push(ContentBlock::ToolUse { id, name, input });
                }
                StreamEvent::TokenUsage(u) => {
                    usage = u;
                }
                StreamEvent::Stop => {
                    break;
                }
                StreamEvent::Error(e) => {
                    return Err(AmosError::Internal(format!("Stream error: {}", e)));
                }
            }
        }

        // Flush any remaining text
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
        "bedrock"
    }
}

// Helper functions

/// Normalize a tool definition into Bedrock Converse's required shape.
///
/// Bedrock wants `{toolSpec: {name, description, inputSchema: {json: <schema>}}}`.
/// We accept three input shapes and coerce to that:
///   1. Already wrapped in `toolSpec`/`systemTool`/`modelTool`/`cachePoint`
///      — passed through unchanged.
///   2. Raw harness form — `{name, description, inputSchema: {json: ...}}` —
///      only needs the outer `toolSpec` wrapper.
///   3. Raw agent-local form — `{name, description, inputSchema: <schema>}` —
///      needs both the `toolSpec` wrapper AND an inner `{json: ...}` envelope
///      around the schema.
fn normalize_tool_for_bedrock(t: &serde_json::Value) -> serde_json::Value {
    if t.get("toolSpec").is_some()
        || t.get("systemTool").is_some()
        || t.get("modelTool").is_some()
        || t.get("cachePoint").is_some()
    {
        return t.clone();
    }

    let mut obj = t.as_object().cloned().unwrap_or_default();

    // If inputSchema is present but missing the `json` envelope, add it.
    if let Some(input_schema) = obj.get("inputSchema").cloned() {
        if input_schema.get("json").is_none() {
            obj.insert(
                "inputSchema".to_string(),
                serde_json::json!({ "json": input_schema }),
            );
        }
    } else {
        // Schema missing entirely — Bedrock rejects empty, so default to
        // a valid "any object" schema.
        obj.insert(
            "inputSchema".to_string(),
            serde_json::json!({ "json": { "type": "object" } }),
        );
    }

    serde_json::json!({ "toolSpec": obj })
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| AmosError::Internal(format!("HMAC error: {}", e)))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn percent_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

fn parse_headers(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let mut headers = BTreeMap::new();
    let mut offset = 0;

    while offset < bytes.len() {
        if bytes.len() - offset < 2 {
            break;
        }

        // Read name length
        let name_len = bytes[offset] as usize;
        offset += 1;

        if bytes.len() - offset < name_len {
            break;
        }

        // Read name
        let name = String::from_utf8_lossy(&bytes[offset..offset + name_len]).to_string();
        offset += name_len;

        if bytes.len() - offset < 1 {
            break;
        }

        // Read value type
        let value_type = bytes[offset];
        offset += 1;

        // Parse value based on type
        let value = match value_type {
            7 => {
                // String
                if bytes.len() - offset < 2 {
                    break;
                }
                let value_len = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                offset += 2;

                if bytes.len() - offset < value_len {
                    break;
                }

                let value_str =
                    String::from_utf8_lossy(&bytes[offset..offset + value_len]).to_string();
                offset += value_len;
                value_str
            }
            _ => {
                debug!("Unsupported header value type: {}", value_type);
                continue;
            }
        };

        headers.insert(name, value);
    }

    Ok(headers)
}

/// Load credentials from ECS container metadata endpoint (task role).
/// ECS Fargate sets AWS_CONTAINER_CREDENTIALS_RELATIVE_URI automatically.
fn load_ecs_container_credentials(relative_uri: &str) -> Result<(String, String, Option<String>)> {
    let url = format!("http://169.254.170.2{}", relative_uri);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AmosError::Internal(format!("HTTP client error: {}", e)))?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| AmosError::Internal(format!("ECS metadata request failed: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AmosError::Internal(format!(
            "ECS metadata returned status {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .map_err(|e| AmosError::Internal(format!("ECS metadata parse error: {}", e)))?;

    let access_key = body["AccessKeyId"]
        .as_str()
        .ok_or_else(|| AmosError::Internal("Missing AccessKeyId in ECS metadata".to_string()))?
        .to_string();
    let secret_key = body["SecretAccessKey"]
        .as_str()
        .ok_or_else(|| AmosError::Internal("Missing SecretAccessKey in ECS metadata".to_string()))?
        .to_string();
    let session_token = body["Token"].as_str().map(|s: &str| s.to_string());

    tracing::info!("Loaded AWS credentials from ECS task role");
    Ok((access_key, secret_key, session_token))
}

fn load_credentials_from_file(profile: &str) -> Result<(String, String, Option<String>)> {
    let home = std::env::var("HOME")
        .map_err(|_| AmosError::Config("HOME environment variable not set".to_string()))?;
    let credentials_path = format!("{}/.aws/credentials", home);

    let contents = std::fs::read_to_string(&credentials_path)
        .map_err(|e| AmosError::Config(format!("Failed to read {}: {}", credentials_path, e)))?;

    let mut in_profile = false;
    let mut access_key: Option<String> = None;
    let mut secret_key: Option<String> = None;
    let mut session_token: Option<String> = None;

    for line in contents.lines() {
        let line = line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            let profile_name = &line[1..line.len() - 1];
            in_profile = profile_name == profile;
            continue;
        }

        if !in_profile {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "aws_access_key_id" => access_key = Some(value.to_string()),
                "aws_secret_access_key" => secret_key = Some(value.to_string()),
                "aws_session_token" => session_token = Some(value.to_string()),
                _ => {}
            }
        }
    }

    match (access_key, secret_key) {
        (Some(access), Some(secret)) => Ok((access, secret, session_token)),
        _ => Err(AmosError::Config(format!(
            "Profile '{}' not found or incomplete in {}",
            profile, credentials_path
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_encode() {
        assert_eq!(percent_encode("hello"), "hello");
        assert_eq!(percent_encode("hello:world"), "hello%3Aworld");
        assert_eq!(
            percent_encode("anthropic.claude-sonnet-4-20250514-v1:0"),
            "anthropic.claude-sonnet-4-20250514-v1%3A0"
        );
    }

    #[test]
    fn test_build_request_body_simple() {
        let provider = BedrockProvider::new(
            "us-east-1".to_string(),
            "test_key".to_string(),
            "test_secret".to_string(),
            None,
        );

        let messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
            tool_use_id: None,
            timestamp: Utc::now(),
        }];

        let body = provider
            .build_request_body("", &messages, &[], "anthropic.claude-sonnet-4")
            .unwrap();

        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["text"], "Hello");
        assert_eq!(body["inferenceConfig"]["maxTokens"], 16384);
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = BedrockProvider::new(
            "us-east-1".to_string(),
            "test_key".to_string(),
            "test_secret".to_string(),
            None,
        );

        let messages = vec![
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "What's the weather?".to_string(),
                }],
                tool_use_id: None,
                timestamp: Utc::now(),
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tool_123".to_string(),
                    name: "get_weather".to_string(),
                    input: serde_json::json!({"city": "San Francisco"}),
                }],
                tool_use_id: None,
                timestamp: Utc::now(),
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool_123".to_string(),
                    content: "Sunny, 72°F".to_string(),
                    is_error: false,
                }],
                tool_use_id: None,
                timestamp: Utc::now(),
            },
        ];

        let tools = vec![serde_json::json!({
            "toolSpec": {
                "name": "get_weather",
                "description": "Get weather",
                "inputSchema": {
                    "json": {
                        "type": "object",
                        "properties": {
                            "city": {"type": "string"}
                        }
                    }
                }
            }
        })];

        let body = provider
            .build_request_body("", &messages, &tools, "anthropic.claude-haiku")
            .unwrap();

        assert_eq!(body["messages"].as_array().unwrap().len(), 3);
        assert_eq!(body["toolConfig"]["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["inferenceConfig"]["maxTokens"], 4096); // haiku
    }

    #[test]
    fn test_hmac_sha256() {
        let result = hmac_sha256(b"key", b"data").unwrap();
        assert!(!result.is_empty());
        assert_eq!(result.len(), 32); // SHA256 produces 32 bytes
    }

    // ── Tool normalization (Bedrock Converse shape quirks) ──────────

    #[test]
    fn normalize_passthrough_when_already_toolspec() {
        let input = serde_json::json!({
            "toolSpec": { "name": "x", "description": "y", "inputSchema": {"json": {"type": "object"}} }
        });
        let out = normalize_tool_for_bedrock(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn normalize_adds_toolspec_and_json_envelope_for_agent_local_shape() {
        // Agent-local tools: `{name, description, inputSchema: <schema>}`
        let input = serde_json::json!({
            "name": "think",
            "description": "internal reasoning",
            "inputSchema": { "type": "object", "properties": { "thought": { "type": "string" } } }
        });
        let out = normalize_tool_for_bedrock(&input);
        let spec = out.get("toolSpec").expect("outer toolSpec wrapper");
        let input_schema = spec.get("inputSchema").expect("inputSchema present");
        let json_env = input_schema
            .get("json")
            .expect("inputSchema.json envelope present");
        assert_eq!(json_env.get("type"), Some(&serde_json::json!("object")));
    }

    #[test]
    fn normalize_adds_toolspec_only_when_harness_shape_already_has_json() {
        // Harness tools: `{name, description, inputSchema: {json: <schema>}}`
        let input = serde_json::json!({
            "name": "harness_foo",
            "description": "a harness tool",
            "inputSchema": { "json": { "type": "object" } }
        });
        let out = normalize_tool_for_bedrock(&input);
        let spec = out.get("toolSpec").expect("outer toolSpec wrapper");
        // Should NOT have double-wrapped json
        let json_env = spec
            .get("inputSchema")
            .and_then(|s| s.get("json"))
            .expect("inputSchema.json envelope present");
        assert!(json_env.get("json").is_none(), "must not double-wrap json");
    }

    #[test]
    fn normalize_supplies_default_schema_when_missing() {
        let input = serde_json::json!({
            "name": "weirdtool",
            "description": "no schema"
        });
        let out = normalize_tool_for_bedrock(&input);
        let spec = out.get("toolSpec").expect("outer toolSpec wrapper");
        let json_env = spec
            .get("inputSchema")
            .and_then(|s| s.get("json"))
            .expect("inputSchema.json envelope present");
        assert_eq!(json_env.get("type"), Some(&serde_json::json!("object")));
    }

    #[tokio::test]
    async fn test_credential_loading() {
        // Test that credential loading doesn't panic
        // Actual credential loading is environment-dependent
        let result = BedrockProvider::from_env();
        // We don't assert success/failure since it depends on the environment
        // Just ensure it doesn't panic
        let _ = result;
    }
}
