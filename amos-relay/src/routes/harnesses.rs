//! Harness connection management routes.

use crate::{
    identity::Principal,
    middleware::{generate_api_key, hash_api_key},
    state::RelayState,
};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Build harness routes.
pub fn routes() -> Router<RelayState> {
    Router::new()
        .route("/connect", post(connect_harness))
        .route("/", get(list_harnesses))
        .route("/{id}", get(get_harness))
        .route("/{id}/heartbeat", post(harness_heartbeat))
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct ConnectHarnessRequest {
    pub harness_id: String,
    pub name: String,
    pub version: String,
    pub endpoint_url: String,
    /// Legacy input is ignored; the Relay generates its own credentials.
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HarnessHeartbeatRequest {
    pub version: String,
    pub healthy: bool,
    pub agent_count: u32,
    pub active_bounties: u32,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct HarnessResponse {
    pub harness_id: String,
    pub name: String,
    pub version: String,
    pub endpoint_url: String,
    pub healthy: bool,
    pub agent_count: i32,
    pub active_bounties: i32,
    pub connected_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct ConnectedHarness {
    #[serde(flatten)]
    harness: HarnessResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

// =============================================================================
// HANDLERS
// =============================================================================

/// Connect a harness to the relay.
async fn connect_harness(
    State(state): State<RelayState>,
    principal: Option<Extension<Principal>>,
    Json(req): Json<ConnectHarnessRequest>,
) -> Result<(StatusCode, Json<ConnectedHarness>), StatusCode> {
    // Input validation
    if req.harness_id.is_empty() || req.harness_id.len() > 255 {
        warn!("Invalid harness_id length: {}", req.harness_id.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.name.is_empty() || req.name.len() > 255 {
        warn!("Invalid harness name length: {}", req.name.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.version.is_empty() || req.version.len() > 50 {
        warn!("Invalid harness version length: {}", req.version.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.endpoint_url.is_empty() || req.endpoint_url.len() > 500 {
        warn!(
            "Invalid harness endpoint_url length: {}",
            req.endpoint_url.len()
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    let now = Utc::now();

    let existing: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM relay_harnesses WHERE harness_id = $1)")
            .bind(&req.harness_id)
            .fetch_one(&state.db)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let columns = "harness_id, name, version, endpoint_url, healthy, agent_count, active_bounties, connected_at, last_heartbeat";
    let api_key = if existing {
        None
    } else {
        Some(generate_api_key("harness"))
    };
    let harness = if existing {
        principal.as_ref().ok_or(StatusCode::FORBIDDEN)?.0.require_harness(&req.harness_id)?;
        sqlx::query_as::<_, HarnessResponse>(&format!("UPDATE relay_harnesses SET name=$2, version=$3, endpoint_url=$4, last_heartbeat=$5 WHERE harness_id=$1 AND status IN ('active','inactive') RETURNING {columns}"))
            .bind(&req.harness_id).bind(&req.name).bind(&req.version).bind(&req.endpoint_url).bind(now)
            .fetch_optional(&state.db).await
    } else {
        sqlx::query_as::<_, HarnessResponse>(&format!("INSERT INTO relay_harnesses (harness_id,name,version,endpoint_url,api_key_hash,healthy,agent_count,active_bounties,connected_at,last_heartbeat) VALUES ($1,$2,$3,$4,$5,true,0,0,$6,$6) ON CONFLICT(harness_id) DO NOTHING RETURNING {columns}"))
            .bind(&req.harness_id).bind(&req.name).bind(&req.version).bind(&req.endpoint_url)
            .bind(hash_api_key(api_key.as_deref().expect("new harness key"))).bind(now)
            .fetch_optional(&state.db).await
    }.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?.ok_or(StatusCode::CONFLICT)?;

    info!(
        "Harness {} ({}) connected at {}",
        req.harness_id, req.name, req.endpoint_url
    );

    Ok((
        StatusCode::CREATED,
        Json(ConnectedHarness { harness, api_key }),
    ))
}

/// List all connected harnesses.
async fn list_harnesses(
    State(state): State<RelayState>,
) -> Result<Json<Vec<HarnessResponse>>, StatusCode> {
    let harnesses = sqlx::query_as::<_, HarnessResponse>(
        r#"
        SELECT
            harness_id, name, version, endpoint_url,
            healthy, agent_count, active_bounties,
            connected_at, last_heartbeat
        FROM relay_harnesses
        ORDER BY connected_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to list harnesses: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(harnesses))
}

/// Get a single harness by ID.
async fn get_harness(
    State(state): State<RelayState>,
    Path(id): Path<String>,
) -> Result<Json<HarnessResponse>, StatusCode> {
    let harness = sqlx::query_as::<_, HarnessResponse>(
        r#"
        SELECT
            harness_id, name, version, endpoint_url,
            healthy, agent_count, active_bounties,
            connected_at, last_heartbeat
        FROM relay_harnesses
        WHERE harness_id = $1
        "#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to get harness {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(harness))
}

/// Harness heartbeat to report health and metrics.
async fn harness_heartbeat(
    State(state): State<RelayState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    Json(req): Json<HarnessHeartbeatRequest>,
) -> Result<Json<HarnessResponse>, StatusCode> {
    principal.require_harness(&id)?;
    let now = Utc::now();

    let harness = sqlx::query_as::<_, HarnessResponse>(
        r#"
        UPDATE relay_harnesses
        SET
            version = $1,
            healthy = $2,
            agent_count = $3,
            active_bounties = $4,
            last_heartbeat = $5
        WHERE harness_id = $6
        RETURNING
            harness_id, name, version, endpoint_url,
            healthy, agent_count, active_bounties,
            connected_at, last_heartbeat
        "#,
    )
    .bind(&req.version)
    .bind(req.healthy)
    .bind(req.agent_count as i32)
    .bind(req.active_bounties as i32)
    .bind(now)
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to update heartbeat for harness {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(harness))
}
