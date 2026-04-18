//! Unified communication tool — `send_message` dispatches to email (SES),
//! WhatsApp (Twilio), or Discord (webhooks) based on the `channel` param.
//!
//! Single tool keeps the agent's surface area small; channel-specific fields
//! are documented in the tool description. New transports (SMS, Telegram,
//! Signal, etc.) can be added as new channels without registering more tools.
//!
//! Credential resolution is env-var based today; BYOK per-customer comes
//! later via the credential vault.

use super::{Tool, ToolCategory, ToolResult};
use crate::ses::{EmailMessage, SesClient};
use amos_core::{AppConfig, Result};
use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

/// Unified message-sending tool. The `channel` parameter selects the
/// transport; channel-specific fields are validated per-channel.
pub struct SendMessageTool {
    email_client: Option<Arc<SesClient>>,
    config: Arc<AppConfig>,
    http_client: reqwest::Client,
}

impl SendMessageTool {
    pub fn new(email_client: Option<Arc<SesClient>>, config: Arc<AppConfig>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            email_client,
            config,
            http_client,
        }
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "send_message"
    }

    fn description(&self) -> &str {
        "Send a message via email (SES), WhatsApp (Twilio), or Discord (webhook). \
         Pick the channel and supply the fields it needs.\n\n\
         EMAIL (channel=email): requires `to` (string or array), `subject`. \
         At least one of `text` or `html` body. Optional: `cc`, `bcc`, \
         `reply_to`, `from` (override; must be SES-verified).\n\n\
         WHATSAPP (channel=whatsapp): requires `to` (E.164 phone like \
         '+15551234567'), `body`. Optional: `from` (override Twilio number). \
         Recipient must have opted in. Sandbox works for testing; prod \
         requires Meta WhatsApp Business approval.\n\n\
         DISCORD (channel=discord): requires `body`. Optional: `webhook_url` \
         (if AMOS__DISCORD__DEFAULT_WEBHOOK_URL is set, can be omitted), \
         `username`, `avatar_url`, `embeds` (array of Discord embed objects).\n\n\
         For mass sends, query a collection for recipients and call this \
         tool in a loop — or wire up a SendNotification automation with \
         channel='email'. Each channel has clear error messages if the \
         harness isn't configured for it."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "enum": ["email", "whatsapp", "discord"],
                    "description": "Which transport to use"
                },

                // Email + WhatsApp share 'to'; Discord ignores it (uses webhook_url).
                "to": {
                    "description": "Recipient(s). Email: address or array of addresses. WhatsApp: E.164 phone (e.g. '+15551234567'). Discord: ignored.",
                    "oneOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ]
                },

                // Email-specific
                "subject": { "type": "string", "description": "Email subject line (required for email)" },
                "text": { "type": "string", "description": "Plain-text body. Email: at least one of text/html required." },
                "html": { "type": "string", "description": "HTML body (email only)" },
                "cc": {
                    "description": "CC recipients (email only)",
                    "oneOf": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }]
                },
                "bcc": {
                    "description": "BCC recipients (email only)",
                    "oneOf": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }]
                },
                "reply_to": { "type": "string", "description": "Reply-To address (email only)" },
                "from": {
                    "type": "string",
                    "description": "Override the default sender. Email: must be SES-verified. WhatsApp: 'whatsapp:+E.164' format."
                },

                // WhatsApp + Discord use 'body' for the message text.
                "body": {
                    "type": "string",
                    "description": "Message body. Required for WhatsApp and Discord."
                },

                // Discord-specific
                "webhook_url": { "type": "string", "description": "Discord webhook URL. Optional if AMOS__DISCORD__DEFAULT_WEBHOOK_URL is set." },
                "username": { "type": "string", "description": "Discord display-name override" },
                "avatar_url": { "type": "string", "description": "Discord avatar override" },
                "embeds": {
                    "type": "array",
                    "description": "Discord embed objects (see Discord API)",
                    "items": { "type": "object" }
                }
            },
            "required": ["channel"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let channel = match params.get("channel").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return Ok(ToolResult::error("`channel` is required".to_string())),
        };

        match channel {
            "email" => self.send_email(&params).await,
            "whatsapp" => self.send_whatsapp(&params).await,
            "discord" => self.send_discord(&params).await,
            other => Ok(ToolResult::error(format!(
                "Unknown channel '{}'. Supported: email, whatsapp, discord.",
                other
            ))),
        }
    }
}

impl SendMessageTool {
    async fn send_email(&self, params: &JsonValue) -> Result<ToolResult> {
        let client = match &self.email_client {
            Some(c) => c,
            None => {
                return Ok(ToolResult::error(
                    "Email is not configured on this harness. Set AMOS__EMAIL__FROM_ADDRESS \
                     to enable SES delivery."
                        .to_string(),
                ));
            }
        };

        let to = parse_address_list(params.get("to"));
        if to.is_empty() {
            return Ok(ToolResult::error(
                "`to` is required for email (string or non-empty array)".to_string(),
            ));
        }
        let cc = parse_address_list(params.get("cc"));
        let bcc = parse_address_list(params.get("bcc"));

        let subject = match params.get("subject").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => {
                return Ok(ToolResult::error(
                    "`subject` is required for email".to_string(),
                ))
            }
        };

        let text = params
            .get("text")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let html = params
            .get("html")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if text.is_none() && html.is_none() {
            return Ok(ToolResult::error(
                "Email requires at least one of `text` or `html`".to_string(),
            ));
        }

        let from = params
            .get("from")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let reply_to = params
            .get("reply_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let msg = EmailMessage {
            to: to.clone(),
            cc,
            bcc,
            subject: subject.clone(),
            text,
            html,
            from,
            reply_to,
        };

        match client.send(msg).await {
            Ok(result) => Ok(ToolResult::success(json!({
                "sent": true,
                "channel": "email",
                "message_id": result.message_id,
                "to": to,
                "subject": subject,
            }))),
            Err(e) => Ok(ToolResult::error(format!("Email send failed: {}", e))),
        }
    }

    async fn send_whatsapp(&self, params: &JsonValue) -> Result<ToolResult> {
        let cfg = &self.config.twilio;
        let account_sid = match &cfg.account_sid {
            Some(s) if !s.trim().is_empty() => s,
            _ => {
                return Ok(ToolResult::error(
                    "WhatsApp not configured: set AMOS__TWILIO__ACCOUNT_SID".to_string(),
                ))
            }
        };
        let auth_token = match &cfg.auth_token {
            Some(t) if !t.expose_secret().trim().is_empty() => t.expose_secret().to_string(),
            _ => {
                return Ok(ToolResult::error(
                    "WhatsApp not configured: set AMOS__TWILIO__AUTH_TOKEN".to_string(),
                ))
            }
        };

        let from = params
            .get("from")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| cfg.from_number.clone())
            .ok_or_else(|| {
                amos_core::AmosError::Config(
                    "WhatsApp not configured: set AMOS__TWILIO__FROM_NUMBER or pass `from`"
                        .to_string(),
                )
            })?;
        let from = if from.starts_with("whatsapp:") {
            from
        } else {
            format!("whatsapp:{}", from)
        };

        // WhatsApp uses a single recipient; if an array is passed, take the first.
        let to_list = parse_address_list(params.get("to"));
        let to_raw = match to_list.first() {
            Some(s) if !s.is_empty() => s.clone(),
            _ => {
                return Ok(ToolResult::error(
                    "`to` is required for whatsapp (phone in E.164)".to_string(),
                ))
            }
        };
        let to = if to_raw.starts_with("whatsapp:") {
            to_raw
        } else {
            format!("whatsapp:{}", to_raw)
        };

        let body = match params.get("body").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => {
                return Ok(ToolResult::error(
                    "`body` is required for whatsapp".to_string(),
                ))
            }
        };

        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            account_sid
        );
        let form = [
            ("From", from.as_str()),
            ("To", to.as_str()),
            ("Body", body.as_str()),
        ];

        let resp = self
            .http_client
            .post(&url)
            .basic_auth(account_sid, Some(auth_token))
            .form(&form)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status();
                let body_text = r.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Ok(ToolResult::error(format!(
                        "Twilio HTTP {}: {}",
                        status, body_text
                    )));
                }
                let parsed: serde_json::Value = serde_json::from_str(&body_text)
                    .unwrap_or_else(|_| json!({ "raw": body_text }));
                let sid = parsed
                    .get("sid")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                Ok(ToolResult::success(json!({
                    "sent": true,
                    "channel": "whatsapp",
                    "sid": sid,
                    "to": to,
                    "from": from,
                })))
            }
            Err(e) => Ok(ToolResult::error(format!("Twilio request failed: {}", e))),
        }
    }

    async fn send_discord(&self, params: &JsonValue) -> Result<ToolResult> {
        let url = params
            .get("webhook_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| self.config.discord.default_webhook_url.clone());

        let url = match url {
            Some(u) if !u.trim().is_empty() => u,
            _ => {
                return Ok(ToolResult::error(
                    "No Discord webhook URL available. Set AMOS__DISCORD__DEFAULT_WEBHOOK_URL \
                     or pass `webhook_url`."
                        .to_string(),
                ))
            }
        };

        let content = match params.get("body").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => {
                return Ok(ToolResult::error(
                    "`body` is required for discord".to_string(),
                ))
            }
        };

        let mut body_obj = serde_json::Map::new();
        body_obj.insert("content".to_string(), json!(content));
        if let Some(u) = params.get("username").and_then(|v| v.as_str()) {
            body_obj.insert("username".to_string(), json!(u));
        }
        if let Some(a) = params.get("avatar_url").and_then(|v| v.as_str()) {
            body_obj.insert("avatar_url".to_string(), json!(a));
        }
        if let Some(e) = params.get("embeds").cloned() {
            body_obj.insert("embeds".to_string(), e);
        }

        let resp = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body_obj)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status();
                if !status.is_success() {
                    let body_text = r.text().await.unwrap_or_default();
                    return Ok(ToolResult::error(format!(
                        "Discord HTTP {}: {}",
                        status, body_text
                    )));
                }
                Ok(ToolResult::success(json!({
                    "sent": true,
                    "channel": "discord",
                    "status": status.as_u16(),
                })))
            }
            Err(e) => Ok(ToolResult::error(format!("Discord request failed: {}", e))),
        }
    }
}

/// Parse a JSON value into a list of recipient addresses.
/// Accepts either a single string or an array of strings.
fn parse_address_list(value: Option<&JsonValue>) -> Vec<String> {
    match value {
        Some(JsonValue::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        Some(JsonValue::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_config() -> Arc<AppConfig> {
        Arc::new(
            serde_json::from_value(json!({
                "database": { "url": "postgres://x" }
            }))
            .unwrap(),
        )
    }

    #[test]
    fn tool_metadata() {
        let tool = SendMessageTool::new(None, mock_config());
        assert_eq!(tool.name(), "send_message");
        assert_eq!(tool.category(), ToolCategory::Integration);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["channel"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "channel");
    }

    #[tokio::test]
    async fn email_channel_returns_error_when_disabled() {
        let tool = SendMessageTool::new(None, mock_config());
        let result = tool
            .execute(json!({
                "channel": "email",
                "to": "a@b.com",
                "subject": "hi",
                "text": "body"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not configured"));
    }

    #[tokio::test]
    async fn whatsapp_channel_returns_error_when_twilio_missing() {
        let tool = SendMessageTool::new(None, mock_config());
        let result = tool
            .execute(json!({
                "channel": "whatsapp",
                "to": "+15551234567",
                "body": "hi"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("ACCOUNT_SID"));
    }

    #[tokio::test]
    async fn discord_channel_returns_error_without_webhook() {
        let tool = SendMessageTool::new(None, mock_config());
        let result = tool
            .execute(json!({
                "channel": "discord",
                "body": "hi"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("webhook"));
    }

    #[tokio::test]
    async fn unknown_channel_rejected() {
        let tool = SendMessageTool::new(None, mock_config());
        let result = tool
            .execute(json!({
                "channel": "telegram",
                "body": "hi"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unknown channel"));
    }

    #[tokio::test]
    async fn missing_channel_rejected() {
        let tool = SendMessageTool::new(None, mock_config());
        let result = tool.execute(json!({ "body": "hi" })).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("channel"));
    }

    #[test]
    fn parse_address_list_single_string() {
        assert_eq!(
            parse_address_list(Some(&json!("a@b.com"))),
            vec!["a@b.com".to_string()]
        );
    }

    #[test]
    fn parse_address_list_array() {
        assert_eq!(
            parse_address_list(Some(&json!(["a@b.com", "c@d.com"]))).len(),
            2
        );
    }

    #[test]
    fn parse_address_list_empty() {
        assert!(parse_address_list(Some(&json!(""))).is_empty());
        assert!(parse_address_list(None).is_empty());
    }

    #[test]
    fn parse_address_list_filters_empty_strings_in_array() {
        assert_eq!(
            parse_address_list(Some(&json!(["a@b.com", "", "  ", "c@d.com"]))).len(),
            2
        );
    }
}
