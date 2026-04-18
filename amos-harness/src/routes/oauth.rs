//! OAuth2 Authorization Code + PKCE flow scaffolding for integrations.
//!
//! Flow:
//!   1. Agent creates an `integration_credentials` row with auth_type='oauth2'
//!      and populates oauth_auth_url, oauth_token_url, oauth_client_id,
//!      oauth_client_secret, oauth_scopes.
//!   2. User visits `GET /api/v1/oauth/start/:credential_id` — we generate a
//!      state_token + PKCE code_verifier, store them in `oauth_states`,
//!      then 302 to the provider's authorize URL.
//!   3. Provider redirects back to `/api/v1/oauth/callback?code=...&state=...`.
//!   4. We exchange the code for tokens (with client_secret + code_verifier)
//!      and write them back to `integration_credentials`.
//!
//! Cleanup: stale `oauth_states` rows (>10min) are harmless; add a periodic
//! DELETE later if the table grows.

use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Redirect},
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/start/{credential_id}", get(start))
        .route("/callback", get(callback))
}

#[derive(Deserialize)]
struct StartQuery {
    /// Optional URL to redirect the user to after a successful exchange.
    /// Defaults to a simple HTML success page.
    redirect_to: Option<String>,
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Kick off the OAuth2 Authorization Code + PKCE flow for the given
/// `integration_credentials` row. Redirects to the provider's authorize URL.
async fn start(
    State(state): State<Arc<AppState>>,
    Path(credential_id): Path<Uuid>,
    Query(q): Query<StartQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    // Load the credential row to get the provider's OAuth endpoints.
    let row = sqlx::query_as::<_, CredentialOAuthRow>(
        r#"SELECT id, oauth_auth_url, oauth_token_url, oauth_client_id,
                  oauth_client_secret, oauth_scopes
             FROM integration_credentials
            WHERE id = $1 AND auth_type = 'oauth2'"#,
    )
    .bind(credential_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "credential not found".to_string()))?;

    let auth_url = row.oauth_auth_url.ok_or((
        StatusCode::BAD_REQUEST,
        "oauth_auth_url not set".to_string(),
    ))?;
    let client_id = row.oauth_client_id.ok_or((
        StatusCode::BAD_REQUEST,
        "oauth_client_id not set".to_string(),
    ))?;

    let state_token = random_string(32);
    let code_verifier = random_string(64);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));

    // Persist state for the callback.
    sqlx::query(
        r#"INSERT INTO oauth_states (state_token, credential_id, code_verifier, redirect_to)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(&state_token)
    .bind(credential_id)
    .bind(&code_verifier)
    .bind(&q.redirect_to)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to store oauth state: {}", e),
        )
    })?;

    let redirect_uri = format!(
        "{}/api/v1/oauth/callback",
        state.config.oauth.redirect_base_url.trim_end_matches('/')
    );

    // Build authorize URL.
    let mut url = reqwest::Url::parse(&auth_url).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid oauth_auth_url: {}", e),
        )
    })?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &client_id);
        q.append_pair("redirect_uri", &redirect_uri);
        q.append_pair("response_type", "code");
        q.append_pair("state", &state_token);
        q.append_pair("code_challenge", &code_challenge);
        q.append_pair("code_challenge_method", "S256");
        if let Some(scopes) = row.oauth_scopes.as_deref() {
            if !scopes.is_empty() {
                // Providers accept space- or comma-separated; pass through as-is
                // (callers can format in the row however the provider wants).
                q.append_pair("scope", scopes);
            }
        }
    }

    Ok(Redirect::temporary(url.as_str()))
}

/// OAuth2 callback: exchanges the authorization code for tokens and stores
/// them on the `integration_credentials` row.
async fn callback(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CallbackQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    if let Some(err) = q.error {
        let desc = q.error_description.unwrap_or_default();
        return Ok(Html(format!(
            "<h1>Authorization failed</h1><p><strong>{}</strong>: {}</p>",
            html_escape(&err),
            html_escape(&desc)
        )));
    }

    let code = q.code.ok_or((
        StatusCode::BAD_REQUEST,
        "missing 'code' parameter".to_string(),
    ))?;
    let state_token = q.state.ok_or((
        StatusCode::BAD_REQUEST,
        "missing 'state' parameter".to_string(),
    ))?;

    // Look up and consume the state row.
    let state_row = sqlx::query_as::<_, OAuthStateRow>(
        r#"DELETE FROM oauth_states
            WHERE state_token = $1 AND expires_at > NOW()
            RETURNING credential_id, code_verifier, redirect_to"#,
    )
    .bind(&state_token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
    })?
    .ok_or((
        StatusCode::BAD_REQUEST,
        "state_token not found or expired".to_string(),
    ))?;

    // Load the credential row to get token URL and client secret.
    let cred_row = sqlx::query_as::<_, CredentialOAuthRow>(
        r#"SELECT id, oauth_auth_url, oauth_token_url, oauth_client_id,
                  oauth_client_secret, oauth_scopes
             FROM integration_credentials
            WHERE id = $1"#,
    )
    .bind(state_row.credential_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
    })?
    .ok_or((
        StatusCode::NOT_FOUND,
        "credential row disappeared".to_string(),
    ))?;

    let token_url = cred_row.oauth_token_url.ok_or((
        StatusCode::BAD_REQUEST,
        "oauth_token_url not set".to_string(),
    ))?;
    let client_id = cred_row.oauth_client_id.ok_or((
        StatusCode::BAD_REQUEST,
        "oauth_client_id not set".to_string(),
    ))?;

    let redirect_uri = format!(
        "{}/api/v1/oauth/callback",
        state.config.oauth.redirect_base_url.trim_end_matches('/')
    );

    // Exchange code for tokens.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("http: {}", e)))?;

    let mut form: Vec<(&str, String)> = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", state_row.code_verifier),
    ];
    if let Some(secret) = &cred_row.oauth_client_secret {
        if !secret.is_empty() {
            form.push(("client_secret", secret.clone()));
        }
    }

    let resp = client
        .post(&token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("token request: {}", e)))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("token body: {}", e)))?;

    if !status.is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("token exchange failed: HTTP {} — {}", status, body),
        ));
    }

    let token_json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("invalid token JSON: {} ({})", e, body),
        )
    })?;

    let access_token = token_json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or((
            StatusCode::BAD_GATEWAY,
            "token response missing access_token".to_string(),
        ))?
        .to_string();
    let refresh_token = token_json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let expires_in = token_json.get("expires_in").and_then(|v| v.as_i64());
    let expires_at = expires_in.map(|s| chrono::Utc::now() + chrono::Duration::seconds(s));

    // Store tokens on the credential row.
    sqlx::query(
        r#"UPDATE integration_credentials
              SET access_token = $1,
                  refresh_token = COALESCE($2, refresh_token),
                  token_expires_at = $3,
                  status = 'active',
                  last_rotated_at = NOW(),
                  updated_at = NOW()
            WHERE id = $4"#,
    )
    .bind(&access_token)
    .bind(&refresh_token)
    .bind(expires_at)
    .bind(state_row.credential_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
    })?;

    if let Some(redirect_to) = state_row.redirect_to.as_deref() {
        if !redirect_to.is_empty() {
            return Ok(Html(format!(
                "<!DOCTYPE html><meta http-equiv=\"refresh\" content=\"0;url={}\">\
                 <p>Connected. Redirecting…</p>",
                html_escape(redirect_to)
            )));
        }
    }

    Ok(Html(
        "<!DOCTYPE html><html><body style=\"font-family: system-ui; padding: 2rem\">\
         <h1>Connected</h1><p>You can close this tab and return to AMOS.</p>\
         </body></html>"
            .to_string(),
    ))
}

// ─── Helpers ─────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct CredentialOAuthRow {
    #[allow(dead_code)]
    id: Uuid,
    oauth_auth_url: Option<String>,
    oauth_token_url: Option<String>,
    oauth_client_id: Option<String>,
    oauth_client_secret: Option<String>,
    oauth_scopes: Option<String>,
}

#[derive(sqlx::FromRow)]
struct OAuthStateRow {
    credential_id: Uuid,
    code_verifier: String,
    redirect_to: Option<String>,
}

fn random_string(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_string_has_correct_length() {
        assert_eq!(random_string(32).len(), 32);
        assert_eq!(random_string(64).len(), 64);
    }

    #[test]
    fn random_strings_differ_between_calls() {
        let a = random_string(32);
        let b = random_string(32);
        assert_ne!(a, b);
    }

    #[test]
    fn html_escape_handles_common_chars() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }
}
