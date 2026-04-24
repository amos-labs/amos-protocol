//! AWS Bedrock client for LLM inference
//!
//! This module provides a client for AWS Bedrock's ConverseStream API with real HTTP streaming support.
//! Uses manual HTTP requests with AWS SigV4 signing to avoid the heavy aws-sdk-bedrockruntime dependency.

use amos_core::{types::Message, AmosError, Result};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

type HmacSha256 = Hmac<Sha256>;

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Events from the streaming API
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text delta (incremental text generation)
    TextDelta(String),

    /// Tool use request (id, name, input)
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Stream stopped
    Stop,

    /// Error occurred
    Error(String),

    /// Token usage information
    TokenUsage(TokenUsage),
}

/// Resolved AWS credentials
#[derive(Debug, Clone)]
pub(crate) struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub region: String,
}

/// AWS Bedrock client
#[derive(Clone)]
pub struct BedrockClient {
    region: String,
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    http_client: reqwest::Client,
}

/// Load AWS credentials using the standard credential chain:
/// 1. Explicit parameters
/// 2. Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
/// 3. AWS config/credentials files (~/.aws/credentials, ~/.aws/config)
///    - Respects AWS_PROFILE (defaults to "default")
///    - Respects AWS_SHARED_CREDENTIALS_FILE and AWS_CONFIG_FILE overrides
pub(crate) fn load_aws_credentials(
    region: Option<String>,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
) -> Result<AwsCredentials> {
    // --- Region resolution ---
    let resolved_region = region
        .or_else(|| std::env::var("AWS_REGION").ok())
        .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
        .or_else(|| read_aws_config_value("region"))
        .unwrap_or_else(|| "us-east-1".to_string());

    // --- Credential resolution ---

    // 1. Explicit params
    if let (Some(key), Some(secret)) = (access_key_id.clone(), secret_access_key.clone()) {
        debug!("Using explicitly provided AWS credentials");
        return Ok(AwsCredentials {
            access_key_id: key,
            secret_access_key: secret,
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
            region: resolved_region,
        });
    }

    // 2. Environment variables
    if let (Ok(key), Ok(secret)) = (
        std::env::var("AWS_ACCESS_KEY_ID"),
        std::env::var("AWS_SECRET_ACCESS_KEY"),
    ) {
        debug!("Using AWS credentials from environment variables");
        return Ok(AwsCredentials {
            access_key_id: key,
            secret_access_key: secret,
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
            region: resolved_region,
        });
    }

    // 3. ECS container credentials (task role).
    // ECS Fargate sets AWS_CONTAINER_CREDENTIALS_RELATIVE_URI automatically;
    // the metadata endpoint at 169.254.170.2 returns temporary credentials
    // tied to the task's IAM role (taskRoleArn).
    if let Ok(relative_uri) = std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") {
        match load_ecs_container_credentials(&relative_uri) {
            Ok((access_key, secret_key, session_token)) => {
                debug!("Using AWS credentials from ECS task role");
                return Ok(AwsCredentials {
                    access_key_id: access_key,
                    secret_access_key: secret_key,
                    session_token,
                    region: resolved_region,
                });
            }
            Err(e) => {
                warn!(
                    "ECS container credentials fetch failed, falling through: {}",
                    e
                );
            }
        }
    }

    // 4. AWS credentials file (~/.aws/credentials)
    if let Some(creds) = read_aws_credentials_file() {
        debug!("Using AWS credentials from ~/.aws/credentials");
        return Ok(AwsCredentials {
            access_key_id: creds.0,
            secret_access_key: creds.1,
            session_token: creds.2,
            region: resolved_region,
        });
    }

    Err(AmosError::Config(
        "AWS credentials not found. Checked: explicit params, environment variables \
         (AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY), ECS container credentials \
         (AWS_CONTAINER_CREDENTIALS_RELATIVE_URI), and AWS credentials file \
         (~/.aws/credentials). Please configure at least one credential source."
            .to_string(),
    ))
}

/// Load temporary credentials from the ECS container metadata endpoint.
/// Returns (access_key_id, secret_access_key, session_token).
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
    let session_token = body["Token"].as_str().map(|s| s.to_string());

    Ok((access_key, secret_key, session_token))
}

/// Read a value from ~/.aws/config for the active profile
fn read_aws_config_value(key: &str) -> Option<String> {
    let config_path = std::env::var("AWS_CONFIG_FILE")
        .ok()
        .or_else(|| dirs_path("/.aws/config"))?;

    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());
    read_ini_value(&config_path, &profile, key, true)
}

/// Read the AWS credentials file and return (access_key_id, secret_access_key, session_token)
fn read_aws_credentials_file() -> Option<(String, String, Option<String>)> {
    let creds_path = std::env::var("AWS_SHARED_CREDENTIALS_FILE")
        .ok()
        .or_else(|| dirs_path("/.aws/credentials"))?;

    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());

    let access_key = read_ini_value(&creds_path, &profile, "aws_access_key_id", false)?;
    let secret_key = read_ini_value(&creds_path, &profile, "aws_secret_access_key", false)?;
    let session_token = read_ini_value(&creds_path, &profile, "aws_session_token", false);

    Some((access_key, secret_key, session_token))
}

/// Get a path relative to the user's home directory
fn dirs_path(suffix: &str) -> Option<String> {
    std::env::var("HOME")
        .ok()
        .map(|home| format!("{}{}", home, suffix))
}

/// Parse an INI-style AWS config/credentials file.
/// `is_config` controls whether profile sections use "profile " prefix (config file) or not (credentials file).
fn read_ini_value(path: &str, profile: &str, key: &str, is_config: bool) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;

    // In config file, non-default profiles are "[profile foo]"; in credentials, they're "[foo]"
    let section_header = if is_config && profile != "default" {
        format!("[profile {}]", profile)
    } else {
        format!("[{}]", profile)
    };

    let mut in_section = false;

    for line in contents.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            in_section = trimmed == section_header;
            continue;
        }

        if in_section {
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }

    None
}

impl BedrockClient {
    /// Create a new Bedrock client
    ///
    /// Reads credentials using the standard AWS credential chain:
    /// 1. Explicit parameters (if provided)
    /// 2. Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
    /// 3. AWS config files (~/.aws/credentials, ~/.aws/config)
    ///    - Uses AWS_PROFILE env var to select profile (defaults to "default")
    pub fn new(
        region: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
    ) -> Result<Self> {
        let creds = load_aws_credentials(region, access_key_id, secret_access_key)?;

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AmosError::Internal(format!("Failed to build HTTP client: {}", e)))?;

        debug!(
            "Initialized BedrockClient for region: {} (profile: {})",
            creds.region,
            std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string())
        );

        Ok(Self {
            region: creds.region,
            access_key_id: creds.access_key_id,
            secret_access_key: creds.secret_access_key,
            session_token: creds.session_token,
            http_client,
        })
    }

    /// Stream a conversation with the model
    ///
    /// # Arguments
    ///
    /// * `model_id` - The Bedrock model ID (e.g., "anthropic.claude-3-5-sonnet-20241022-v2:0")
    /// * `system_prompt` - The system prompt
    /// * `messages` - The conversation history
    /// * `tools` - Available tool schemas
    ///
    /// # Returns
    ///
    /// A receiver channel that yields StreamEvents
    pub async fn converse_stream(
        &self,
        model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let (tx, rx) = mpsc::channel(100);

        // Build the request body
        let request_body = build_converse_request(model_id, system_prompt, messages, tools)?;
        let body_json = serde_json::to_string(&request_body)
            .map_err(|e| AmosError::Internal(format!("Failed to serialize request: {}", e)))?;

        // Build the endpoint URL — percent-encode the model ID since it may contain
        // special characters like `:` (e.g., "us.anthropic.claude-3-5-haiku-20241022-v1:0")
        // that must be encoded as `%3A` in the HTTP path for AWS to accept the signature.
        let encoded_model_id: String = model_id
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
                    format!("{}", b as char)
                } else {
                    format!("%{:02X}", b)
                }
            })
            .collect();
        let endpoint = format!(
            "https://bedrock-runtime.{}.amazonaws.com/model/{}/converse-stream",
            self.region, encoded_model_id
        );

        debug!("Bedrock endpoint: {}", endpoint);
        debug!("Request body length: {} bytes", body_json.len());

        // Sign the request
        let headers = self.sign_request("POST", &endpoint, &body_json).await?;

        // Clone values for the async task
        let http_client = self.http_client.clone();
        let body_json = body_json.clone();

        tokio::spawn(async move {
            match make_streaming_request(&http_client, &endpoint, headers, body_json).await {
                Ok(mut stream_rx) => {
                    while let Some(event) = stream_rx.recv().await {
                        if tx.send(event).await.is_err() {
                            debug!("Stream receiver dropped, stopping");
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to make streaming request: {:?}", e);
                    let _ = tx.send(StreamEvent::Error(format!("{:?}", e))).await;
                }
            }
        });

        Ok(rx)
    }

    /// Non-streaming conversation (synchronous response)
    pub async fn converse(
        &self,
        model_id: &str,
        system_prompt: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<(Message, TokenUsage)> {
        // Use streaming API and collect all events
        let mut stream_rx = self
            .converse_stream(model_id, system_prompt, messages, tools)
            .await?;

        let mut text_parts = Vec::new();
        let mut tool_uses = Vec::new();
        let mut usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
        };

        while let Some(event) = stream_rx.recv().await {
            match event {
                StreamEvent::TextDelta(text) => {
                    text_parts.push(text);
                }
                StreamEvent::ToolUse { id, name, input } => {
                    tool_uses.push((id, name, input));
                }
                StreamEvent::TokenUsage(u) => {
                    usage = u;
                }
                StreamEvent::Stop => break,
                StreamEvent::Error(e) => {
                    return Err(AmosError::Internal(format!("Stream error: {}", e)));
                }
            }
        }

        // Build the response message
        let mut content_blocks = Vec::new();
        if !text_parts.is_empty() {
            content_blocks.push(amos_core::types::ContentBlock::Text {
                text: text_parts.join(""),
            });
        }
        for (id, name, input) in tool_uses {
            content_blocks.push(amos_core::types::ContentBlock::ToolUse { id, name, input });
        }

        let response_message = Message {
            role: amos_core::types::Role::Assistant,
            content: content_blocks,
            tool_use_id: None,
            timestamp: Utc::now(),
        };

        Ok((response_message, usage))
    }

    /// Sign an HTTP request using AWS SigV4
    async fn sign_request(&self, method: &str, url: &str, body: &str) -> Result<HeaderMap> {
        let now = Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        // Parse URL
        let parsed_url = reqwest::Url::parse(url)
            .map_err(|e| AmosError::Internal(format!("Invalid URL: {}", e)))?;
        let host = parsed_url
            .host_str()
            .ok_or_else(|| AmosError::Internal("No host in URL".to_string()))?;
        // SigV4 requires the canonical URI to use percent-encoded path segments (RFC 3986).
        // `Url::path()` returns the decoded path, but we need the encoded form
        // (e.g., `:0` in model IDs must remain as `%3A0`).
        // Re-encode each path segment individually to get the correct canonical URI.
        let canonical_uri = {
            let segments: Vec<&str> = parsed_url
                .path_segments()
                .map(|segs| segs.collect())
                .unwrap_or_default();
            if segments.is_empty() {
                "/".to_string()
            } else {
                let encoded_segments: Vec<String> = segments
                    .iter()
                    .map(|seg| {
                        // Percent-encode per RFC 3986 (unreserved chars are NOT encoded)
                        seg.bytes()
                            .map(|b| {
                                if b.is_ascii_alphanumeric()
                                    || b == b'-'
                                    || b == b'.'
                                    || b == b'_'
                                    || b == b'~'
                                {
                                    format!("{}", b as char)
                                } else {
                                    format!("%{:02X}", b)
                                }
                            })
                            .collect::<String>()
                    })
                    .collect();
                format!("/{}", encoded_segments.join("/"))
            }
        };
        let canonical_querystring = parsed_url.query().unwrap_or("");

        // Hash the payload
        let payload_hash = format!("{:x}", Sha256::digest(body.as_bytes()));

        // Build canonical headers
        let mut canonical_headers_map = BTreeMap::new();
        canonical_headers_map.insert("content-type".to_string(), "application/json".to_string());
        canonical_headers_map.insert("host".to_string(), host.to_string());
        canonical_headers_map.insert("x-amz-date".to_string(), amz_date.clone());

        // Include security token in signed headers if present (for temporary/SSO credentials)
        if let Some(ref token) = self.session_token {
            canonical_headers_map.insert("x-amz-security-token".to_string(), token.clone());
        }

        let canonical_headers_str = canonical_headers_map
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        let signed_headers = canonical_headers_map
            .keys()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(";");

        // Build canonical request
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n\n{}\n{}",
            method,
            canonical_uri,
            canonical_querystring,
            canonical_headers_str,
            signed_headers,
            payload_hash
        );

        let canonical_request_hash = format!("{:x}", Sha256::digest(canonical_request.as_bytes()));

        // Build string to sign
        let service = "bedrock";
        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!("{}/{}/{}/aws4_request", date_stamp, self.region, service);

        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            algorithm, amz_date, credential_scope, canonical_request_hash
        );

        // Calculate signature
        let signature = calculate_signature(
            &self.secret_access_key,
            &date_stamp,
            &self.region,
            service,
            &string_to_sign,
        )?;

        // Build authorization header
        let authorization_header = format!(
            "{} Credential={}/{}, SignedHeaders={}, Signature={}",
            algorithm, self.access_key_id, credential_scope, signed_headers, signature
        );

        // Build headers
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            HeaderName::from_static("x-amz-date"),
            HeaderValue::from_str(&amz_date)
                .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
        );
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_str(&authorization_header)
                .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
        );
        headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_str(host)
                .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
        );

        // Include session token for temporary/SSO credentials
        if let Some(ref token) = self.session_token {
            headers.insert(
                HeaderName::from_static("x-amz-security-token"),
                HeaderValue::from_str(token)
                    .map_err(|e| AmosError::Internal(format!("Invalid header value: {}", e)))?,
            );
        }

        Ok(headers)
    }
}

impl Default for BedrockClient {
    fn default() -> Self {
        Self::new(None, None, None).expect("Failed to create default BedrockClient")
    }
}

/// Convert a MIME media type to the Bedrock image format string
fn media_type_to_format(media_type: &str) -> &str {
    match media_type {
        "image/jpeg" => "jpeg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png", // default fallback
    }
}

/// Sanitize a filename into a valid Bedrock document name.
///
/// The Bedrock Converse API requires document names to match
/// `[a-zA-Z0-9][a-zA-Z0-9._-]*` (no spaces, parens, or special chars).
fn sanitize_document_name(name: &str) -> String {
    // Strip extension and path
    let stem = std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");

    let sanitized: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Ensure starts with alphanumeric
    if sanitized.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        sanitized
    } else {
        format!("doc_{sanitized}")
    }
}

/// Build a Bedrock ConverseStream API request
fn build_converse_request(
    model_id: &str,
    system_prompt: &str,
    messages: &[Message],
    tools: &[serde_json::Value],
) -> Result<serde_json::Value> {
    // Scale maxTokens by model tier — tool calls with large payloads
    // (e.g. freeform canvas CSS/HTML) need more output room than 4096.
    // Haiku: 4096 (cheap, fast), Sonnet: 16384, Opus: 16384.
    let max_tokens = if model_id.contains("haiku") {
        4096
    } else {
        16384
    };

    let mut request = serde_json::json!({
        "inferenceConfig": {
            "maxTokens": max_tokens,
            "temperature": 0.7
        }
    });

    // Add system prompt
    if !system_prompt.is_empty() {
        request["system"] = serde_json::json!([{"text": system_prompt}]);
    }

    // Convert messages to Bedrock format.
    // Track document names across the entire conversation to avoid Bedrock's
    // "duplicate document names" error when the same file appears in history.
    let mut doc_name_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    let bedrock_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                amos_core::types::Role::User => "user",
                amos_core::types::Role::Assistant => "assistant",
                amos_core::types::Role::System => {
                    // System messages should be in the system field, not in messages
                    warn!("System role in messages array - skipping");
                    return serde_json::json!({
                        "role": "user",
                        "content": []
                    });
                }
                amos_core::types::Role::Tool => {
                    // Tool role should be represented as tool results
                    warn!("Tool role in messages array - skipping");
                    return serde_json::json!({
                        "role": "user",
                        "content": []
                    });
                }
            };

            let content: Vec<serde_json::Value> = msg
                .content
                .iter()
                .filter_map(|block| match block {
                    amos_core::types::ContentBlock::Text { text } => {
                        Some(serde_json::json!({"text": text}))
                    }
                    amos_core::types::ContentBlock::ToolUse { id, name, input } => {
                        Some(serde_json::json!({
                            "toolUse": {
                                "toolUseId": id,
                                "name": name,
                                "input": input
                            }
                        }))
                    }
                    amos_core::types::ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => Some(serde_json::json!({
                        "toolResult": {
                            "toolUseId": tool_use_id,
                            "content": [{"text": content}],
                            "status": if *is_error { "error" } else { "success" }
                        }
                    })),
                    amos_core::types::ContentBlock::Image { source } => Some(serde_json::json!({
                        "image": {
                            "format": media_type_to_format(&source.media_type),
                            "source": {
                                "bytes": source.data
                            }
                        }
                    })),
                    amos_core::types::ContentBlock::Document { source } => {
                        // Deduplicate document names across the entire conversation.
                        // Bedrock requires unique names; the same file in history
                        // would otherwise cause a 400 error.
                        let base_name = sanitize_document_name(&source.name);
                        let count = doc_name_counts.entry(base_name.clone()).or_insert(0);
                        *count += 1;
                        let unique_name = if *count == 1 {
                            base_name
                        } else {
                            format!("{}_{}", base_name, count)
                        };
                        Some(serde_json::json!({
                            "document": {
                                "format": source.format,
                                "name": unique_name,
                                "source": {
                                    "bytes": source.data
                                }
                            }
                        }))
                    }
                })
                .collect();

            serde_json::json!({
                "role": role,
                "content": content
            })
        })
        .collect();

    request["messages"] = serde_json::json!(bedrock_messages);

    // Add tool config if tools are provided
    if !tools.is_empty() {
        let tool_specs: Vec<serde_json::Value> = tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "toolSpec": tool
                })
            })
            .collect();

        request["toolConfig"] = serde_json::json!({
            "tools": tool_specs
        });
    }

    Ok(request)
}

/// Make a streaming HTTP request to Bedrock
async fn make_streaming_request(
    client: &reqwest::Client,
    url: &str,
    headers: HeaderMap,
    body: String,
) -> Result<mpsc::Receiver<StreamEvent>> {
    let (tx, rx) = mpsc::channel(100);

    let response = client
        .post(url)
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|e| AmosError::Internal(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        return Err(AmosError::Internal(format!(
            "Bedrock API error {}: {}",
            status, body
        )));
    }

    tokio::spawn(async move {
        if let Err(e) = parse_event_stream(response, tx).await {
            error!("Error parsing event stream: {:?}", e);
        }
    });

    Ok(rx)
}

/// Parse the Bedrock event stream (AWS binary Event Stream protocol)
///
/// AWS Event Stream wire format per message:
/// ```text
/// [4 bytes] total_byte_length (big-endian u32)
/// [4 bytes] headers_byte_length (big-endian u32)
/// [4 bytes] prelude CRC32
/// [N bytes] headers
/// [M bytes] payload (JSON)
/// [4 bytes] message CRC32
/// ```
///
/// Each header:
/// ```text
/// [1 byte]  name_length
/// [N bytes] name (UTF-8)
/// [1 byte]  value_type (7 = string)
/// [2 bytes] value_length (big-endian u16)
/// [N bytes] value (UTF-8)
/// ```
async fn parse_event_stream(
    response: reqwest::Response,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<()> {
    use tokio_stream::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buf = Vec::<u8>::new();

    // State for accumulating tool input JSON
    let mut current_tool_use_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut tool_input_buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AmosError::Internal(format!("Stream read error: {}", e)))?;
        buf.extend_from_slice(&chunk);

        // Process all complete messages in the buffer
        loop {
            // Need at least 12 bytes for the prelude (total_len + headers_len + prelude_crc)
            if buf.len() < 12 {
                break;
            }

            let total_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;

            // Wait until we have the full message
            if buf.len() < total_len {
                break;
            }

            let headers_len = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
            // prelude CRC is bytes 8..12 (skip, not validating)

            // Parse headers starting at offset 12
            let headers_start = 12;
            let headers_end = headers_start + headers_len;
            let headers = parse_event_headers(&buf[headers_start..headers_end]);

            // Payload is between headers end and message CRC (last 4 bytes)
            let payload_end = total_len - 4; // exclude trailing message CRC
            let payload = &buf[headers_end..payload_end];

            // Extract event type from headers
            let event_type = headers.get(":event-type").cloned();
            let message_type = headers.get(":message-type").cloned();

            // Handle exceptions
            if message_type.as_deref() == Some("exception") {
                let error_msg = String::from_utf8_lossy(payload).to_string();
                warn!("Bedrock stream exception: {}", error_msg);
                let _ = tx.send(StreamEvent::Error(error_msg)).await;
                buf.drain(..total_len);
                continue;
            }

            // Process event payload
            if let Some(ref et) = event_type {
                let payload_str = String::from_utf8_lossy(payload);
                if !payload_str.is_empty() {
                    match parse_event(
                        et,
                        &payload_str,
                        &mut current_tool_use_id,
                        &mut current_tool_name,
                        &mut tool_input_buffer,
                    ) {
                        Ok(Some(event)) => {
                            if tx.send(event).await.is_err() {
                                debug!("Stream receiver dropped");
                                return Ok(());
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            warn!("Failed to parse event {}: {:?}", et, e);
                        }
                    }
                }
            }

            // Consume this message from the buffer
            buf.drain(..total_len);
        }
    }

    let _ = tx.send(StreamEvent::Stop).await;
    Ok(())
}

/// Parse headers from the AWS Event Stream binary header block
fn parse_event_headers(data: &[u8]) -> std::collections::HashMap<String, String> {
    let mut headers = std::collections::HashMap::new();
    let mut pos = 0;

    while pos < data.len() {
        // Header name length (1 byte)
        if pos >= data.len() {
            break;
        }
        let name_len = data[pos] as usize;
        pos += 1;

        // Header name
        if pos + name_len > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[pos..pos + name_len]).to_string();
        pos += name_len;

        // Value type (1 byte) — 7 = string, 6 = bool, etc.
        if pos >= data.len() {
            break;
        }
        let value_type = data[pos];
        pos += 1;

        match value_type {
            7 => {
                // String: 2-byte length + value
                if pos + 2 > data.len() {
                    break;
                }
                let value_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                pos += 2;
                if pos + value_len > data.len() {
                    break;
                }
                let value = String::from_utf8_lossy(&data[pos..pos + value_len]).to_string();
                pos += value_len;
                headers.insert(name, value);
            }
            0 => {
                // Bool true
                headers.insert(name, "true".to_string());
            }
            1 => {
                // Bool false
                headers.insert(name, "false".to_string());
            }
            2 => {
                // Byte
                pos += 1;
            }
            3 => {
                // Short (2 bytes)
                pos += 2;
            }
            4 => {
                // Int (4 bytes)
                pos += 4;
            }
            5 => {
                // Long (8 bytes)
                pos += 8;
            }
            6 => {
                // Bytes: 2-byte length + value
                if pos + 2 > data.len() {
                    break;
                }
                let value_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                pos += 2;
                pos += value_len;
            }
            8 => {
                // Timestamp (8 bytes)
                pos += 8;
            }
            9 => {
                // UUID (16 bytes)
                pos += 16;
            }
            _ => {
                warn!("Unknown header value type: {}", value_type);
                break;
            }
        }
    }

    headers
}

/// Parse a single event from the stream
fn parse_event(
    event_type: &str,
    data: &str,
    current_tool_use_id: &mut Option<String>,
    current_tool_name: &mut Option<String>,
    tool_input_buffer: &mut String,
) -> Result<Option<StreamEvent>> {
    match event_type {
        "contentBlockStart" => {
            let json: serde_json::Value = serde_json::from_str(data)
                .map_err(|e| AmosError::Internal(format!("Failed to parse JSON: {}", e)))?;

            // Check if this is a tool use start
            if let Some(tool_use) = json["start"]["toolUse"].as_object() {
                *current_tool_use_id = tool_use["toolUseId"].as_str().map(|s| s.to_string());
                *current_tool_name = tool_use["name"].as_str().map(|s| s.to_string());
                *tool_input_buffer = String::new();
            }
            Ok(None)
        }
        "contentBlockDelta" => {
            let json: serde_json::Value = serde_json::from_str(data)
                .map_err(|e| AmosError::Internal(format!("Failed to parse JSON: {}", e)))?;

            // Text delta
            if let Some(text) = json["delta"]["text"].as_str() {
                return Ok(Some(StreamEvent::TextDelta(text.to_string())));
            }

            // Tool use input delta
            if let Some(input) = json["delta"]["toolUse"]["input"].as_str() {
                tool_input_buffer.push_str(input);
                return Ok(None); // Accumulate, don't send yet
            }

            Ok(None)
        }
        "contentBlockStop" => {
            // If we have accumulated tool input, parse and send it
            if let (Some(id), Some(name)) =
                (current_tool_use_id.as_ref(), current_tool_name.as_ref())
            {
                let input: serde_json::Value = serde_json::from_str(tool_input_buffer)
                    .unwrap_or_else(|e| {
                        warn!(
                            "Failed to parse tool input JSON: {}. Input: {}",
                            e, tool_input_buffer
                        );
                        serde_json::json!({})
                    });

                let event = StreamEvent::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                };

                // Reset tool state
                *current_tool_use_id = None;
                *current_tool_name = None;
                *tool_input_buffer = String::new();

                return Ok(Some(event));
            }
            Ok(None)
        }
        "messageStop" => Ok(Some(StreamEvent::Stop)),
        "metadata" => {
            let json: serde_json::Value = serde_json::from_str(data)
                .map_err(|e| AmosError::Internal(format!("Failed to parse JSON: {}", e)))?;

            if let Some(usage) = json["usage"].as_object() {
                let input_tokens = usage["inputTokens"].as_u64().unwrap_or(0);
                let output_tokens = usage["outputTokens"].as_u64().unwrap_or(0);
                let total_tokens = input_tokens + output_tokens;

                return Ok(Some(StreamEvent::TokenUsage(TokenUsage {
                    input_tokens,
                    output_tokens,
                    total_tokens,
                })));
            }

            Ok(None)
        }
        _ => {
            debug!("Unhandled event type: {}", event_type);
            Ok(None)
        }
    }
}

/// Calculate AWS SigV4 signature
pub(crate) fn calculate_signature(
    secret_key: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
    string_to_sign: &str,
) -> Result<String> {
    let k_secret = format!("AWS4{}", secret_key);
    let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, service.as_bytes())?;
    let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
    let signature = hmac_sha256(&k_signing, string_to_sign.as_bytes())?;

    Ok(hex::encode(signature))
}

/// HMAC-SHA256 helper
pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| AmosError::Internal(format!("HMAC error: {}", e)))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}
