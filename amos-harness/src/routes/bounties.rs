//! Bounty proxy routes — forwards bounty API calls to the AMOS Network Relay.
//!
//! The harness acts as a transparent proxy so the frontend canvas and agent
//! can interact with bounties without knowing the relay URL directly.

use crate::middleware::Claims;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use std::sync::Arc;

/// Build bounty proxy routes.
pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_bounties).post(create_bounty))
        .route("/{id}", get(get_bounty))
        .route("/{id}/claim", post(claim_bounty))
        .route("/{id}/submit", post(submit_work))
        .route("/{id}/approve", post(approve_submission))
        .route("/{id}/reject", post(reject_submission))
}

/// Forward GET /api/v1/bounties to relay.
async fn list_bounties(
    State(state): State<Arc<AppState>>,
    Query(params): Query<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let relay_url = &state.config.relay.url;
    let url = format!("{}/api/v1/bounties", relay_url);

    let client = reqwest::Client::new();
    let resp = client.get(&url).query(&params).send().await.map_err(|e| {
        tracing::warn!("Relay bounty list failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    if status.is_success() {
        Ok(Json(body))
    } else {
        Err(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
    }
}

/// Forward POST /api/v1/bounties to relay.
async fn create_bounty(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let relay_url = &state.config.relay.url;
    let url = format!("{}/api/v1/bounties", relay_url);

    let client = reqwest::Client::new();
    let resp = client.post(&url).json(&payload).send().await.map_err(|e| {
        tracing::warn!("Relay bounty create failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    if status.is_success() {
        Ok((StatusCode::CREATED, Json(body)))
    } else {
        Err(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
    }
}

/// Forward GET /api/v1/bounties/:id to relay.
async fn get_bounty(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    proxy_get(&state.config.relay.url, &format!("/api/v1/bounties/{}", id)).await
}

/// Forward POST /api/v1/bounties/:id/claim to relay.
/// Injects the user's connected wallet address if available.
async fn claim_bounty(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let payload = inject_wallet_address(&state, &claims, payload).await;
    proxy_post(
        &state.config.relay.url,
        &format!("/api/v1/bounties/{}/claim", id),
        &payload,
    )
    .await
}

/// Forward POST /api/v1/bounties/:id/submit to relay.
/// Injects the user's connected wallet address if available.
async fn submit_work(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let payload = inject_wallet_address(&state, &claims, payload).await;
    proxy_post(
        &state.config.relay.url,
        &format!("/api/v1/bounties/{}/submit", id),
        &payload,
    )
    .await
}

/// Forward POST /api/v1/bounties/:id/approve to relay.
async fn approve_submission(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    proxy_post(
        &state.config.relay.url,
        &format!("/api/v1/bounties/{}/approve", id),
        &payload,
    )
    .await
}

/// Forward POST /api/v1/bounties/:id/reject to relay.
async fn reject_submission(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    proxy_post(
        &state.config.relay.url,
        &format!("/api/v1/bounties/{}/reject", id),
        &payload,
    )
    .await
}

/// Helper: proxy a GET request to the relay.
async fn proxy_get(relay_url: &str, path: &str) -> Result<Json<serde_json::Value>, StatusCode> {
    let url = format!("{}{}", relay_url, path);
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await.map_err(|e| {
        tracing::warn!("Relay proxy GET {} failed: {}", path, e);
        StatusCode::BAD_GATEWAY
    })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    if status.is_success() {
        Ok(Json(body))
    } else {
        Err(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
    }
}

/// Helper: proxy a POST request with JSON body to the relay.
async fn proxy_post(
    relay_url: &str,
    path: &str,
    payload: &serde_json::Value,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let url = format!("{}{}", relay_url, path);
    let client = reqwest::Client::new();
    let resp = client.post(&url).json(payload).send().await.map_err(|e| {
        tracing::warn!("Relay proxy POST {} failed: {}", path, e);
        StatusCode::BAD_GATEWAY
    })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    if status.is_success() {
        Ok(Json(body))
    } else {
        Err(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
    }
}

/// Look up the user's connected wallet and inject it into the payload.
/// If no wallet is connected, the payload is returned unchanged.
async fn inject_wallet_address(
    state: &AppState,
    claims: &Claims,
    mut payload: serde_json::Value,
) -> serde_json::Value {
    if let Ok(tenant_id) = claims.tenant_id.parse::<uuid::Uuid>() {
        let wallet: Option<String> = sqlx::query_scalar(
            "SELECT wallet_address FROM wallet_connections WHERE tenant_id = $1 AND is_primary = true",
        )
        .bind(tenant_id)
        .fetch_optional(&state.db_pool)
        .await
        .ok()
        .flatten();

        if let Some(addr) = wallet {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "wallet_address".to_string(),
                    serde_json::Value::String(addr),
                );
            }
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routes_build() {
        // Verify routes compile and build without panic
        // (actual integration tests require running relay)
        let _ = Router::<Arc<AppState>>::new()
            .route("/", get(list_bounties).post(create_bounty))
            .route("/{id}", get(get_bounty))
            .route("/{id}/claim", post(claim_bounty));
    }
}
