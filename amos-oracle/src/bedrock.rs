//! AWS Bedrock LLM client for Oracle.
//!
//! Minimal implementation of [`LlmClient`] backed by the AWS Bedrock Converse
//! API. Purpose-built for Oracle's single-turn, structured-JSON-output use
//! case — no streaming, no tools, no images.
//!
//! SigV4 signing + credential loading adapted from `amos-harness/src/bedrock.rs`
//! (the full-featured client used by the agent chat flow). If the two ever
//! need to stay in lockstep, extract a shared `amos-bedrock` crate. For now
//! the patterns are duplicated but isolated — Oracle's transitive deps stay
//! lean.
//!
//! ## Credential resolution
//!
//! Standard AWS chain:
//! 1. Explicit params to [`BedrockLlmClient::new`]
//! 2. `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` + `AWS_REGION` env vars
//! 3. `~/.aws/credentials` + `~/.aws/config` (respects `AWS_PROFILE`)
//!
//! ## Model selection
//!
//! The model ID is set at construction. Oracle defaults to Claude Opus
//! (`anthropic.claude-opus-4-20250514-v1:0`) — highest-capability model for
//! mission-alignment reasoning. Override via env for cost-tier work.

use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tracing::{debug, warn};

use crate::llm::LlmClient;
use crate::{OracleError, Result};

type HmacSha256 = Hmac<Sha256>;

/// Default model — highest-capability Claude for mission reasoning.
pub const DEFAULT_MODEL_ID: &str = "anthropic.claude-opus-4-20250514-v1:0";

#[derive(Clone)]
pub struct BedrockLlmClient {
    region: String,
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    model_id: String,
    http: reqwest::Client,
    /// Temperature — Oracle uses 0.0 for determinism. Exposed for tests.
    temperature: f64,
    /// Max output tokens. 16k is plenty for a Decision JSON.
    max_tokens: u32,
}

impl BedrockLlmClient {
    /// Construct with the default Oracle model + determinism settings.
    ///
    /// Credentials + region resolved via the standard AWS chain.
    pub fn new(
        region: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        model_id: Option<String>,
    ) -> Result<Self> {
        let creds = load_aws_credentials(region, access_key_id, secret_access_key)?;

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| OracleError::Llm(format!("build http client: {}", e)))?;

        Ok(Self {
            region: creds.region,
            access_key_id: creds.access_key_id,
            secret_access_key: creds.secret_access_key,
            session_token: creds.session_token,
            model_id: model_id.unwrap_or_else(|| DEFAULT_MODEL_ID.to_string()),
            http,
            temperature: 0.0,
            max_tokens: 16_384,
        })
    }

    pub fn with_temperature(mut self, t: f64) -> Self {
        self.temperature = t.clamp(0.0, 1.0);
        self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }
}

#[async_trait]
impl LlmClient for BedrockLlmClient {
    async fn complete(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        let request_body = serde_json::json!({
            "inferenceConfig": {
                "maxTokens": self.max_tokens,
                "temperature": self.temperature,
            },
            "system": [{ "text": system_prompt }],
            "messages": [{
                "role": "user",
                "content": [{ "text": user_message }],
            }],
        });
        let body_json = serde_json::to_string(&request_body)?;

        let encoded_model_id: String = percent_encode(&self.model_id);
        let endpoint = format!(
            "https://bedrock-runtime.{}.amazonaws.com/model/{}/converse",
            self.region, encoded_model_id
        );

        debug!(model = %self.model_id, region = %self.region, "bedrock converse");

        let headers = self.sign_request("POST", &endpoint, &body_json)?;

        let resp = self
            .http
            .post(&endpoint)
            .headers(headers)
            .body(body_json)
            .send()
            .await
            .map_err(|e| OracleError::Llm(format!("POST failed: {}", e)))?;

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| OracleError::Llm(format!("read body: {}", e)))?;

        if !status.is_success() {
            return Err(OracleError::Llm(format!(
                "bedrock returned {}: {}",
                status,
                body_text.chars().take(500).collect::<String>()
            )));
        }

        extract_text_from_converse_response(&body_text)
    }

    fn model_version(&self) -> String {
        format!("bedrock:{}", self.model_id)
    }
}

impl BedrockLlmClient {
    fn sign_request(&self, method: &str, url: &str, body: &str) -> Result<HeaderMap> {
        let now = Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        let parsed = reqwest::Url::parse(url)
            .map_err(|e| OracleError::Llm(format!("invalid url: {}", e)))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| OracleError::Llm("url has no host".into()))?;

        // Canonical URI: re-encode path segments per RFC 3986 (model IDs
        // contain `:` which must stay as `%3A` in the signed path).
        let canonical_uri = {
            let segs: Vec<&str> = parsed
                .path_segments()
                .map(|s| s.collect())
                .unwrap_or_default();
            if segs.is_empty() {
                "/".to_string()
            } else {
                format!(
                    "/{}",
                    segs.iter()
                        .map(|s| percent_encode(s))
                        .collect::<Vec<_>>()
                        .join("/")
                )
            }
        };
        let canonical_querystring = parsed.query().unwrap_or("");

        let payload_hash = format!("{:x}", Sha256::digest(body.as_bytes()));

        let mut headers_map = BTreeMap::new();
        headers_map.insert("content-type".to_string(), "application/json".to_string());
        headers_map.insert("host".to_string(), host.to_string());
        headers_map.insert("x-amz-date".to_string(), amz_date.clone());
        if let Some(ref token) = self.session_token {
            headers_map.insert("x-amz-security-token".to_string(), token.clone());
        }

        let canonical_headers_str = headers_map
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join("\n");
        let signed_headers = headers_map.keys().cloned().collect::<Vec<_>>().join(";");

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

        let service = "bedrock";
        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!("{}/{}/{}/aws4_request", date_stamp, self.region, service);

        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            algorithm, amz_date, credential_scope, canonical_request_hash
        );

        let signature = calculate_signature(
            &self.secret_access_key,
            &date_stamp,
            &self.region,
            service,
            &string_to_sign,
        )?;

        let auth_header = format!(
            "{} Credential={}/{}, SignedHeaders={}, Signature={}",
            algorithm, self.access_key_id, credential_scope, signed_headers, signature
        );

        let mut out = HeaderMap::new();
        out.insert("content-type", HeaderValue::from_static("application/json"));
        out.insert(
            HeaderName::from_static("x-amz-date"),
            HeaderValue::from_str(&amz_date)
                .map_err(|e| OracleError::Llm(format!("header amz-date: {}", e)))?,
        );
        out.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_str(&auth_header)
                .map_err(|e| OracleError::Llm(format!("header authorization: {}", e)))?,
        );
        out.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_str(host)
                .map_err(|e| OracleError::Llm(format!("header host: {}", e)))?,
        );
        if let Some(ref token) = self.session_token {
            out.insert(
                HeaderName::from_static("x-amz-security-token"),
                HeaderValue::from_str(token)
                    .map_err(|e| OracleError::Llm(format!("header token: {}", e)))?,
            );
        }

        Ok(out)
    }
}

// ── SigV4 primitives ──────────────────────────────────────────────────────

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| OracleError::Llm(format!("hmac init: {}", e)))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn calculate_signature(
    secret: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
    string_to_sign: &str,
) -> Result<String> {
    let k_date = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date_stamp.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, service.as_bytes())?;
    let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
    let signature = hmac_sha256(&k_signing, string_to_sign.as_bytes())?;
    Ok(signature
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>())
}

/// Percent-encode per RFC 3986 (unreserved chars pass through).
fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
                (b as char).to_string()
            } else {
                format!("%{:02X}", b)
            }
        })
        .collect()
}

// ── Response parsing ──────────────────────────────────────────────────────

/// Extract the text of the first content block in a Bedrock converse response.
///
/// Response shape:
/// ```json
/// {
///   "output": {
///     "message": {
///       "role": "assistant",
///       "content": [{ "text": "..." }]
///     }
///   },
///   "stopReason": "...",
///   "usage": { ... }
/// }
/// ```
fn extract_text_from_converse_response(body: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| OracleError::Llm(format!("response not JSON: {} (body: {})", e, body)))?;

    let content = v
        .get("output")
        .and_then(|o| o.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
        .ok_or_else(|| OracleError::Llm(format!("unexpected response shape: {}", body)))?;

    let mut combined = String::new();
    for block in content {
        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
            combined.push_str(text);
        }
    }

    if combined.is_empty() {
        return Err(OracleError::Llm(format!(
            "response had no text content: {}",
            body
        )));
    }

    Ok(combined)
}

// ── Credential loading ────────────────────────────────────────────────────

struct AwsCredentials {
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    region: String,
}

fn load_aws_credentials(
    region: Option<String>,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
) -> Result<AwsCredentials> {
    let region = region
        .or_else(|| std::env::var("AWS_REGION").ok())
        .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
        .or_else(|| read_aws_config_value("region"))
        .unwrap_or_else(|| "us-east-1".to_string());

    // 1. Explicit params
    if let (Some(key), Some(secret)) = (access_key_id.clone(), secret_access_key.clone()) {
        return Ok(AwsCredentials {
            access_key_id: key,
            secret_access_key: secret,
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
            region,
        });
    }

    // 2. Environment variables
    if let (Ok(key), Ok(secret)) = (
        std::env::var("AWS_ACCESS_KEY_ID"),
        std::env::var("AWS_SECRET_ACCESS_KEY"),
    ) {
        return Ok(AwsCredentials {
            access_key_id: key,
            secret_access_key: secret,
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
            region,
        });
    }

    // 3. ECS container credentials (Fargate task role).
    // ECS sets AWS_CONTAINER_CREDENTIALS_RELATIVE_URI; the metadata endpoint
    // at 169.254.170.2 returns short-lived creds tied to the task role.
    if let Ok(relative_uri) = std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") {
        match load_ecs_container_credentials(&relative_uri) {
            Ok((key, secret, token)) => {
                return Ok(AwsCredentials {
                    access_key_id: key,
                    secret_access_key: secret,
                    session_token: token,
                    region,
                });
            }
            Err(e) => {
                warn!(error = %e, "ECS container credentials fetch failed; falling through");
            }
        }
    }

    // 4. ~/.aws/credentials (profile-based)
    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());
    if let Some(creds) = read_aws_credentials_file(&profile) {
        return Ok(AwsCredentials { region, ..creds });
    }

    warn!(
        "no AWS credentials found (env vars unset, ECS metadata unavailable, ~/.aws/credentials missing profile '{}')",
        profile
    );
    Err(OracleError::Llm(format!(
        "no AWS credentials found for profile '{}'; set AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY, run inside ECS with a task role, or configure ~/.aws/credentials",
        profile
    )))
}

/// Fetch short-lived credentials from the ECS container metadata endpoint.
/// Returns `(access_key_id, secret_access_key, session_token)`.
fn load_ecs_container_credentials(relative_uri: &str) -> Result<(String, String, Option<String>)> {
    let url = format!("http://169.254.170.2{}", relative_uri);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| OracleError::Llm(format!("ECS http client: {}", e)))?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| OracleError::Llm(format!("ECS metadata request failed: {}", e)))?;

    if !resp.status().is_success() {
        return Err(OracleError::Llm(format!(
            "ECS metadata returned {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .map_err(|e| OracleError::Llm(format!("ECS metadata parse: {}", e)))?;

    let key = body["AccessKeyId"]
        .as_str()
        .ok_or_else(|| OracleError::Llm("ECS metadata missing AccessKeyId".into()))?
        .to_string();
    let secret = body["SecretAccessKey"]
        .as_str()
        .ok_or_else(|| OracleError::Llm("ECS metadata missing SecretAccessKey".into()))?
        .to_string();
    let token = body["Token"].as_str().map(|s| s.to_string());

    Ok((key, secret, token))
}

fn credentials_file_path() -> Option<std::path::PathBuf> {
    if let Ok(explicit) = std::env::var("AWS_SHARED_CREDENTIALS_FILE") {
        return Some(std::path::PathBuf::from(explicit));
    }
    let home = std::env::var("HOME").ok()?;
    Some(
        std::path::PathBuf::from(home)
            .join(".aws")
            .join("credentials"),
    )
}

fn config_file_path() -> Option<std::path::PathBuf> {
    if let Ok(explicit) = std::env::var("AWS_CONFIG_FILE") {
        return Some(std::path::PathBuf::from(explicit));
    }
    let home = std::env::var("HOME").ok()?;
    Some(std::path::PathBuf::from(home).join(".aws").join("config"))
}

fn read_aws_credentials_file(profile: &str) -> Option<AwsCredentials> {
    let path = credentials_file_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    parse_ini_profile(&content, profile).map(|(key, secret, token)| AwsCredentials {
        access_key_id: key,
        secret_access_key: secret,
        session_token: token,
        region: String::new(), // filled in by caller
    })
}

fn read_aws_config_value(key: &str) -> Option<String> {
    let path = config_file_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());
    // Config file profiles are "[profile NAME]" except "[default]"
    let section_header = if profile == "default" {
        "[default]".to_string()
    } else {
        format!("[profile {}]", profile)
    };

    let mut in_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == section_header;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn parse_ini_profile(content: &str, profile: &str) -> Option<(String, String, Option<String>)> {
    let header = format!("[{}]", profile);
    let mut in_section = false;
    let mut key = None;
    let mut secret = None;
    let mut token = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == header;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            match k.trim() {
                "aws_access_key_id" => key = Some(v.trim().to_string()),
                "aws_secret_access_key" => secret = Some(v.trim().to_string()),
                "aws_session_token" => token = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }

    match (key, secret) {
        (Some(k), Some(s)) => Some((k, s, token)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_preserves_unreserved() {
        assert_eq!(percent_encode("abc123-._~"), "abc123-._~");
    }

    #[test]
    fn percent_encode_encodes_reserved() {
        assert_eq!(percent_encode(":"), "%3A");
        assert_eq!(percent_encode("/"), "%2F");
        assert_eq!(percent_encode("="), "%3D");
    }

    #[test]
    fn percent_encode_matches_bedrock_model_id_pattern() {
        // `anthropic.claude-opus-4-20250514-v1:0` — the colon must encode.
        assert_eq!(
            percent_encode("anthropic.claude-opus-4-20250514-v1:0"),
            "anthropic.claude-opus-4-20250514-v1%3A0"
        );
    }

    #[test]
    fn calculate_signature_deterministic() {
        // Same inputs → same output.
        let s1 = calculate_signature("secret", "20260423", "us-east-1", "bedrock", "str").unwrap();
        let s2 = calculate_signature("secret", "20260423", "us-east-1", "bedrock", "str").unwrap();
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn hmac_sha256_known_vector() {
        // RFC 4231 test vector #1: key="Hi There", data="Hi"
        let key = b"key";
        let data = b"The quick brown fox jumps over the lazy dog";
        let r = hmac_sha256(key, data).unwrap();
        // Expected: f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8
        assert_eq!(
            r.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn extract_text_from_converse_response_ok() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{ "text": "hello world" }]
                }
            },
            "stopReason": "end_turn"
        }"#;
        let out = extract_text_from_converse_response(body).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn extract_text_concatenates_multiple_blocks() {
        let body = r#"{
            "output": {
                "message": {
                    "content": [
                        { "text": "part 1 " },
                        { "text": "part 2" }
                    ]
                }
            }
        }"#;
        let out = extract_text_from_converse_response(body).unwrap();
        assert_eq!(out, "part 1 part 2");
    }

    #[test]
    fn extract_text_rejects_missing_content() {
        let body = r#"{ "output": { "message": {} } }"#;
        let err = extract_text_from_converse_response(body).unwrap_err();
        assert!(matches!(err, OracleError::Llm(_)));
    }

    #[test]
    fn extract_text_rejects_empty_content() {
        let body = r#"{ "output": { "message": { "content": [] } } }"#;
        let err = extract_text_from_converse_response(body).unwrap_err();
        assert!(matches!(err, OracleError::Llm(_)));
    }

    #[test]
    fn parse_ini_profile_default() {
        let content = r#"
[default]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

[production]
aws_access_key_id = AKIAPRODEXAMPLE
aws_secret_access_key = wJalrProdSecret
"#;
        let (k, s, t) = parse_ini_profile(content, "default").unwrap();
        assert_eq!(k, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(s, "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY");
        assert_eq!(t, None);
    }

    #[test]
    fn parse_ini_profile_named() {
        let content = r#"
[default]
aws_access_key_id = AKIAdefault
aws_secret_access_key = secretdefault

[sso-prod]
aws_access_key_id = AKIAsso
aws_secret_access_key = secretsso
aws_session_token = FQoGZXIvY...
"#;
        let (k, s, t) = parse_ini_profile(content, "sso-prod").unwrap();
        assert_eq!(k, "AKIAsso");
        assert_eq!(s, "secretsso");
        assert_eq!(t.as_deref(), Some("FQoGZXIvY..."));
    }

    #[test]
    fn parse_ini_profile_missing_returns_none() {
        let content = "[default]\naws_access_key_id = K\naws_secret_access_key = S\n";
        assert!(parse_ini_profile(content, "nonexistent").is_none());
    }
}
