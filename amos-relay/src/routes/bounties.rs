//! Bounty marketplace routes.

use crate::{protocol_fees::calculate_fee, solana::SettlementParams, state::RelayState};
use amos_core::types::BountyStatus;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tracing::{info, warn};
use uuid::Uuid;

/// Build bounty routes.
pub fn routes() -> Router<RelayState> {
    Router::new()
        .route("/", post(create_bounty).get(list_bounties))
        .route("/{id}", get(get_bounty))
        .route("/{id}/claim", post(claim_bounty))
        .route("/{id}/submit", post(submit_work))
        .route("/{id}/approve", post(approve_submission))
        .route("/{id}/reject", post(reject_submission))
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateBountyRequest {
    pub title: String,
    pub description: String,
    pub reward_tokens: u64,
    pub deadline: DateTime<Utc>,
    pub required_capabilities: Vec<String>,
    pub poster_wallet: String,
}

/// Max lengths for input validation (prevents oversized payloads hitting the DB)
const MAX_TITLE_LEN: usize = 500;
const MAX_DESCRIPTION_LEN: usize = 50_000;
const MAX_CAPABILITY_LEN: usize = 100;
const MAX_CAPABILITIES_COUNT: usize = 20;
const MAX_REJECTION_REASON_LEN: usize = 5_000;
const MAX_RESULT_JSON_LEN: usize = 1_000_000; // 1MB

#[derive(Debug, Deserialize)]
pub struct ListBountiesQuery {
    pub status: Option<BountyStatus>,
    pub min_reward: Option<u64>,
    pub capability: Option<String>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ClaimBountyRequest {
    pub agent_id: Uuid,
    pub harness_id: String,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitWorkRequest {
    pub agent_id: Uuid,
    pub result: JsonValue,
    pub quality_evidence: Option<JsonValue>,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApproveSubmissionRequest {
    pub reviewer_wallet: String,
    pub quality_score: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct RejectSubmissionRequest {
    pub reviewer_wallet: String,
    pub reason: String,
}

// BountyStatus is re-exported from amos_core::types

#[derive(Debug, Serialize, Deserialize)]
pub struct BountyResponse {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub reward_tokens: i64,
    pub deadline: Option<DateTime<Utc>>,
    pub required_capabilities: Vec<String>,
    pub poster_wallet: Option<String>,
    pub status: BountyStatus,
    pub claimed_by_agent_id: Option<Uuid>,
    pub claimed_by_harness_id: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub result: Option<JsonValue>,
    pub quality_evidence: Option<JsonValue>,
    pub quality_score: Option<i16>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub claimed_by_wallet: Option<String>,
    pub settlement_tx: Option<String>,
    pub settlement_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// =============================================================================
// ROW MAPPING
// =============================================================================

/// Standard SELECT columns for bounty queries (maps DB column names to response fields).
const BOUNTY_SELECT: &str = r#"
    id, title, description, reward_tokens, deadline_at,
    required_capabilities, poster_wallet, status,
    claimed_by_agent_id, claimed_by_harness_id, claimed_by_wallet,
    submitted_at, result, quality_evidence,
    quality_score, approved_at, rejected_at, rejection_reason,
    settlement_tx, settlement_status,
    created_at, updated_at
"#;

fn bounty_from_row(row: sqlx::postgres::PgRow) -> Result<BountyResponse, sqlx::Error> {
    use sqlx::Row;
    let caps: serde_json::Value = row.try_get("required_capabilities")?;
    let caps_vec: Vec<String> = serde_json::from_value(caps).unwrap_or_default();
    let status_str: String = row.try_get("status")?;

    Ok(BountyResponse {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        reward_tokens: row.try_get("reward_tokens")?,
        deadline: row.try_get("deadline_at")?,
        required_capabilities: caps_vec,
        poster_wallet: row.try_get("poster_wallet")?,
        status: BountyStatus::from_str(&status_str),
        claimed_by_agent_id: row.try_get("claimed_by_agent_id")?,
        claimed_by_harness_id: row.try_get("claimed_by_harness_id")?,
        claimed_by_wallet: row.try_get("claimed_by_wallet")?,
        submitted_at: row.try_get("submitted_at")?,
        result: row.try_get("result")?,
        quality_evidence: row.try_get("quality_evidence")?,
        quality_score: row.try_get("quality_score")?,
        approved_at: row.try_get("approved_at")?,
        rejected_at: row.try_get("rejected_at")?,
        rejection_reason: row.try_get("rejection_reason")?,
        settlement_tx: row.try_get("settlement_tx")?,
        settlement_status: row.try_get("settlement_status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// =============================================================================
// HANDLERS
// =============================================================================

/// Create a new bounty.
async fn create_bounty(
    State(state): State<RelayState>,
    Json(req): Json<CreateBountyRequest>,
) -> Result<(StatusCode, Json<BountyResponse>), StatusCode> {
    // Input validation
    if req.title.len() > MAX_TITLE_LEN {
        warn!("Bounty title too long: {} chars", req.title.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.description.len() > MAX_DESCRIPTION_LEN {
        warn!(
            "Bounty description too long: {} chars",
            req.description.len()
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.required_capabilities.len() > MAX_CAPABILITIES_COUNT {
        warn!("Too many capabilities: {}", req.required_capabilities.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if req
        .required_capabilities
        .iter()
        .any(|c| c.len() > MAX_CAPABILITY_LEN)
    {
        warn!("Capability string too long");
        return Err(StatusCode::BAD_REQUEST);
    }
    if !crate::validate_wallet_address(&req.poster_wallet) {
        warn!("Invalid poster wallet address: {}", req.poster_wallet);
        return Err(StatusCode::BAD_REQUEST);
    }

    let bounty_id = Uuid::new_v4();
    let now = Utc::now();

    let caps_json = serde_json::to_value(&req.required_capabilities).unwrap_or_default();
    let row = sqlx::query(&format!(
        "INSERT INTO relay_bounties (
                id, title, description, reward_tokens, deadline_at,
                required_capabilities, poster_wallet, status,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING {BOUNTY_SELECT}"
    ))
    .bind(bounty_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(req.reward_tokens as i64)
    .bind(req.deadline)
    .bind(&caps_json)
    .bind(&req.poster_wallet)
    .bind(BountyStatus::Open.as_str())
    .bind(now)
    .bind(now)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to create bounty: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let bounty = bounty_from_row(row).map_err(|e| {
        warn!("Failed to map bounty row: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!(
        "Created bounty {} with reward {}",
        bounty_id, req.reward_tokens
    );

    Ok((StatusCode::CREATED, Json(bounty)))
}

/// List bounties with optional filters.
async fn list_bounties(
    State(state): State<RelayState>,
    Query(query): Query<ListBountiesQuery>,
) -> Result<Json<Vec<BountyResponse>>, StatusCode> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let rows = if let Some(ref status) = query.status {
        sqlx::query(
            &format!("SELECT {BOUNTY_SELECT} FROM relay_bounties WHERE status = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"),
        )
        .bind(status.as_str())
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            &format!("SELECT {BOUNTY_SELECT} FROM relay_bounties ORDER BY created_at DESC LIMIT $1 OFFSET $2"),
        )
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| {
        warn!("Failed to list bounties: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let bounties: Vec<BountyResponse> = rows
        .into_iter()
        .filter_map(|r| bounty_from_row(r).ok())
        .collect();
    Ok(Json(bounties))
}

/// Get a single bounty by ID.
async fn get_bounty(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BountyResponse>, StatusCode> {
    let row = sqlx::query(&format!(
        "SELECT {BOUNTY_SELECT} FROM relay_bounties WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to get bounty {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(bounty))
}

/// Claim a bounty for an agent.
async fn claim_bounty(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ClaimBountyRequest>,
) -> Result<Json<BountyResponse>, StatusCode> {
    // Validate wallet address if provided
    if let Some(ref addr) = req.wallet_address {
        if !crate::validate_wallet_address(addr) {
            warn!("Invalid wallet address in claim: {}", addr);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let now = Utc::now();

    let row = sqlx::query(
        &format!("UPDATE relay_bounties SET status = $1, claimed_by_agent_id = $2, claimed_by_harness_id = $3, claimed_by_wallet = $4, updated_at = $5 WHERE id = $6 AND status = $7 RETURNING {BOUNTY_SELECT}"),
    )
    .bind(BountyStatus::Claimed.as_str())
    .bind(req.agent_id)
    .bind(&req.harness_id)
    .bind(&req.wallet_address)
    .bind(now)
    .bind(id)
    .bind(BountyStatus::Open.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to claim bounty {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::CONFLICT)?;
    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!(
        "Bounty {} claimed by agent {} (wallet: {:?})",
        id, req.agent_id, req.wallet_address
    );

    Ok(Json(bounty))
}

/// Submit work for a claimed bounty.
async fn submit_work(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SubmitWorkRequest>,
) -> Result<Json<BountyResponse>, StatusCode> {
    // Validate wallet address if provided
    if let Some(ref addr) = req.wallet_address {
        if !crate::validate_wallet_address(addr) {
            warn!("Invalid wallet address in submission: {}", addr);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Validate result JSON size
    let result_str = req.result.to_string();
    if result_str.len() > MAX_RESULT_JSON_LEN {
        warn!("Submit result JSON too large: {} bytes", result_str.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(ref evidence) = req.quality_evidence {
        let evidence_str = evidence.to_string();
        if evidence_str.len() > MAX_RESULT_JSON_LEN {
            warn!(
                "Quality evidence JSON too large: {} bytes",
                evidence_str.len()
            );
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let now = Utc::now();

    // If wallet_address provided at submit time and not yet stored, update it
    let wallet_clause = if req.wallet_address.is_some() {
        ", claimed_by_wallet = COALESCE(claimed_by_wallet, $9)"
    } else {
        ""
    };
    let sql = format!("UPDATE relay_bounties SET status = $1, submitted_at = $2, result = $3, quality_evidence = $4, updated_at = $5{wallet_clause} WHERE id = $6 AND status = $7 AND claimed_by_agent_id = $8 RETURNING {BOUNTY_SELECT}");
    let mut query = sqlx::query(&sql)
        .bind(BountyStatus::Submitted.as_str())
        .bind(now)
        .bind(&req.result)
        .bind(&req.quality_evidence)
        .bind(now)
        .bind(id)
        .bind(BountyStatus::Claimed.as_str())
        .bind(req.agent_id);
    if let Some(ref wallet) = req.wallet_address {
        query = query.bind(wallet);
    }
    let row = query
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            warn!("Failed to submit work for bounty {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::CONFLICT)?;
    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("Work submitted for bounty {} by agent {}", id, req.agent_id);

    Ok(Json(bounty))
}

/// Approve a bounty submission and trigger payout.
async fn approve_submission(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ApproveSubmissionRequest>,
) -> Result<Json<BountyResponse>, StatusCode> {
    // Validate reviewer wallet format
    if !crate::validate_wallet_address(&req.reviewer_wallet) {
        warn!(
            "Invalid reviewer wallet in approval: {}",
            req.reviewer_wallet
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    let now = Utc::now();

    // Fetch the bounty with poster and claimer wallets for separation-of-duties checks
    let current_bounty = sqlx::query(
        r#"
        SELECT reward_tokens, poster_wallet, claimed_by_wallet
        FROM relay_bounties
        WHERE id = $1 AND status = $2
        "#,
    )
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to fetch bounty {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    // --- Separation of duties: prevent self-approval ---

    // 1. Poster cannot approve their own bounty
    let poster_wallet: Option<String> = current_bounty.get("poster_wallet");
    if let Some(ref poster) = poster_wallet {
        if poster == &req.reviewer_wallet {
            warn!(
                "Self-approval blocked: poster {} tried to approve bounty {}",
                req.reviewer_wallet, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // 2. Claimer/submitter cannot approve their own submission
    let claimed_by_wallet: Option<String> = current_bounty.get("claimed_by_wallet");
    if let Some(ref claimer) = claimed_by_wallet {
        if claimer == &req.reviewer_wallet {
            warn!(
                "Self-approval blocked: claimer {} tried to approve bounty {}",
                req.reviewer_wallet, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // 3. Reviewer must be a registered agent with trust level >= 3
    let reviewer_trust: Option<i16> = sqlx::query_scalar(
        "SELECT trust_level FROM relay_agents WHERE wallet_address = $1 AND status = 'active'",
    )
    .bind(&req.reviewer_wallet)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to look up reviewer trust level: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match reviewer_trust {
        None => {
            warn!(
                "Reviewer {} is not a registered agent — cannot approve bounty {}",
                req.reviewer_wallet, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
        Some(level) if level < 3 => {
            warn!(
                "Reviewer {} has trust level {} (need >= 3) — cannot approve bounty {}",
                req.reviewer_wallet, level, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
        _ => {} // trust level >= 3, proceed
    }

    // Calculate protocol fee
    let reward_tokens: i64 = current_bounty.get("reward_tokens");
    let reward_tokens = reward_tokens as u64;
    let fee = calculate_fee(reward_tokens);

    info!(
        "Approving bounty {}: reward={}, protocol_fee={}, holder_share={}, burn_share={}, labs_share={}",
        id, reward_tokens, fee.total_fee, fee.holder_share, fee.burn_share, fee.labs_share
    );

    // Update the bounty status
    let row = sqlx::query(
        &format!("UPDATE relay_bounties SET status = $1, approved_at = $2, quality_score = $3, updated_at = $4 WHERE id = $5 AND status = $6 RETURNING {BOUNTY_SELECT}"),
    )
    .bind(BountyStatus::Approved.as_str())
    .bind(now)
    .bind(req.quality_score.map(|s| s as i16))
    .bind(now)
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to approve bounty {}: {}", e, id);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::CONFLICT)?;
    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Record protocol fee in the ledger
    let fee_id = Uuid::new_v4();
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO protocol_fee_ledger (id, bounty_id, total_fee, holder_share, burn_share, labs_share)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(fee_id)
    .bind(id)
    .bind(fee.total_fee as i64)
    .bind(fee.holder_share as i64)
    .bind(fee.burn_share as i64)
    .bind(fee.labs_share as i64)
    .execute(&state.db)
    .await
    {
        warn!("Failed to record protocol fee: {}", e);
    }

    // Trigger Solana settlement if configured
    let mut settlement_tx: Option<String> = None;
    if let Some(ref solana) = state.solana {
        if solana.is_settlement_ready() {
            // Prefer wallet stored directly on the bounty claim; fall back to relay_agents lookup
            let agent_wallet = if let Some(ref w) = bounty.claimed_by_wallet {
                Some(w.clone())
            } else if let Some(agent_id) = bounty.claimed_by_agent_id {
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT wallet_address FROM relay_agents WHERE id = $1",
                )
                .bind(agent_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .flatten()
            } else {
                None
            };

            if let Some(wallet) = agent_wallet {
                // Hash the bounty ID and agent ID for on-chain records
                let bounty_id_str = id.to_string();
                let agent_id_bytes = {
                    let mut hasher = Sha256::new();
                    hasher.update(
                        bounty
                            .claimed_by_agent_id
                            .map(|a| a.to_string())
                            .unwrap_or_default()
                            .as_bytes(),
                    );
                    let result = hasher.finalize();
                    let mut out = [0u8; 32];
                    out.copy_from_slice(&result);
                    out
                };
                let evidence_hash = {
                    let mut hasher = Sha256::new();
                    hasher.update(
                        serde_json::to_string(&bounty.result)
                            .unwrap_or_default()
                            .as_bytes(),
                    );
                    let result = hasher.finalize();
                    let mut out = [0u8; 32];
                    out.copy_from_slice(&result);
                    out
                };

                // Look up agent's trust level to cap points per on-chain limits
                let agent_trust_level: i16 = if let Some(aid) = bounty.claimed_by_agent_id {
                    sqlx::query_scalar::<_, i16>(
                        "SELECT trust_level FROM relay_agents WHERE id = $1",
                    )
                    .bind(aid)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(1)
                } else {
                    1
                };

                // On-chain max points per trust level: [100, 200, 500, 1000, 2000]
                let max_for_trust = match agent_trust_level {
                    1 => 100u64,
                    2 => 200,
                    3 => 500,
                    4 => 1000,
                    _ => 2000,
                };
                let base_points = (reward_tokens.min(max_for_trust)) as u16;

                // max_reward = reward_tokens in whole AMOS × 10^9 decimals
                let max_reward = reward_tokens.saturating_mul(1_000_000_000);

                let params = SettlementParams {
                    bounty_id: bounty_id_str,
                    agent_wallet: wallet,
                    reviewer_wallet: req.reviewer_wallet.clone(),
                    base_points,
                    quality_score: req.quality_score.unwrap_or(70),
                    contribution_type: 1, // default: feature
                    is_agent: true,
                    agent_id: agent_id_bytes,
                    evidence_hash,
                    max_reward,
                };

                match solana.process_bounty_payout(&params).await {
                    Ok(result) => {
                        settlement_tx = Some(result.tx_signature.clone());
                        info!(
                            bounty_id = %id,
                            tx = %result.tx_signature,
                            "On-chain settlement successful"
                        );

                        // Update fee ledger with settlement tx
                        let _ = sqlx::query(
                            "UPDATE protocol_fee_ledger SET settled_on_chain = true, settlement_tx = $1 WHERE id = $2",
                        )
                        .bind(&result.tx_signature)
                        .bind(fee_id)
                        .execute(&state.db)
                        .await;

                        // Update bounty with settlement info
                        let _ = sqlx::query(
                            "UPDATE relay_bounties SET settlement_tx = $1, settlement_status = 'settled' WHERE id = $2",
                        )
                        .bind(&result.tx_signature)
                        .bind(id)
                        .execute(&state.db)
                        .await;
                    }
                    Err(e) => {
                        warn!(
                            bounty_id = %id,
                            error = %e,
                            "On-chain settlement failed — bounty approved but tokens not distributed"
                        );
                        // Mark as failed for retry
                        let _ = sqlx::query(
                            "UPDATE relay_bounties SET settlement_status = 'failed' WHERE id = $1",
                        )
                        .bind(id)
                        .execute(&state.db)
                        .await;
                    }
                }
            } else {
                warn!(
                    bounty_id = %id,
                    "Agent has no wallet address — cannot settle on-chain"
                );
            }
        } else {
            info!(
                bounty_id = %id,
                "Solana settlement not fully configured — fee recorded in ledger only"
            );
        }
    }

    info!(
        bounty_id = %id,
        reward = reward_tokens,
        fee = fee.total_fee,
        settlement = ?settlement_tx,
        "Bounty approved"
    );

    Ok(Json(bounty))
}

/// Reject a bounty submission.
async fn reject_submission(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<RejectSubmissionRequest>,
) -> Result<Json<BountyResponse>, StatusCode> {
    if req.reason.len() > MAX_REJECTION_REASON_LEN {
        warn!("Rejection reason too long: {} chars", req.reason.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    if !crate::validate_wallet_address(&req.reviewer_wallet) {
        warn!("Invalid reviewer wallet in rejection");
        return Err(StatusCode::BAD_REQUEST);
    }

    // --- Separation of duties: same rules as approve ---

    // Fetch poster and claimer wallets
    let bounty_check = sqlx::query(
        "SELECT poster_wallet, claimed_by_wallet FROM relay_bounties WHERE id = $1 AND status = $2",
    )
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to fetch bounty for rejection check {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Poster cannot reject their own bounty (could be used to grief claimers)
    let poster_wallet: Option<String> = bounty_check.get("poster_wallet");
    if let Some(ref poster) = poster_wallet {
        if poster == &req.reviewer_wallet {
            warn!(
                "Self-rejection blocked: poster {} tried to reject bounty {}",
                req.reviewer_wallet, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // Claimer cannot reject their own submission (use withdraw instead)
    let claimed_by_wallet: Option<String> = bounty_check.get("claimed_by_wallet");
    if let Some(ref claimer) = claimed_by_wallet {
        if claimer == &req.reviewer_wallet {
            warn!(
                "Self-rejection blocked: claimer {} tried to reject bounty {}",
                req.reviewer_wallet, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // Reviewer must be a registered agent with trust level >= 3
    let reviewer_trust: Option<i16> = sqlx::query_scalar(
        "SELECT trust_level FROM relay_agents WHERE wallet_address = $1 AND status = 'active'",
    )
    .bind(&req.reviewer_wallet)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to look up reviewer trust level: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match reviewer_trust {
        None => {
            warn!(
                "Reviewer {} is not a registered agent — cannot reject bounty {}",
                req.reviewer_wallet, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
        Some(level) if level < 3 => {
            warn!(
                "Reviewer {} has trust level {} (need >= 3) — cannot reject bounty {}",
                req.reviewer_wallet, level, id
            );
            return Err(StatusCode::FORBIDDEN);
        }
        _ => {}
    }

    let now = Utc::now();

    let row = sqlx::query(
        &format!("UPDATE relay_bounties SET status = $1, rejected_at = $2, rejection_reason = $3, updated_at = $4 WHERE id = $5 AND status = $6 RETURNING {BOUNTY_SELECT}"),
    )
    .bind(BountyStatus::Rejected.as_str())
    .bind(now)
    .bind(&req.reason)
    .bind(now)
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to reject bounty {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::CONFLICT)?;
    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("Bounty {} rejected by reviewer {}", id, req.reviewer_wallet);

    Ok(Json(bounty))
}
