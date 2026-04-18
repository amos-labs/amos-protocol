//! OAuth2 self-service tools.
//!
//! These let Amos walk users through connecting any OAuth2 provider without
//! operator involvement. The flow:
//!
//!   1. Agent calls `list_oauth_providers` to show what AMOS knows about.
//!   2. User picks one (e.g. "Google").
//!   3. Agent calls `initiate_oauth_connection` — user is told *exactly* where
//!      to create their own OAuth app, which redirect URI to register, and
//!      then supplies client_id + client_secret. The tool creates an
//!      `integration_credentials` row and returns a start URL.
//!   4. User clicks the start URL, consents with the provider, gets redirected
//!      back, tokens stored (handled by `routes/oauth.rs`).
//!   5. Agent can call `list_connections` any time to see what's wired up.
//!   6. `revoke_connection` marks a credential as revoked when the user
//!      wants to disconnect.
//!
//! Custom providers not in the directory are supported: pass
//! `auth_url`/`token_url`/etc. directly to `initiate_oauth_connection`.

use super::{Tool, ToolCategory, ToolResult};
use amos_core::{AmosError, AppConfig, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

// ─── ListOAuthProvidersTool ─────────────────────────────────────────────

/// Browse the directory of well-known OAuth providers.
pub struct ListOAuthProvidersTool {
    db_pool: PgPool,
}

impl ListOAuthProvidersTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for ListOAuthProvidersTool {
    fn name(&self) -> &str {
        "list_oauth_providers"
    }

    fn description(&self) -> &str {
        "List the OAuth2 providers AMOS has pre-configured knowledge about \
         (Google, Slack, HubSpot, Notion, Microsoft, GitHub, LinkedIn, X, \
         Calendly, Atlassian, Zoom, etc.). Each entry includes the provider's \
         auth/token URLs, default scopes, developer console URL, and exact \
         setup instructions. Use this before initiate_oauth_connection to \
         show the user what's available. Custom providers (not in this list) \
         are still supported — pass auth_url/token_url directly to \
         initiate_oauth_connection."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "slug": {
                    "type": "string",
                    "description": "Optional: return details for a single provider by slug (e.g. 'google', 'slack')."
                }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let slug = params.get("slug").and_then(|v| v.as_str());

        let rows = if let Some(s) = slug {
            sqlx::query_as::<_, OAuthProviderRow>(
                r#"SELECT slug, name, auth_url, token_url, default_scopes,
                          app_creation_url, docs_url, icon_url, setup_instructions,
                          extra_auth_params
                     FROM oauth_providers
                    WHERE slug = $1"#,
            )
            .bind(s)
            .fetch_all(&self.db_pool)
            .await
        } else {
            sqlx::query_as::<_, OAuthProviderRow>(
                r#"SELECT slug, name, auth_url, token_url, default_scopes,
                          app_creation_url, docs_url, icon_url, setup_instructions,
                          extra_auth_params
                     FROM oauth_providers
                 ORDER BY name ASC"#,
            )
            .fetch_all(&self.db_pool)
            .await
        }
        .map_err(|e| AmosError::Internal(format!("Failed to query oauth_providers: {}", e)))?;

        Ok(ToolResult::success(json!({
            "providers": rows,
            "count": rows.len(),
        })))
    }
}

// ─── InitiateOAuthConnectionTool ────────────────────────────────────────

/// Create an OAuth2 credential row and return the start URL.
pub struct InitiateOAuthConnectionTool {
    db_pool: PgPool,
    config: Arc<AppConfig>,
}

impl InitiateOAuthConnectionTool {
    pub fn new(db_pool: PgPool, config: Arc<AppConfig>) -> Self {
        Self { db_pool, config }
    }
}

#[async_trait]
impl Tool for InitiateOAuthConnectionTool {
    fn name(&self) -> &str {
        "initiate_oauth_connection"
    }

    fn description(&self) -> &str {
        "Set up a new OAuth2 connection. Creates an integration_credentials \
         row and returns a start URL the user clicks to authorize. Before \
         calling this you need (a) a connection label and (b) the user's \
         client_id + client_secret from their OAuth app in the provider's \
         developer console. Use `provider_slug` for known providers (fills \
         auth_url/token_url/scopes from the directory). For custom/unknown \
         providers, pass `auth_url`, `token_url`, and `scopes` directly. \
         Also requires `integration_id` — create an `integrations` row \
         first via manage_integration if you don't already have one."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "integration_id": {
                    "type": "string",
                    "description": "UUID of the parent integration row. Required."
                },
                "label": {
                    "type": "string",
                    "description": "Human-friendly name shown in the Connections canvas (e.g. 'Jana's Google Calendar')."
                },
                "client_id": {
                    "type": "string",
                    "description": "OAuth client ID from the provider's developer console."
                },
                "client_secret": {
                    "type": "string",
                    "description": "OAuth client secret. Stored in the credential vault, never returned in plaintext."
                },
                "provider_slug": {
                    "type": "string",
                    "description": "Slug of a known provider (e.g. 'google'). Fills auth_url/token_url/scopes from oauth_providers directory. Either this OR auth_url+token_url must be provided."
                },
                "auth_url": {
                    "type": "string",
                    "description": "Provider's authorize URL. Required if provider_slug not given."
                },
                "token_url": {
                    "type": "string",
                    "description": "Provider's token exchange URL. Required if provider_slug not given."
                },
                "scopes": {
                    "type": "string",
                    "description": "Override the default scopes. Space-separated string."
                }
            },
            "required": ["integration_id", "label", "client_id", "client_secret"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let integration_id = match params
            .get("integration_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            Some(id) => id,
            None => {
                return Ok(ToolResult::error(
                    "integration_id must be a valid UUID".to_string(),
                ))
            }
        };

        let label = match params.get("label").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => return Ok(ToolResult::error("label is required".to_string())),
        };
        let client_id = match params.get("client_id").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => return Ok(ToolResult::error("client_id is required".to_string())),
        };
        let client_secret = match params.get("client_secret").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => return Ok(ToolResult::error("client_secret is required".to_string())),
        };

        // Resolve provider details: either from the directory or from explicit params.
        let (auth_url, token_url, default_scopes, extra_auth_params) = if let Some(slug) =
            params.get("provider_slug").and_then(|v| v.as_str())
        {
            let row = sqlx::query_as::<_, OAuthProviderRow>(
                r#"SELECT slug, name, auth_url, token_url, default_scopes,
                              app_creation_url, docs_url, icon_url, setup_instructions,
                              extra_auth_params
                         FROM oauth_providers
                        WHERE slug = $1"#,
            )
            .bind(slug)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("DB error: {}", e)))?;

            let row = match row {
                    Some(r) => r,
                    None => {
                        return Ok(ToolResult::error(format!(
                            "Unknown provider_slug '{}'. Call list_oauth_providers to see available slugs or pass auth_url/token_url directly.",
                            slug
                        )))
                    }
                };
            (
                row.auth_url,
                row.token_url,
                row.default_scopes.unwrap_or_default(),
                row.extra_auth_params.unwrap_or_else(|| json!({})),
            )
        } else {
            let auth_url = match params.get("auth_url").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => {
                    return Ok(ToolResult::error(
                        "Either provider_slug or auth_url is required".to_string(),
                    ))
                }
            };
            let token_url = match params.get("token_url").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => {
                    return Ok(ToolResult::error(
                        "token_url is required when provider_slug is not given".to_string(),
                    ))
                }
            };
            (auth_url, token_url, String::new(), json!({}))
        };

        let scopes = params
            .get("scopes")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(default_scopes);

        let credential_id = Uuid::new_v4();

        let metadata = json!({ "extra_auth_params": extra_auth_params });

        sqlx::query(
            r#"INSERT INTO integration_credentials
                  (id, integration_id, auth_type, credentials_data, label, status,
                   oauth_auth_url, oauth_token_url, oauth_client_id, oauth_client_secret,
                   oauth_scopes, metadata)
               VALUES ($1, $2, 'oauth2', '{}'::jsonb, $3, 'pending',
                   $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(credential_id)
        .bind(integration_id)
        .bind(&label)
        .bind(&auth_url)
        .bind(&token_url)
        .bind(&client_id)
        .bind(&client_secret)
        .bind(if scopes.is_empty() {
            None
        } else {
            Some(&scopes)
        })
        .bind(&metadata)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to create credential: {}", e)))?;

        let start_url = format!(
            "{}/api/v1/oauth/start/{}",
            self.config.oauth.redirect_base_url.trim_end_matches('/'),
            credential_id
        );
        let redirect_uri = format!(
            "{}/api/v1/oauth/callback",
            self.config.oauth.redirect_base_url.trim_end_matches('/')
        );

        Ok(ToolResult::success(json!({
            "credential_id": credential_id,
            "label": label,
            "status": "pending",
            "start_url": start_url,
            "redirect_uri": redirect_uri,
            "message": format!("Have the user open {} in a browser to authorize the connection.", start_url),
        })))
    }
}

// ─── ListConnectionsTool ────────────────────────────────────────────────

pub struct ListConnectionsTool {
    db_pool: PgPool,
}

impl ListConnectionsTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for ListConnectionsTool {
    fn name(&self) -> &str {
        "list_connections"
    }

    fn description(&self) -> &str {
        "List all integration credentials (OAuth, API key, bearer, basic auth) \
         with status, auth type, and last-used timestamps. Secrets are never \
         returned. Use this to show the user what's connected or to check a \
         specific integration_id. Status values: active, pending, expired, \
         revoked."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "integration_id": {
                    "type": "string",
                    "description": "Optional: filter by parent integration UUID."
                },
                "auth_type": {
                    "type": "string",
                    "description": "Optional: filter by auth type (oauth2, api_key, bearer_token, basic_auth)."
                },
                "status": {
                    "type": "string",
                    "description": "Optional: filter by status (active, pending, expired, revoked)."
                }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let integration_id = params
            .get("integration_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let auth_type = params.get("auth_type").and_then(|v| v.as_str());
        let status = params.get("status").and_then(|v| v.as_str());

        let rows = sqlx::query_as::<_, ConnectionRow>(
            r#"SELECT c.id, c.integration_id, i.name AS integration_name,
                      c.auth_type, c.status, c.label,
                      c.token_expires_at, c.last_used_at, c.created_at, c.updated_at,
                      c.oauth_scopes
                 FROM integration_credentials c
                 LEFT JOIN integrations i ON i.id = c.integration_id
                WHERE ($1::uuid IS NULL OR c.integration_id = $1)
                  AND ($2::text IS NULL OR c.auth_type = $2)
                  AND ($3::text IS NULL OR c.status = $3)
             ORDER BY c.created_at DESC"#,
        )
        .bind(integration_id)
        .bind(auth_type)
        .bind(status)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to query connections: {}", e)))?;

        Ok(ToolResult::success(json!({
            "connections": rows,
            "count": rows.len(),
        })))
    }
}

// ─── RevokeConnectionTool ───────────────────────────────────────────────

pub struct RevokeConnectionTool {
    db_pool: PgPool,
}

impl RevokeConnectionTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for RevokeConnectionTool {
    fn name(&self) -> &str {
        "revoke_connection"
    }

    fn description(&self) -> &str {
        "Mark an integration credential as revoked. The row is kept for audit \
         purposes, but access_token and refresh_token are cleared and status \
         is set to 'revoked'. The credential can no longer be used for API \
         calls. Does NOT revoke the token with the upstream provider — the \
         user should also revoke access in the provider's account settings \
         for full logout."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "credential_id": {
                    "type": "string",
                    "description": "UUID of the credential to revoke."
                }
            },
            "required": ["credential_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let credential_id = match params
            .get("credential_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            Some(id) => id,
            None => {
                return Ok(ToolResult::error(
                    "credential_id must be a valid UUID".to_string(),
                ))
            }
        };

        let result = sqlx::query(
            r#"UPDATE integration_credentials
                  SET status = 'revoked',
                      access_token = NULL,
                      refresh_token = NULL,
                      token_expires_at = NULL,
                      updated_at = NOW()
                WHERE id = $1"#,
        )
        .bind(credential_id)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to revoke credential: {}", e)))?;

        if result.rows_affected() == 0 {
            return Ok(ToolResult::error(format!(
                "No credential found with id {}",
                credential_id
            )));
        }

        Ok(ToolResult::success(json!({
            "revoked": true,
            "credential_id": credential_id,
        })))
    }
}

// ─── Row types ──────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct OAuthProviderRow {
    slug: String,
    name: String,
    auth_url: String,
    token_url: String,
    default_scopes: Option<String>,
    app_creation_url: Option<String>,
    docs_url: Option<String>,
    icon_url: Option<String>,
    setup_instructions: Option<String>,
    extra_auth_params: Option<JsonValue>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct ConnectionRow {
    id: Uuid,
    integration_id: Uuid,
    integration_name: Option<String>,
    auth_type: String,
    status: String,
    label: Option<String>,
    token_expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    oauth_scopes: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    fn mock_pool() -> PgPool {
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent_test_db")
            .unwrap()
    }

    fn mock_config() -> Arc<AppConfig> {
        // Build via env-free default path: use the Config crate in tests to
        // construct a minimal AppConfig. If that's too heavy here, skip —
        // these tests only exercise tool metadata.
        Arc::new(
            serde_json::from_value(json!({
                "database": { "url": "postgres://x" }
            }))
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn list_providers_metadata() {
        let tool = ListOAuthProvidersTool::new(mock_pool());
        assert_eq!(tool.name(), "list_oauth_providers");
        assert_eq!(tool.category(), ToolCategory::Integration);
    }

    #[tokio::test]
    async fn initiate_oauth_metadata() {
        let tool = InitiateOAuthConnectionTool::new(mock_pool(), mock_config());
        assert_eq!(tool.name(), "initiate_oauth_connection");
        let schema = tool.parameters_schema();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"integration_id"));
        assert!(required.contains(&"label"));
        assert!(required.contains(&"client_id"));
        assert!(required.contains(&"client_secret"));
    }

    #[tokio::test]
    async fn list_connections_metadata() {
        let tool = ListConnectionsTool::new(mock_pool());
        assert_eq!(tool.name(), "list_connections");
    }

    #[tokio::test]
    async fn revoke_connection_metadata() {
        let tool = RevokeConnectionTool::new(mock_pool());
        assert_eq!(tool.name(), "revoke_connection");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["credential_id"].is_object());
    }

    #[tokio::test]
    async fn initiate_rejects_invalid_integration_id() {
        let tool = InitiateOAuthConnectionTool::new(mock_pool(), mock_config());
        let result = tool
            .execute(json!({
                "integration_id": "not-a-uuid",
                "label": "test",
                "client_id": "cid",
                "client_secret": "cs"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("UUID"));
    }

    #[tokio::test]
    async fn revoke_rejects_invalid_uuid() {
        let tool = RevokeConnectionTool::new(mock_pool());
        let result = tool
            .execute(json!({ "credential_id": "not-a-uuid" }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("UUID"));
    }
}
