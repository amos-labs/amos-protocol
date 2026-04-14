//! Global agent directory routes.

use crate::state::RelayState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{info, warn};
use uuid::Uuid;

/// Build agent routes.
pub fn routes() -> Router<RelayState> {
    Router::new()
        .route("/register", post(register_agent))
        .route("/", get(list_agents))
        .route("/{id}", get(get_agent))
        .route("/{id}/heartbeat", post(agent_heartbeat))
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    pub display_name: String,
    pub endpoint_url: String,
    pub capabilities: Vec<String>,
    pub description: Option<String>,
    pub wallet_address: String,
    pub harness_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListAgentsQuery {
    pub capability: Option<String>,
    pub trust_level: Option<u8>,
    pub status: Option<String>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub id: Uuid,
    pub name: String,
    pub display_name: Option<String>,
    pub endpoint_url: Option<String>,
    pub capabilities: Vec<String>,
    pub description: Option<String>,
    pub wallet_address: Option<String>,
    pub harness_id: Option<Uuid>,
    pub trust_level: i16,
    pub status: String,
    pub total_bounties_completed: i64,
    pub avg_quality_score: f64,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
}

const AGENT_SELECT: &str = r#"
    id, name, display_name, endpoint_url, capabilities,
    description, wallet_address, harness_id, trust_level,
    status, total_bounties_completed, avg_quality_score,
    registered_at, last_heartbeat
"#;

fn agent_from_row(row: sqlx::postgres::PgRow) -> Result<AgentResponse, sqlx::Error> {
    let caps: serde_json::Value = row.try_get("capabilities")?;
    let caps_vec: Vec<String> = serde_json::from_value(caps).unwrap_or_default();

    Ok(AgentResponse {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        display_name: row.try_get("display_name")?,
        endpoint_url: row.try_get("endpoint_url")?,
        capabilities: caps_vec,
        description: row.try_get("description")?,
        wallet_address: row.try_get("wallet_address")?,
        harness_id: row.try_get("harness_id")?,
        trust_level: row.try_get("trust_level")?,
        status: row.try_get("status")?,
        total_bounties_completed: row.try_get("total_bounties_completed")?,
        avg_quality_score: row.try_get("avg_quality_score")?,
        registered_at: row.try_get("registered_at")?,
        last_heartbeat: row.try_get("last_heartbeat")?,
    })
}

// =============================================================================
// HANDLERS
// =============================================================================

/// Register a new agent in the global directory.
async fn register_agent(
    State(state): State<RelayState>,
    Json(req): Json<RegisterAgentRequest>,
) -> Result<(StatusCode, Json<AgentResponse>), StatusCode> {
    // Validate wallet address format
    if !crate::validate_wallet_address(&req.wallet_address) {
        warn!(
            "Invalid wallet address in agent registration: {}",
            req.wallet_address
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    let agent_id = Uuid::new_v4();
    let now = Utc::now();
    let caps_json = serde_json::to_value(&req.capabilities).unwrap_or_default();

    let row = sqlx::query(&format!(
        "INSERT INTO relay_agents (
                id, name, display_name, endpoint_url, capabilities,
                description, wallet_address, harness_id, trust_level,
                status, total_bounties_completed, avg_quality_score,
                registered_at, last_heartbeat
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING {AGENT_SELECT}"
    ))
    .bind(agent_id)
    .bind(&req.name)
    .bind(&req.display_name)
    .bind(&req.endpoint_url)
    .bind(&caps_json)
    .bind(&req.description)
    .bind(&req.wallet_address)
    .bind(req.harness_id)
    .bind(1i16) // Start at trust level 1 (Newcomer)
    .bind("active")
    .bind(0i64)
    .bind(0.0f64)
    .bind(now)
    .bind(now)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to register agent: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let agent = agent_from_row(row).map_err(|e| {
        warn!("Failed to map agent row: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!(
        "Registered agent {} ({}) on harness {:?}",
        agent_id, req.name, req.harness_id
    );

    Ok((StatusCode::CREATED, Json(agent)))
}

/// List agents with optional filters.
async fn list_agents(
    State(state): State<RelayState>,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Json<Vec<AgentResponse>>, StatusCode> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let rows = sqlx::query(&format!(
        "SELECT {AGENT_SELECT} FROM relay_agents ORDER BY registered_at DESC LIMIT $1 OFFSET $2"
    ))
    .bind(per_page as i64)
    .bind(offset as i64)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to list agents: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let agents: Vec<AgentResponse> = rows
        .into_iter()
        .filter_map(|r| agent_from_row(r).ok())
        .collect();
    Ok(Json(agents))
}

/// Get a single agent by ID.
async fn get_agent(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AgentResponse>, StatusCode> {
    let row = sqlx::query(&format!(
        "SELECT {AGENT_SELECT} FROM relay_agents WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to get agent {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let agent = agent_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(agent))
}

/// Agent heartbeat to indicate it's still active.
async fn agent_heartbeat(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<AgentResponse>, StatusCode> {
    let now = Utc::now();

    let row = if let Some(ref status) = req.status {
        sqlx::query(
            &format!("UPDATE relay_agents SET last_heartbeat = $1, status = $2 WHERE id = $3 RETURNING {AGENT_SELECT}"),
        )
        .bind(now)
        .bind(status)
        .bind(id)
        .fetch_optional(&state.db)
        .await
    } else {
        sqlx::query(
            &format!("UPDATE relay_agents SET last_heartbeat = $1 WHERE id = $2 RETURNING {AGENT_SELECT}"),
        )
        .bind(now)
        .bind(id)
        .fetch_optional(&state.db)
        .await
    }
    .map_err(|e| {
        warn!("Failed to update heartbeat for agent {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let agent = agent_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(agent))
}
