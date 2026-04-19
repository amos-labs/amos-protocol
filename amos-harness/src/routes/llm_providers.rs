//! LLM Provider management routes - BYOK (Bring Your Own Key) support.
//!
//! These endpoints let users configure which LLM provider (Anthropic, OpenAI, etc.)
//! the agent uses, and supply their own API key. The API key is stored encrypted
//! in the credential vault.
//!
//! Endpoints:
//!   - `GET  /api/v1/llm-providers`          - List all configured providers
//!   - `POST /api/v1/llm-providers`          - Add a new provider config
//!   - `PUT  /api/v1/llm-providers/:id`      - Update a provider config
//!   - `POST /api/v1/llm-providers/:id/activate` - Set this provider as active
//!   - `POST /api/v1/llm-providers/:id/test` - Test connection with provider
//!   - `DELETE /api/v1/llm-providers/:id`    - Delete a provider config
//!   - `GET  /api/v1/llm-providers/active`   - Get active provider (with decrypted key, internal only)

use crate::routes::credentials;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_providers).post(create_provider))
        .route("/active", get(get_active_provider))
        .route("/{id}", put(update_provider).delete(delete_provider))
        .route("/{id}/activate", post(activate_provider))
        .route("/{id}/test", post(test_provider))
}

// ═══════════════════════════════════════════════════════════════════════════
// Request/Response types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,          // "anthropic", "openai", "custom"
    pub display_name: String,  // "Claude (Anthropic)"
    pub api_base: String,      // "https://api.anthropic.com/v1"
    pub api_key: String,       // plaintext key (will be encrypted and stored in vault)
    pub default_model: String, // "claude-sonnet-4-20250514"
    #[serde(default)]
    pub available_models: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub display_name: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>, // if provided, update the credential vault
    pub default_model: Option<String>,
    pub available_models: Option<Vec<String>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LlmProviderRow {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub api_base: String,
    pub credential_id: Option<Uuid>,
    pub default_model: String,
    pub available_models: serde_json::Value,
    pub is_active: bool,
    pub is_verified: bool,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Active provider response includes the decrypted API key.
/// This should only be consumed internally (by agent proxy), never by the browser.
#[derive(Debug, Serialize)]
pub struct ActiveProviderResponse {
    pub provider_type: String,
    pub api_base: String,
    pub api_key: String,
    pub model_id: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════════════════

/// GET /api/v1/llm-providers - List all configured providers.
async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LlmProviderRow>>, StatusCode> {
    let providers = sqlx::query_as::<_, LlmProviderRow>(
        r#"SELECT id, name, display_name, api_base, credential_id,
                  default_model, available_models, is_active, is_verified,
                  last_error, created_at, updated_at
           FROM llm_providers
           ORDER BY is_active DESC, name ASC"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list LLM providers: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(providers))
}

/// POST /api/v1/llm-providers - Create a new provider config with an API key.
async fn create_provider(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProviderRequest>,
) -> Result<(StatusCode, Json<LlmProviderRow>), StatusCode> {
    // Store the API key in the credential vault
    let credential_id = store_api_key_in_vault(&state, &body.name, &body.api_key).await?;

    let available_models = serde_json::to_value(&body.available_models).unwrap_or_default();
    let provider_id = Uuid::new_v4();

    // Check if there are any existing providers - if not, auto-activate this one
    let existing_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM llm_providers")
        .fetch_one(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let should_activate = existing_count.0 == 0;

    sqlx::query(
        r#"INSERT INTO llm_providers
           (id, name, display_name, api_base, credential_id, default_model, available_models, is_active)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(provider_id)
    .bind(&body.name)
    .bind(&body.display_name)
    .bind(&body.api_base)
    .bind(credential_id)
    .bind(&body.default_model)
    .bind(&available_models)
    .bind(should_activate)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create LLM provider: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let row = sqlx::query_as::<_, LlmProviderRow>("SELECT * FROM llm_providers WHERE id = $1")
        .bind(provider_id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(
        provider = %body.name,
        model = %body.default_model,
        active = should_activate,
        "LLM provider created"
    );

    Ok((StatusCode::CREATED, Json(row)))
}

/// PUT /api/v1/llm-providers/:id - Update provider config.
async fn update_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateProviderRequest>,
) -> Result<Json<LlmProviderRow>, StatusCode> {
    // If a new API key is provided, update the credential vault
    if let Some(ref api_key) = body.api_key {
        // Get current provider to find its name and credential_id
        let current: (String, Option<Uuid>) =
            sqlx::query_as("SELECT name, credential_id FROM llm_providers WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.db_pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;

        let (name, old_cred_id) = current;

        // Revoke old credential if exists
        if let Some(old_id) = old_cred_id {
            let _ = sqlx::query(
                "UPDATE credential_vault SET status = 'revoked', updated_at = NOW() WHERE id = $1",
            )
            .bind(old_id)
            .execute(&state.db_pool)
            .await;
        }

        // Store new key
        let new_cred_id = store_api_key_in_vault(&state, &name, api_key).await?;
        sqlx::query(
            "UPDATE llm_providers SET credential_id = $1, is_verified = false, updated_at = NOW() WHERE id = $2",
        )
        .bind(new_cred_id)
        .bind(id)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Auto-activate: if no provider is currently active and this one now has a key, activate it
        let active_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM llm_providers WHERE is_active = true")
                .fetch_one(&state.db_pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if active_count.0 == 0 {
            sqlx::query(
                "UPDATE llm_providers SET is_active = true, updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .execute(&state.db_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            tracing::info!(provider = %name, "Auto-activated provider (only one with a key)");
        }
    }

    // Update other fields
    if let Some(ref display_name) = body.display_name {
        sqlx::query("UPDATE llm_providers SET display_name = $1, updated_at = NOW() WHERE id = $2")
            .bind(display_name)
            .bind(id)
            .execute(&state.db_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(ref api_base) = body.api_base {
        sqlx::query("UPDATE llm_providers SET api_base = $1, updated_at = NOW() WHERE id = $2")
            .bind(api_base)
            .bind(id)
            .execute(&state.db_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(ref default_model) = body.default_model {
        sqlx::query(
            "UPDATE llm_providers SET default_model = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(default_model)
        .bind(id)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(ref models) = body.available_models {
        let models_json = serde_json::to_value(models).unwrap_or_default();
        sqlx::query(
            "UPDATE llm_providers SET available_models = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(&models_json)
        .bind(id)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let row = sqlx::query_as::<_, LlmProviderRow>("SELECT * FROM llm_providers WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

/// POST /api/v1/llm-providers/:id/activate - Set this provider as the active one.
async fn activate_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<LlmProviderRow>, StatusCode> {
    // Verify provider exists and has a credential
    let provider: (Option<Uuid>,) =
        sqlx::query_as("SELECT credential_id FROM llm_providers WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    if provider.0.is_none() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Deactivate all, then activate this one (in a transaction)
    let mut tx = state
        .db_pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("UPDATE llm_providers SET is_active = false, updated_at = NOW()")
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("UPDATE llm_providers SET is_active = true, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Activating a BYOK provider implies the customer wants to use it —
    // flip `llm_provider_mode` to "byok" so the agent proxy actually routes
    // to this provider instead of shared Bedrock. Without this the provider
    // would be flagged active in the DB but chats would silently still go
    // through shared Bedrock, which is the bug we just closed.
    sqlx::query(
        r#"INSERT INTO harness_settings (key, value, updated_at)
              VALUES ('llm_provider_mode', '"byok"'::jsonb, NOW())
           ON CONFLICT (key) DO UPDATE SET value = '"byok"'::jsonb, updated_at = NOW()"#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query_as::<_, LlmProviderRow>("SELECT * FROM llm_providers WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    tracing::info!(
        provider = %row.name,
        model = %row.default_model,
        "LLM provider activated (mode flipped to byok)"
    );

    Ok(Json(row))
}

/// POST /api/v1/llm-providers/:id/test - Test connection to the provider.
async fn test_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Get provider config and decrypt the API key
    let provider = sqlx::query_as::<_, LlmProviderRow>("SELECT * FROM llm_providers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let credential_id = provider
        .credential_id
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let api_key = credentials::decrypt_credential(&state.db_pool, &state.vault, credential_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Make a minimal API call to test the connection
    let client = reqwest::Client::new();
    let test_result = match provider.name.as_str() {
        "anthropic" => {
            let resp = client
                .post(format!(
                    "{}/messages",
                    provider.api_base.trim_end_matches('/')
                ))
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": provider.default_model,
                    "max_tokens": 10,
                    "messages": [{"role": "user", "content": "Hi"}]
                }))
                .send()
                .await;
            resp
        }
        _ => {
            // OpenAI-compatible
            let mut req = client
                .post(format!(
                    "{}/chat/completions",
                    provider.api_base.trim_end_matches('/')
                ))
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": provider.default_model,
                    "max_tokens": 10,
                    "messages": [{"role": "user", "content": "Hi"}]
                }));
            if !api_key.is_empty() {
                req = req.bearer_auth(&api_key);
            }
            req.send().await
        }
    };

    match test_result {
        Ok(resp) if resp.status().is_success() => {
            sqlx::query(
                "UPDATE llm_providers SET is_verified = true, last_error = NULL, updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .execute(&state.db_pool)
            .await
            .ok();

            Ok(Json(serde_json::json!({
                "status": "ok",
                "message": "Connection successful"
            })))
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let error_msg = format!("HTTP {}: {}", status, body);

            sqlx::query(
                "UPDATE llm_providers SET is_verified = false, last_error = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(&error_msg)
            .bind(id)
            .execute(&state.db_pool)
            .await
            .ok();

            Ok(Json(serde_json::json!({
                "status": "error",
                "message": error_msg
            })))
        }
        Err(e) => {
            let error_msg = format!("Connection failed: {}", e);

            sqlx::query(
                "UPDATE llm_providers SET is_verified = false, last_error = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(&error_msg)
            .bind(id)
            .execute(&state.db_pool)
            .await
            .ok();

            Ok(Json(serde_json::json!({
                "status": "error",
                "message": error_msg
            })))
        }
    }
}

/// DELETE /api/v1/llm-providers/:id - Delete a provider config.
async fn delete_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // Revoke associated credential
    let cred_id: Option<(Option<Uuid>,)> =
        sqlx::query_as("SELECT credential_id FROM llm_providers WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some((Some(cred_id),)) = cred_id {
        let _ = sqlx::query(
            "UPDATE credential_vault SET status = 'revoked', updated_at = NOW() WHERE id = $1",
        )
        .bind(cred_id)
        .execute(&state.db_pool)
        .await;
    }

    let result = sqlx::query("DELETE FROM llm_providers WHERE id = $1")
        .bind(id)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/llm-providers/active - Get the active provider with decrypted API key.
///
/// This is called by the agent proxy before forwarding chat requests, so it can
/// inject the provider config as headers. This endpoint should NOT be exposed to
/// the browser (the proxy calls it internally on the same host).
pub async fn get_active_provider(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ActiveProviderResponse>, StatusCode> {
    let provider = sqlx::query_as::<_, LlmProviderRow>(
        "SELECT * FROM llm_providers WHERE is_active = true LIMIT 1",
    )
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get active LLM provider: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let provider = match provider {
        Some(p) => p,
        None => {
            // No active provider configured - return 404 so proxy falls back to default
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let credential_id = provider
        .credential_id
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;

    let api_key =
        credentials::decrypt_credential(&state.db_pool, &state.vault, credential_id).await?;

    Ok(Json(ActiveProviderResponse {
        provider_type: provider.name,
        api_base: provider.api_base,
        api_key,
        model_id: provider.default_model,
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Internal helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Encrypt and store an API key in the credential vault, returning the credential ID.
async fn store_api_key_in_vault(
    state: &AppState,
    provider_name: &str,
    api_key: &str,
) -> Result<Uuid, StatusCode> {
    let encrypted_value = state.vault.encrypt_string(api_key).map_err(|e| {
        tracing::error!("Failed to encrypt API key: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let credential_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO credential_vault
           (id, label, service, credential_type, encrypted_value, status)
           VALUES ($1, $2, $3, 'api_key', $4, 'active')"#,
    )
    .bind(credential_id)
    .bind(format!("{} API Key", provider_name))
    .bind(format!("llm_{}", provider_name))
    .bind(&encrypted_value)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to store API key in vault: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(credential_id)
}
