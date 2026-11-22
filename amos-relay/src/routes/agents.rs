//! Global agent directory routes.

use crate::{
    identity::Principal,
    middleware::{generate_api_key, hash_api_key},
    state::RelayState,
};
use axum::{
    extract::{Extension, Path, Query, State},
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
        .route("/challenge", post(registration_challenge))
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
    pub wallet_proof: WalletProof,
}

#[derive(Debug, Deserialize)]
pub struct WalletProof {
    pub challenge_id: Uuid,
    /// Base58 Ed25519 signature over the exact UTF-8 challenge message.
    pub signature: String,
}

#[derive(Debug, Deserialize)]
struct ChallengeRequest {
    wallet_address: String,
}

#[derive(Serialize)]
struct ChallengeResponse {
    challenge_id: Uuid,
    message: String,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct RegisteredAgent {
    #[serde(flatten)]
    agent: AgentResponse,
    /// Returned once. Never part of the public directory response.
    api_key: String,
}

fn challenge_message(id: Uuid, wallet: &str, nonce: &str, expires: DateTime<Utc>) -> String {
    format!("AMOS Relay agent registration v1\nchallenge:{id}\nwallet:{wallet}\nnonce:{nonce}\nexpires:{}", expires.timestamp())
}

fn verify_wallet_signature(wallet: &str, message: &str, signature: &str) -> bool {
    use std::str::FromStr;
    let (Ok(key), Ok(sig)) = (
        solana_sdk::pubkey::Pubkey::from_str(wallet),
        solana_sdk::signature::Signature::from_str(signature),
    ) else {
        return false;
    };
    sig.verify(key.as_ref(), message.as_bytes())
}

async fn registration_challenge(
    State(state): State<RelayState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, StatusCode> {
    if !crate::validate_wallet_address(&req.wallet_address) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let id = Uuid::new_v4();
    let expires = Utc::now() + chrono::Duration::minutes(5);
    let message = challenge_message(id, &req.wallet_address, &generate_api_key("nonce"), expires);
    sqlx::query("DELETE FROM relay_identity_challenges WHERE expires_at < now()")
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    sqlx::query("INSERT INTO relay_identity_challenges (id, wallet_address, message, expires_at) VALUES ($1, $2, $3, $4)")
        .bind(id).bind(&req.wallet_address).bind(&message).bind(expires)
        .execute(&state.db).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(ChallengeResponse {
        challenge_id: id,
        message,
        expires_at: expires,
    }))
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
    principal: Option<Extension<Principal>>,
    Json(req): Json<RegisterAgentRequest>,
) -> Result<(StatusCode, Json<RegisteredAgent>), StatusCode> {
    // Validate wallet address format
    if !crate::validate_wallet_address(&req.wallet_address) {
        warn!(
            "Invalid wallet address in agent registration: {}",
            req.wallet_address
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    // Input length validation
    if req.name.len() > 255 {
        warn!("Agent name too long: {} chars", req.name.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.display_name.len() > 255 {
        warn!(
            "Agent display_name too long: {} chars",
            req.display_name.len()
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.endpoint_url.len() > 500 {
        warn!(
            "Agent endpoint_url too long: {} chars",
            req.endpoint_url.len()
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.capabilities.len() > 20 {
        warn!("Too many agent capabilities: {}", req.capabilities.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.capabilities.iter().any(|c| c.len() > 100) {
        warn!("Agent capability string too long");
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(ref desc) = req.description {
        if desc.len() > 5000 {
            warn!("Agent description too long: {} chars", desc.len());
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let agent_id = Uuid::new_v4();
    let now = Utc::now();
    let caps_json = serde_json::to_value(&req.capabilities).unwrap_or_default();
    // A caller cannot attach itself to somebody else's reporting harness.
    if let Some(harness_id) = req.harness_id {
        let name: Option<String> = sqlx::query_scalar(
            "SELECT harness_id FROM relay_harnesses WHERE id = $1 AND status = 'active'",
        )
        .bind(harness_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let name = name.ok_or(StatusCode::FORBIDDEN)?;
        principal
            .as_ref()
            .ok_or(StatusCode::FORBIDDEN)?
            .0
            .require_harness(&name)?;
    }
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // Consume inside the registration transaction: expired, wrong-wallet, and
    // replayed proofs fail closed, including concurrent copies of a signature.
    let challenge: Option<(String,)> = sqlx::query_as("DELETE FROM relay_identity_challenges WHERE id = $1 AND wallet_address = $2 AND expires_at > now() RETURNING message")
        .bind(req.wallet_proof.challenge_id).bind(&req.wallet_address)
        .fetch_optional(&mut *tx).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let (message,) = challenge.ok_or(StatusCode::UNAUTHORIZED)?;
    if !verify_wallet_signature(&req.wallet_address, &message, &req.wallet_proof.signature) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let api_key = generate_api_key("agent");

    let row = sqlx::query(&format!(
        "INSERT INTO relay_agents (
                id, name, display_name, endpoint_url, capabilities,
                description, wallet_address, harness_id, trust_level,
                status, total_bounties_completed, avg_quality_score,
                registered_at, last_heartbeat, api_key_hash, wallet_verified
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, true)
            ON CONFLICT (wallet_address) WHERE wallet_address IS NOT NULL DO UPDATE SET
                name = EXCLUDED.name, display_name = EXCLUDED.display_name,
                endpoint_url = EXCLUDED.endpoint_url, capabilities = EXCLUDED.capabilities,
                description = EXCLUDED.description, harness_id = EXCLUDED.harness_id,
                api_key_hash = EXCLUDED.api_key_hash, wallet_verified = true,
                last_heartbeat = EXCLUDED.last_heartbeat
            WHERE relay_agents.status IN ('active', 'inactive')
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
    .bind(hash_api_key(&api_key))
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        warn!("Failed to register agent: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::FORBIDDEN)?;
    let agent = agent_from_row(row).map_err(|e| {
        warn!("Failed to map agent row: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let agent_id = agent.id;
    info!(
        "Registered agent {} ({}) on harness {:?}",
        agent_id, req.name, req.harness_id
    );

    // Register agent trust on-chain (non-blocking — on-chain is supplementary)
    if let Some(ref solana) = state.solana {
        let solana = solana.clone();
        let db = state.db.clone();
        let wallet = req.wallet_address.clone();
        let aid = agent_id;
        tokio::spawn(async move {
            match solana.register_agent_on_chain(&wallet).await {
                Ok(tx_sig) => {
                    info!(agent_id = %aid, tx = %tx_sig, "Agent trust registered on-chain");
                    let _ =
                        sqlx::query("UPDATE relay_agents SET onchain_trust_tx = $1 WHERE id = $2")
                            .bind(&tx_sig)
                            .bind(aid)
                            .execute(&db)
                            .await;
                }
                Err(e) => {
                    warn!(agent_id = %aid, error = %e, "Failed to register agent on-chain (non-critical)");
                }
            }
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(RegisteredAgent { agent, api_key }),
    ))
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

/// Valid agent status values.
const VALID_AGENT_STATUSES: &[&str] = &["active", "inactive", "suspended"];

/// Agent heartbeat to indicate it's still active.
async fn agent_heartbeat(
    State(state): State<RelayState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<AgentResponse>, StatusCode> {
    principal.require_agent(id)?;
    // Validate status if provided
    if let Some(ref status) = req.status {
        if !VALID_AGENT_STATUSES.contains(&status.as_str()) {
            warn!("Invalid agent status in heartbeat: {}", status);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

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

#[cfg(test)]
mod identity_tests {
    use super::*;
    use solana_sdk::signature::{Keypair, Signer};

    #[test]
    fn wallet_proof_binds_wallet_and_exact_challenge_bytes() {
        let signer = Keypair::new();
        let other = Keypair::new();
        let wallet = signer.pubkey().to_string();
        let message = challenge_message(Uuid::new_v4(), &wallet, "nonce", Utc::now());
        let signature = signer.sign_message(message.as_bytes()).to_string();
        assert!(verify_wallet_signature(&wallet, &message, &signature));
        assert!(!verify_wallet_signature(
            &other.pubkey().to_string(),
            &message,
            &signature
        ));
        assert!(!verify_wallet_signature(
            &wallet,
            &(message.clone() + "changed"),
            &signature
        ));
        assert!(!verify_wallet_signature(
            &wallet,
            &message,
            &other.sign_message(message.as_bytes()).to_string()
        ));
        assert!(!verify_wallet_signature(&wallet, &message, "invalid"));
    }

    #[test]
    fn registration_requires_proof_and_directory_shape_has_no_credential() {
        let req = serde_json::json!({"name":"a","display_name":"A","endpoint_url":"https://example.test","capabilities":[],"wallet_address":Keypair::new().pubkey().to_string()});
        assert!(serde_json::from_value::<RegisterAgentRequest>(req).is_err());
        assert!(!AGENT_SELECT.contains("api_key"));
    }
}
