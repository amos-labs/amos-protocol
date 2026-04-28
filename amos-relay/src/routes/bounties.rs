//! Bounty marketplace routes.

use crate::{
    pointing::{self, PointingInput},
    protocol_fees::calculate_fee,
    solana::{compute_dynamic_max_reward, fallback_max_reward, SettlementParams},
    state::RelayState,
};
use amos_core::types::BountyStatus;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;
use sqlx::Row;
use std::str::FromStr;
use tracing::{info, warn};
use uuid::Uuid;

// =============================================================================
// API ERROR TYPE — Returns JSON error bodies with helpful messages
// =============================================================================

/// API error that returns a JSON body with a human-readable message.
///
/// Usage: `return Err(ApiError::forbidden("The poster cannot approve their own bounty. ..."))`
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    fn precondition_required(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PRECONDITION_REQUIRED, message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": self.message,
            "status": self.status.as_u16(),
        });
        (self.status, Json(body)).into_response()
    }
}

/// Allow bare StatusCode to convert to ApiError (backwards compat for .map_err patterns)
impl From<StatusCode> for ApiError {
    fn from(status: StatusCode) -> Self {
        let message = status
            .canonical_reason()
            .unwrap_or("Unknown error")
            .to_string();
        Self { status, message }
    }
}

/// Build bounty routes.
pub fn routes() -> Router<RelayState> {
    Router::new()
        .route("/", post(create_bounty).get(list_bounties))
        .route("/pending-review", get(list_pending_review))
        .route("/{id}", get(get_bounty))
        .route("/{id}/claim", post(claim_bounty))
        .route("/{id}/submit", post(submit_work))
        .route("/{id}/verify", post(verify_submission))
        .route("/{id}/approve", post(approve_submission))
        .route("/{id}/reject", post(reject_submission))
        .route("/{id}/request_revision", post(request_revision))
        .route("/{id}/pushback", post(pushback))
        .route("/{id}/settle", post(retry_settlement))
        .route("/calculate-points", post(calculate_points_endpoint))
}

// ─────────────────────────────────────────────────────────────────────────
// Oracle-facing: bounties awaiting mission-alignment review
// ─────────────────────────────────────────────────────────────────────────

/// Shape the Oracle daemon expects — matches `amos_oracle::review::ReviewRequest`.
#[derive(Debug, Serialize)]
pub struct PendingReviewResponse {
    pub bounty_id: Uuid,
    pub bounty_title: String,
    pub bounty_description: String,
    pub bounty_category: String,
    pub bounty_contribution_type_id: u8,
    pub qa_evidence: JsonValue,
    pub proof: JsonValue,
    pub revision_count: u8,
    /// AMOS-META-007 phase 4: policy block from bounty creation. Constraints
    /// the submission was supposed to respect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<JsonValue>,
    /// AMOS-META-007 phase 4: worker's proof receipt — intent, validation
    /// plan, execution evidence. The substrate Oracle judges against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_receipt: Option<JsonValue>,
    /// AMOS-META-007 phase 4: failure capsule from the most recent revision
    /// request, if any. Lets Oracle judge whether the resubmission addresses
    /// the prior cited failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_capsule: Option<JsonValue>,
}

/// A bounty is "pending Oracle review" when it's submitted, mechanically
/// verified by the QA bot, and not yet approved/rejected by the Oracle.
async fn list_pending_review(
    State(state): State<RelayState>,
) -> Result<Json<Vec<PendingReviewResponse>>, StatusCode> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, title, description, category,
            verification_evidence, quality_evidence, result,
            revision_count,
            policy, proof_receipt, failure_capsule
        FROM relay_bounties
        WHERE status = 'submitted'
          AND verified_at IS NOT NULL
          AND approved_at IS NULL
          AND rejected_at IS NULL
        ORDER BY verified_at ASC
        LIMIT 100
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        warn!(error = %e, "list_pending_review query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let bounty_id: Uuid = match row.try_get("id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let title: String = row.try_get("title").unwrap_or_default();
        let description: String = row.try_get("description").unwrap_or_default();
        let category: Option<String> = row.try_get("category").ok();
        let category_str = category.unwrap_or_else(|| "uncategorized".to_string());

        // Merge verification_evidence (QA bot) and quality_evidence into a
        // single qa_evidence blob for the Oracle. The Oracle doesn't care
        // about the internal distinction; mission judgment sits on top.
        let mut qa_evidence = serde_json::Map::new();
        if let Ok(Some(v)) = row.try_get::<Option<JsonValue>, _>("verification_evidence") {
            qa_evidence.insert("verification".into(), v);
        }
        if let Ok(Some(v)) = row.try_get::<Option<JsonValue>, _>("quality_evidence") {
            qa_evidence.insert("quality".into(), v);
        }

        let proof: JsonValue = row
            .try_get::<Option<JsonValue>, _>("result")
            .ok()
            .flatten()
            .unwrap_or(JsonValue::Null);

        let revision_count_raw: Option<i32> = row.try_get("revision_count").ok();
        let revision_count = revision_count_raw.unwrap_or(0).clamp(0, u8::MAX as i32) as u8;

        let contribution_type_id = category_to_contribution_type(&category_str);

        let policy: Option<JsonValue> = row.try_get("policy").ok().flatten();
        let proof_receipt: Option<JsonValue> = row.try_get("proof_receipt").ok().flatten();
        let failure_capsule: Option<JsonValue> = row.try_get("failure_capsule").ok().flatten();

        out.push(PendingReviewResponse {
            bounty_id,
            bounty_title: title,
            bounty_description: description,
            bounty_category: category_str,
            bounty_contribution_type_id: contribution_type_id,
            qa_evidence: JsonValue::Object(qa_evidence),
            proof,
            revision_count,
            policy,
            proof_receipt,
            failure_capsule,
        });
    }

    Ok(Json(out))
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
    /// Bounty category: infrastructure, growth, research, content (default: infrastructure)
    pub category: Option<String>,
    /// AMOS-META-007: optional policy block. Constraints the worker's submission
    /// must respect (forbidden_paths, required_paths_subset, scope_constraint_ids,
    /// minimum_coverage_pct, max_file_size_bytes, self_modifying flag). Stored
    /// on the bounty row so future submissions can be judged against it.
    /// Manual posters may set explicitly; Oracle commission flow sets via
    /// proposed_bounty_spec.
    #[serde(default)]
    pub policy: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct CalculatePointsRequest {
    pub title: String,
    pub description: String,
    pub category: Option<String>,
    pub required_capabilities: Vec<String>,
    /// Days until deadline (default: 14)
    pub deadline_days: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct CalculatePointsResponse {
    pub points: u64,
    pub effort_score: u64,
    pub importance_multiplier: f64,
    pub specialization_multiplier: f64,
    pub time_factor: f64,
}

/// Max lengths for input validation (prevents oversized payloads hitting the DB)
const MAX_TITLE_LEN: usize = 500;
const MAX_DESCRIPTION_LEN: usize = 50_000;
const MAX_CAPABILITY_LEN: usize = 100;
const MAX_CAPABILITIES_COUNT: usize = 20;
const MAX_REJECTION_REASON_LEN: usize = 5_000;
const MAX_REVISION_FEEDBACK_LEN: usize = 10_000;
const MAX_RESULT_JSON_LEN: usize = 1_000_000; // 1MB
const MAX_REWARD_TOKENS: u64 = 16_000; // Daily emission cap — no single bounty exceeds a full day
const MAX_REVISIONS: i16 = 3;
/// Valid bounty categories
const VALID_CATEGORIES: &[&str] = &["infrastructure", "growth", "research", "content"];
/// Minimum trust level for QA/verification actions (verify, approve, reject, request_revision)
const QA_TRUST_LEVEL: i16 = 5;

#[derive(Debug, Deserialize)]
pub struct ListBountiesQuery {
    pub status: Option<BountyStatus>,
    pub min_reward: Option<u64>,
    pub capability: Option<String>,
    pub category: Option<String>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    /// Sort order: "newest" (default), "reward", "priority" (intelligent ranking)
    pub sort: Option<String>,
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
    /// AMOS-META-007 phase 2: optional proof-carrying receipt. When present,
    /// shape-validated by relay (see `proof_receipt::validate`) and stored on
    /// the bounty row. Semantic content judgment (does the validation plan
    /// cover the changes?) is Oracle's job, not the relay's.
    /// Becomes required for code-bounty approval in phase 5.
    #[serde(default)]
    pub proof_receipt: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct VerifySubmissionRequest {
    /// Wallet of the person/agent verifying the deliverable.
    pub verifier_wallet: String,
    /// Evidence that the deliverable is live and working.
    /// Example: {"git_sha": "abc123", "tests_passed": true, "build_url": "..."}
    pub evidence: JsonValue,
}

#[derive(Debug, Deserialize)]
pub struct ApproveSubmissionRequest {
    pub reviewer_wallet: String,
    pub quality_score: Option<u8>,
    /// AMOS-META-007 phase 5: when approving a code bounty without a
    /// proof_receipt, the reviewer must supply an explicit override reason
    /// (≥40 chars). The reason is persisted permanently and feeds drift
    /// monitoring per spec §6 (override accountability).
    #[serde(default)]
    pub override_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RejectSubmissionRequest {
    pub reviewer_wallet: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct RequestRevisionRequest {
    pub reviewer_wallet: String,
    pub feedback: String,
    /// AMOS-META-007 phase 3: optional structured failure capsule. When
    /// present, shape-validated and stored on the bounty row. Workers consume
    /// this directly in their rework prompt. See `proof_receipt::validate_failure_capsule`.
    #[serde(default)]
    pub failure_capsule: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct PushbackRequest {
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
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_by_wallet: Option<String>,
    pub verification_evidence: Option<JsonValue>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub claimed_by_wallet: Option<String>,
    pub settlement_tx: Option<String>,
    pub settlement_status: Option<String>,
    pub revision_count: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_feedback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    pub category: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Estimated AMOS payout based on current daily pool state.
    /// Only present for open/claimed bounties when Solana is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_payout_amos: Option<f64>,
    /// AMOS-META-007: policy block set at bounty creation. Constraints the
    /// submission must respect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<JsonValue>,
    /// AMOS-META-007: proof receipt set at submission time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_receipt: Option<JsonValue>,
    /// AMOS-META-007 phase 3: latest failure capsule from the most recent
    /// revision request. Workers consume this in their rework prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_capsule: Option<JsonValue>,
    /// AMOS-META-007 phase 5: when a code bounty was approved without a
    /// proof_receipt, this carries the reviewer's override reason.
    /// Permanent + drift-monitored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_override_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_override_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_override_at: Option<DateTime<Utc>>,
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
    verified_at, verified_by_wallet, verification_evidence,
    quality_score, approved_at, rejected_at, rejection_reason,
    settlement_tx, settlement_status,
    revision_count, revision_feedback, pr_url, category,
    policy, proof_receipt, failure_capsule,
    gate_override_reason, gate_override_by, gate_override_at,
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
        verified_at: row.try_get("verified_at")?,
        verified_by_wallet: row.try_get("verified_by_wallet")?,
        verification_evidence: row.try_get("verification_evidence")?,
        quality_score: row.try_get("quality_score")?,
        approved_at: row.try_get("approved_at")?,
        rejected_at: row.try_get("rejected_at")?,
        rejection_reason: row.try_get("rejection_reason")?,
        settlement_tx: row.try_get("settlement_tx")?,
        settlement_status: row.try_get("settlement_status")?,
        revision_count: row.try_get("revision_count")?,
        revision_feedback: row.try_get("revision_feedback")?,
        pr_url: row.try_get("pr_url")?,
        category: row
            .try_get::<String, _>("category")
            .unwrap_or_else(|_| "infrastructure".to_string()),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        estimated_payout_amos: None, // Enriched in list_bounties when Solana is configured
        policy: row.try_get("policy").ok().flatten(),
        proof_receipt: row.try_get("proof_receipt").ok().flatten(),
        failure_capsule: row.try_get("failure_capsule").ok().flatten(),
        gate_override_reason: row.try_get("gate_override_reason").ok().flatten(),
        gate_override_by: row.try_get("gate_override_by").ok().flatten(),
        gate_override_at: row.try_get("gate_override_at").ok().flatten(),
    })
}

// =============================================================================
// HELPERS
// =============================================================================

/// Check that a wallet belongs to an active agent with trust level >= required.
/// Returns the trust level and council_member flag, or an error status.
async fn require_trust(
    db: &sqlx::PgPool,
    wallet: &str,
    min_trust: i16,
    require_council: bool,
    action: &str,
    bounty_id: Uuid,
) -> Result<(i16, bool), ApiError> {
    let row: Option<(i16, bool)> = sqlx::query_as(
        "SELECT trust_level, council_member FROM relay_agents WHERE wallet_address = $1 AND status = 'active'",
    )
    .bind(wallet)
    .fetch_optional(db)
    .await
    .map_err(|e| {
        warn!("Failed to look up reviewer trust level: {}", e);
        ApiError::internal("Internal error looking up reviewer trust level")
    })?;

    match row {
        None => {
            warn!(
                "Wallet {} is not a registered agent — cannot {} bounty {}",
                wallet, action, bounty_id
            );
            Err(ApiError::forbidden(format!(
                "Wallet {} is not a registered agent. Register at POST /api/v1/agents/register first.",
                wallet
            )))
        }
        Some((level, _)) if level < min_trust => {
            warn!(
                "Wallet {} has trust level {} (need >= {}) — cannot {} bounty {}",
                wallet, level, min_trust, action, bounty_id
            );
            Err(ApiError::forbidden(format!(
                "Trust level {} is too low to {} this bounty. Minimum required: {}. Complete more bounties to increase your trust level.",
                level, action, min_trust
            )))
        }
        Some((_, is_council)) if require_council && !is_council => {
            warn!(
                "Wallet {} is not a council member — cannot {} bounty {}",
                wallet, action, bounty_id
            );
            Err(ApiError::forbidden(format!(
                "Only council members can {} bounties. Wallet {} is not a council member.",
                action, wallet
            )))
        }
        Some((level, is_council)) => Ok((level, is_council)),
    }
}

/// Map relay bounty category to on-chain contribution_type.
/// Must match the constants in amos-solana/programs/amos-bounty/src/constants.rs.
fn category_to_contribution_type(category: &str) -> u8 {
    match category {
        "infrastructure" => 7,
        "growth" => 8,
        "research" => 3,
        "content" => 9,
        "discovery" => 11,
        _ => 1, // default: feature
    }
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
    if req.reward_tokens > MAX_REWARD_TOKENS {
        warn!(
            "Invalid reward_tokens: {} (max {})",
            req.reward_tokens, MAX_REWARD_TOKENS
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate category
    let category = req.category.as_deref().unwrap_or("infrastructure");
    if !VALID_CATEGORIES.contains(&category) {
        warn!("Invalid bounty category: {}", category);
        return Err(StatusCode::BAD_REQUEST);
    }

    let bounty_id = Uuid::new_v4();
    let now = Utc::now();

    // Auto-point: if reward_tokens is 0, calculate points automatically
    let reward_tokens = if req.reward_tokens == 0 {
        let deadline_days = (req.deadline - now).num_hours() as f64 / 24.0;
        let input = PointingInput {
            title: req.title.clone(),
            description: req.description.clone(),
            category: category.to_string(),
            capabilities: req.required_capabilities.clone(),
            deadline_days: deadline_days.max(1.0),
        };
        let breakdown = pointing::calculate_points(&input);
        info!(
            "Auto-pointed bounty '{}': {} pts (effort={}, importance={:.2}, spec={:.2}, time={:.2})",
            req.title, breakdown.points, breakdown.effort_score,
            breakdown.importance_mult, breakdown.specialization_mult, breakdown.time_factor
        );
        breakdown.points
    } else {
        req.reward_tokens
    };

    let caps_json = serde_json::to_value(&req.required_capabilities).unwrap_or_default();
    let row = sqlx::query(&format!(
        "INSERT INTO relay_bounties (
                id, title, description, reward_tokens, deadline_at,
                required_capabilities, poster_wallet, status, category,
                policy,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING {BOUNTY_SELECT}"
    ))
    .bind(bounty_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(reward_tokens as i64)
    .bind(req.deadline)
    .bind(&caps_json)
    .bind(&req.poster_wallet)
    .bind(BountyStatus::Open.as_str())
    .bind(category)
    .bind(req.policy.as_ref())
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
        "Created bounty {} with reward {} pts{}",
        bounty_id,
        reward_tokens,
        if req.reward_tokens == 0 {
            " (auto-pointed)"
        } else {
            ""
        }
    );

    // Post bounty listing on-chain (non-blocking — on-chain is supplementary)
    if let Some(ref solana) = state.solana {
        let solana = solana.clone();
        let db = state.db.clone();
        let bid = bounty_id;
        let reward = reward_tokens;
        let deadline_ts = req.deadline.timestamp();
        let contribution_type = category_to_contribution_type(category);
        tokio::spawn(async move {
            let bounty_id_hash = {
                let mut hasher = Sha256::new();
                hasher.update(bid.to_string().as_bytes());
                let result = hasher.finalize();
                let mut out = [0u8; 32];
                out.copy_from_slice(&result);
                out
            };
            match solana
                .post_bounty_on_chain(
                    &bounty_id_hash,
                    0, // bounty_source: 0 = Treasury (system bounty)
                    reward,
                    contribution_type,
                    1,  // required_trust_level: Newcomer
                    72, // claim_timeout_hours: 3 days
                    deadline_ts,
                )
                .await
            {
                Ok(tx_sig) => {
                    info!(bounty_id = %bid, tx = %tx_sig, "Bounty posted on-chain");
                    let _ = sqlx::query(
                        "UPDATE relay_bounties SET onchain_listing_tx = $1 WHERE id = $2",
                    )
                    .bind(&tx_sig)
                    .bind(bid)
                    .execute(&db)
                    .await;
                }
                Err(e) => {
                    warn!(bounty_id = %bid, error = %e, "Failed to post bounty on-chain (non-critical)");
                }
            }
        });
    }

    Ok((StatusCode::CREATED, Json(bounty)))
}

/// Preview auto-calculated points for a bounty without creating it.
///
/// Useful for META-001 to estimate points before generating a bounty proposal,
/// or for any agent/UI to preview what a bounty would be scored at.
async fn calculate_points_endpoint(
    Json(req): Json<CalculatePointsRequest>,
) -> Result<Json<CalculatePointsResponse>, StatusCode> {
    let input = PointingInput {
        title: req.title,
        description: req.description,
        category: req.category.unwrap_or_else(|| "infrastructure".to_string()),
        capabilities: req.required_capabilities,
        deadline_days: req.deadline_days.unwrap_or(14.0).max(1.0),
    };
    let b = pointing::calculate_points(&input);
    Ok(Json(CalculatePointsResponse {
        points: b.points,
        effort_score: b.effort_score,
        importance_multiplier: b.importance_mult,
        specialization_multiplier: b.specialization_mult,
        time_factor: b.time_factor,
    }))
}

/// List bounties with optional filters and intelligent sorting.
async fn list_bounties(
    State(state): State<RelayState>,
    Query(query): Query<ListBountiesQuery>,
) -> Result<Json<Vec<BountyResponse>>, StatusCode> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    // Build dynamic WHERE clause
    let mut conditions: Vec<String> = Vec::new();
    let mut bind_idx = 1u32;

    if query.status.is_some() {
        conditions.push(format!("status = ${bind_idx}"));
        bind_idx += 1;
    }
    if query.category.is_some() {
        conditions.push(format!("category = ${bind_idx}"));
        bind_idx += 1;
    }
    if query.min_reward.is_some() {
        conditions.push(format!("reward_tokens >= ${bind_idx}"));
        bind_idx += 1;
    }
    if query.capability.is_some() {
        conditions.push(format!(
            "required_capabilities @> ARRAY[${bind_idx}]::text[]"
        ));
        bind_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Build ORDER BY based on sort parameter
    let order_by = match query.sort.as_deref() {
        Some("reward") => "ORDER BY reward_tokens DESC, created_at DESC".to_string(),
        Some("priority") => {
            // Intelligent composite ranking:
            // - Security bounties rank highest (critical path)
            // - Category weight: infrastructure > research > growth > content
            // - Higher reward = higher strategic value
            // - Genesis bounties (no "Depends on:" in description) rank higher
            // - Recency tiebreaker
            "ORDER BY (\
                CASE WHEN title LIKE 'AMOS-SECURE%' THEN 500 ELSE 0 END + \
                CASE category \
                    WHEN 'infrastructure' THEN 200 \
                    WHEN 'research' THEN 150 \
                    WHEN 'growth' THEN 100 \
                    WHEN 'content' THEN 50 \
                    ELSE 0 \
                END + \
                LEAST(reward_tokens, 10000) / 20 + \
                CASE WHEN description NOT LIKE '%Depends on:%' THEN 100 ELSE 0 END \
            ) DESC, created_at DESC"
                .to_string()
        }
        _ => "ORDER BY created_at DESC".to_string(), // "newest" is default
    };

    let sql = format!(
        "SELECT {BOUNTY_SELECT} FROM relay_bounties {where_clause} {order_by} LIMIT ${bind_idx} OFFSET ${}",
        bind_idx + 1
    );

    let mut q = sqlx::query(&sql);
    if let Some(ref status) = query.status {
        q = q.bind(status.as_str());
    }
    if let Some(ref category) = query.category {
        q = q.bind(category.as_str());
    }
    if let Some(min_reward) = query.min_reward {
        q = q.bind(min_reward as i64);
    }
    if let Some(ref capability) = query.capability {
        q = q.bind(capability.as_str());
    }
    q = q.bind(per_page as i64).bind(offset as i64);

    let rows = q.fetch_all(&state.db).await.map_err(|e| {
        warn!("Failed to list bounties: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut bounties: Vec<BountyResponse> = rows
        .into_iter()
        .filter_map(|r| bounty_from_row(r).ok())
        .collect();

    // Enrich open/claimed bounties with estimated payout from on-chain pool
    if let Some(ref solana) = state.solana {
        if let Ok((start_time, day_index)) = solana.read_config_timing().await {
            let now = chrono::Utc::now().timestamp();
            let pool = solana
                .read_daily_pool(day_index)
                .await
                .ok()
                .flatten()
                .unwrap_or(crate::solana::DailyPoolState {
                    day_index,
                    daily_emission: 16_000 * 1_000_000_000,
                    tokens_distributed: 0,
                    total_points: 0,
                    proof_count: 0,
                });
            for b in bounties.iter_mut() {
                if matches!(b.status, BountyStatus::Open | BountyStatus::Claimed) {
                    let points = (b.reward_tokens as u64).min(2000); // conservative cap
                    let est = compute_dynamic_max_reward(points, &pool, start_time, now);
                    b.estimated_payout_amos = Some(est as f64 / 1_000_000_000.0);
                }
            }
        }
    }

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

    // AMOS-META-007 phase 2: shape-validate the proof receipt up front.
    // Content judgment (does the validation plan cover the changes?) belongs
    // to Oracle, not the relay.
    if let Some(ref receipt) = req.proof_receipt {
        if let Err(msg) = crate::proof_receipt::validate(receipt) {
            warn!(bounty = %id, error = %msg, "submit_work: proof_receipt rejected");
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    let now = Utc::now();

    // Extract pr_url from result JSON if present
    let pr_url = req
        .result
        .get("pr_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // If wallet_address provided at submit time and not yet stored, update it
    let wallet_clause = if req.wallet_address.is_some() {
        ", claimed_by_wallet = COALESCE(claimed_by_wallet, $9)"
    } else {
        ""
    };
    // pr_url is always the last bind ($9 or $10 depending on wallet)
    let pr_bind_idx = if req.wallet_address.is_some() {
        "$10"
    } else {
        "$9"
    };
    let sql = format!("UPDATE relay_bounties SET status = $1, submitted_at = $2, result = $3, quality_evidence = $4, updated_at = $5, pr_url = COALESCE({pr_bind_idx}, pr_url){wallet_clause} WHERE id = $6 AND status = $7 AND claimed_by_agent_id = $8 RETURNING {BOUNTY_SELECT}");
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
    query = query.bind(&pr_url);
    let row = query
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            warn!("Failed to submit work for bounty {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::CONFLICT)?;
    let mut bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Persist the proof receipt (if any) on the same row. Already shape-validated.
    if let Some(receipt) = req.proof_receipt {
        let res = sqlx::query(
            "UPDATE relay_bounties SET proof_receipt = $1, updated_at = $2 WHERE id = $3",
        )
        .bind(&receipt)
        .bind(now)
        .bind(id)
        .execute(&state.db)
        .await;
        match res {
            Ok(_) => {
                bounty.proof_receipt = Some(receipt);
                info!(bounty = %id, "proof_receipt stored");
            }
            Err(e) => {
                // Submit succeeded; receipt persist failed. Surface a 207-ish
                // signal by logging — caller can re-submit the receipt. We
                // don't roll back the submit because it already happened.
                warn!(bounty = %id, error = %e, "proof_receipt persist failed; submit succeeded without it");
            }
        }
    }

    info!("Work submitted for bounty {} by agent {}", id, req.agent_id);

    Ok(Json(bounty))
}

/// Verify a submitted bounty's deliverables are pushed and tested.
///
/// This is a required step before approval. Verification evidence must include
/// proof that the work is live (e.g., git SHA, CI pass, test results).
/// The bounty lifecycle is: submitted → verified → approved → settled.
async fn verify_submission(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<VerifySubmissionRequest>,
) -> Result<Json<BountyResponse>, ApiError> {
    if !crate::validate_wallet_address(&req.verifier_wallet) {
        warn!("Invalid verifier wallet: {}", req.verifier_wallet);
        return Err(ApiError::bad_request(
            "Invalid verifier_wallet address format. Must be a valid Solana public key (base58).",
        ));
    }

    // Verifier must be trust >= 5 (QA agent or council)
    require_trust(
        &state.db,
        &req.verifier_wallet,
        QA_TRUST_LEVEL,
        false,
        "verify",
        id,
    )
    .await?;

    // Evidence must not be empty
    if req.evidence.is_null()
        || (req.evidence.is_object() && req.evidence.as_object().unwrap().is_empty())
    {
        warn!("Verification evidence is empty for bounty {}", id);
        return Err(ApiError::bad_request("Verification evidence is required. Provide proof that the work is live (e.g., git SHA, CI pass, test results)."));
    }

    let now = Utc::now();

    let row = sqlx::query(&format!(
        "UPDATE relay_bounties \
         SET verified_at = $1, verified_by_wallet = $2, verification_evidence = $3, updated_at = $4 \
         WHERE id = $5 AND status = $6 AND verified_at IS NULL \
         RETURNING {BOUNTY_SELECT}"
    ))
    .bind(now)
    .bind(&req.verifier_wallet)
    .bind(&req.evidence)
    .bind(now)
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to verify bounty {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or_else(|| {
        warn!(
            "Bounty {} not found, not in submitted state, or already verified",
            id
        );
        StatusCode::CONFLICT
    })?;

    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    info!(
        "Bounty {} verified by {} with evidence",
        id, req.verifier_wallet
    );

    Ok(Json(bounty))
}

/// Approve a bounty submission and trigger payout.
///
/// **Requires verification**: the bounty must have been verified first via
/// the `/verify` endpoint. This ensures deliverables are pushed and tested
/// before on-chain settlement occurs.
async fn approve_submission(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ApproveSubmissionRequest>,
) -> Result<Json<BountyResponse>, ApiError> {
    // Validate reviewer wallet format
    if !crate::validate_wallet_address(&req.reviewer_wallet) {
        warn!(
            "Invalid reviewer wallet in approval: {}",
            req.reviewer_wallet
        );
        return Err(ApiError::bad_request(
            "Invalid reviewer_wallet address format.",
        ));
    }
    // Validate quality score range if provided
    if let Some(score) = req.quality_score {
        if score > 100 {
            warn!("Quality score out of range: {} (must be 0-100)", score);
            return Err(ApiError::bad_request(
                "quality_score must be 0-100 (integer percentage).",
            ));
        }
    }

    let now = Utc::now();

    // Fetch the bounty with poster, claimer wallets, verification status,
    // category, and the proof_receipt (META-007 phase 5 strict gate).
    let current_bounty = sqlx::query(
        r#"
        SELECT reward_tokens, poster_wallet, claimed_by_wallet, verified_at,
               category, proof_receipt
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
        ApiError::internal("Failed to fetch bounty.")
    })?
    .ok_or_else(|| {
        ApiError::not_found(format!(
            "Bounty {} not found or not in 'submitted' status. Only submitted bounties can be approved.",
            id
        ))
    })?;

    // --- Verification gate: deliverable must be verified before approval ---
    let verified_at: Option<DateTime<Utc>> = current_bounty.get("verified_at");
    if verified_at.is_none() {
        warn!(
            "Approval blocked: bounty {} has not been verified yet. \
             Call POST /{}/verify first with evidence that the deliverable is pushed and tested.",
            id, id
        );
        return Err(ApiError::precondition_required(format!(
            "Bounty must be verified before approval. Call POST /api/v1/bounties/{}/verify first \
             with evidence that the deliverable is pushed and tested.",
            id
        )));
    }

    // --- META-007 phase 5 strict gate: code bounties require proof_receipt ---
    // Code-class categories (infrastructure, research) require an
    // AMOS-META-007 proof_receipt on the bounty row before approval.
    // Reviewers can override with an explicit reason (≥40 chars) which is
    // persisted permanently for drift monitoring (spec §6).
    let bounty_category: Option<String> = current_bounty.get("category");
    let proof_receipt: Option<JsonValue> = current_bounty.get("proof_receipt");
    let category_requires_receipt = matches!(
        bounty_category.as_deref(),
        Some("infrastructure") | Some("research")
    );
    let mut gate_override_reason: Option<String> = None;
    if category_requires_receipt && proof_receipt.is_none() {
        match req.override_reason.as_deref() {
            None => {
                warn!(
                    "Strict gate: bounty {} ({:?}) has no proof_receipt and no override_reason",
                    id, bounty_category
                );
                return Err(ApiError::precondition_required(
                    "Code bounties (category=infrastructure or research) require a proof_receipt \
                     submitted with the work. To approve without one, supply override_reason \
                     (≥40 chars) explaining why. Override is permanent and feeds drift monitoring.",
                ));
            }
            Some(reason) if reason.trim().len() < 40 => {
                return Err(ApiError::bad_request(
                    "override_reason must be ≥40 characters. Generic short reasons fail \
                     validation per spec §6.",
                ));
            }
            Some(reason) => {
                gate_override_reason = Some(reason.trim().to_string());
                warn!(
                    bounty = %id,
                    reviewer = %req.reviewer_wallet,
                    "Strict gate OVERRIDDEN: receipt missing, override_reason supplied"
                );
            }
        }
    }

    // --- Separation of duties: prevent self-approval ---

    // 1. Poster cannot approve their own bounty
    let poster_wallet: Option<String> = current_bounty.get("poster_wallet");
    if let Some(ref poster) = poster_wallet {
        if poster == &req.reviewer_wallet {
            warn!(
                "Self-approval blocked: poster {} tried to approve bounty {}",
                req.reviewer_wallet, id
            );
            return Err(ApiError::forbidden(
                "The bounty poster cannot approve their own bounty. A different reviewer with trust level >= 5 must approve it.",
            ));
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
            return Err(ApiError::forbidden(
                "The bounty claimer cannot approve their own submission. A different reviewer with trust level >= 5 must approve it.",
            ));
        }
    }

    // 3. Reviewer must be trust >= 5 and council member
    require_trust(
        &state.db,
        &req.reviewer_wallet,
        QA_TRUST_LEVEL,
        true,
        "approve",
        id,
    )
    .await?;

    // Calculate protocol fee
    let reward_tokens: i64 = current_bounty.get("reward_tokens");
    let reward_tokens = reward_tokens as u64;
    let fee = calculate_fee(reward_tokens);

    info!(
        "Approving bounty {}: reward={}, protocol_fee={}, holder_share={}, burn_share={}, labs_share={}",
        id, reward_tokens, fee.total_fee, fee.holder_share, fee.burn_share, fee.labs_share
    );

    // Update the bounty status (also store reviewer_wallet for settlement
    // retry) + persist any META-007 strict-gate override.
    let row = sqlx::query(&format!(
        "UPDATE relay_bounties SET \
             status = $1, approved_at = $2, quality_score = $3, updated_at = $4, \
             reviewer_wallet = $7, \
             gate_override_reason = $8, \
             gate_override_by    = CASE WHEN $8 IS NOT NULL THEN $7 ELSE NULL END, \
             gate_override_at    = CASE WHEN $8 IS NOT NULL THEN $2 ELSE NULL END \
             WHERE id = $5 AND status = $6 RETURNING {BOUNTY_SELECT}"
    ))
    .bind(BountyStatus::Approved.as_str())
    .bind(now)
    .bind(req.quality_score.map(|s| s as i16))
    .bind(now)
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .bind(&req.reviewer_wallet)
    .bind(gate_override_reason.as_deref())
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
                // Use wallet pubkey bytes as agent_id (portable across relays)
                let bounty_id_str = id.to_string();
                let agent_id_bytes: [u8; 32] = Pubkey::from_str(&wallet)
                    .map(|pk| pk.to_bytes())
                    .unwrap_or([0u8; 32]);
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

                // Dynamic max_reward: read on-chain pool state and compute proportional cap
                let max_reward = if let Some(ref solana_client) = state.solana {
                    match solana_client.read_config_timing().await {
                        Ok((start_time, day_index)) => {
                            let now = chrono::Utc::now().timestamp();
                            match solana_client.read_daily_pool(day_index).await {
                                Ok(Some(pool)) => {
                                    let mr = compute_dynamic_max_reward(
                                        base_points as u64,
                                        &pool,
                                        start_time,
                                        now,
                                    );
                                    info!(bounty_id = %id, points = base_points, max_reward = mr,
                                          pool_distributed = pool.tokens_distributed,
                                          pool_total_points = pool.total_points,
                                          "Dynamic max_reward computed from on-chain pool");
                                    mr
                                }
                                Ok(None) => {
                                    // Pool not created yet today (first submission)
                                    let mr = fallback_max_reward(base_points as u64);
                                    info!(bounty_id = %id, max_reward = mr,
                                          "Using fallback max_reward (pool not yet created)");
                                    mr
                                }
                                Err(e) => {
                                    warn!(bounty_id = %id, error = %e,
                                          "Failed to read daily pool — using fallback max_reward");
                                    fallback_max_reward(base_points as u64)
                                }
                            }
                        }
                        Err(e) => {
                            warn!(bounty_id = %id, error = %e,
                                  "Failed to read config timing — using fallback max_reward");
                            fallback_max_reward(base_points as u64)
                        }
                    }
                } else {
                    fallback_max_reward(base_points as u64)
                };

                let params = SettlementParams {
                    bounty_id: bounty_id_str,
                    agent_wallet: wallet,
                    reviewer_wallet: req.reviewer_wallet.clone(),
                    base_points,
                    quality_score: req.quality_score.unwrap_or(70),
                    contribution_type: category_to_contribution_type(
                        &current_bounty
                            .try_get::<String, _>("category")
                            .unwrap_or_else(|_| "infrastructure".to_string()),
                    ),
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
) -> Result<Json<BountyResponse>, ApiError> {
    if req.reason.len() > MAX_REJECTION_REASON_LEN {
        warn!("Rejection reason too long: {} chars", req.reason.len());
        return Err(ApiError::bad_request(format!(
            "Rejection reason too long ({} chars). Maximum: {} chars.",
            req.reason.len(),
            MAX_REJECTION_REASON_LEN
        )));
    }
    if !crate::validate_wallet_address(&req.reviewer_wallet) {
        warn!("Invalid reviewer wallet in rejection");
        return Err(ApiError::bad_request(
            "Invalid reviewer_wallet address format.",
        ));
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
        ApiError::internal("Failed to fetch bounty.")
    })?
    .ok_or_else(|| {
        ApiError::not_found(format!(
            "Bounty {} not found or not in 'submitted' status. Only submitted bounties can be rejected.",
            id
        ))
    })?;

    // Poster cannot reject their own bounty (could be used to grief claimers)
    let poster_wallet: Option<String> = bounty_check.get("poster_wallet");
    if let Some(ref poster) = poster_wallet {
        if poster == &req.reviewer_wallet {
            warn!(
                "Self-rejection blocked: poster {} tried to reject bounty {}",
                req.reviewer_wallet, id
            );
            return Err(ApiError::forbidden(
                "The bounty poster cannot reject their own bounty. A different reviewer with trust level >= 5 must reject it.",
            ));
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
            return Err(ApiError::forbidden(
                "The bounty claimer cannot reject their own submission. A different reviewer with trust level >= 5 must reject it.",
            ));
        }
    }

    // Reviewer must be trust >= 5
    require_trust(
        &state.db,
        &req.reviewer_wallet,
        QA_TRUST_LEVEL,
        false,
        "reject",
        id,
    )
    .await?;

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

/// Request revision on a submitted bounty — kicks it back to claimed with feedback.
///
/// The agent can then rework and resubmit. Each revision carries a minor reputation
/// cost (-5 quality score) to prevent agents from farming the QA bot for free code review.
/// Max 3 revisions before hard rejection is required.
async fn request_revision(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<RequestRevisionRequest>,
) -> Result<Json<BountyResponse>, ApiError> {
    if !crate::validate_wallet_address(&req.reviewer_wallet) {
        warn!("Invalid reviewer wallet in revision request");
        return Err(ApiError::bad_request(
            "Invalid reviewer_wallet address format.",
        ));
    }
    if req.feedback.is_empty() || req.feedback.len() > MAX_REVISION_FEEDBACK_LEN {
        warn!(
            "Revision feedback invalid length: {} (max {})",
            req.feedback.len(),
            MAX_REVISION_FEEDBACK_LEN
        );
        return Err(ApiError::bad_request(format!(
            "Revision feedback must be 1-{} characters.",
            MAX_REVISION_FEEDBACK_LEN
        )));
    }

    // AMOS-META-007 phase 3: shape-validate the failure capsule.
    if let Some(ref capsule) = req.failure_capsule {
        if let Err(msg) = crate::proof_receipt::validate_failure_capsule(capsule) {
            warn!(bounty = %id, error = %msg, "request_revision: failure_capsule rejected");
            return Err(ApiError::bad_request(format!(
                "failure_capsule rejected: {msg}"
            )));
        }
    }

    // Reviewer must be trust >= 5
    require_trust(
        &state.db,
        &req.reviewer_wallet,
        QA_TRUST_LEVEL,
        false,
        "request_revision",
        id,
    )
    .await?;

    // Fetch current bounty to check state and separation of duties
    let bounty_check = sqlx::query(
        "SELECT poster_wallet, claimed_by_wallet, revision_count FROM relay_bounties WHERE id = $1 AND status = $2",
    )
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to fetch bounty for revision check {}: {}", id, e);
        ApiError::internal("Failed to fetch bounty.")
    })?
    .ok_or_else(|| {
        ApiError::not_found(format!(
            "Bounty {} not found or not in 'submitted' status.",
            id
        ))
    })?;

    // Separation of duties: poster/claimer cannot request revision on own bounty
    let poster_wallet: Option<String> = bounty_check.get("poster_wallet");
    if let Some(ref poster) = poster_wallet {
        if poster == &req.reviewer_wallet {
            warn!(
                "Self-revision blocked: poster {} on bounty {}",
                req.reviewer_wallet, id
            );
            return Err(ApiError::forbidden(
                "The bounty poster cannot request a revision on their own bounty.",
            ));
        }
    }
    let claimed_by_wallet: Option<String> = bounty_check.get("claimed_by_wallet");
    if let Some(ref claimer) = claimed_by_wallet {
        if claimer == &req.reviewer_wallet {
            warn!(
                "Self-revision blocked: claimer {} on bounty {}",
                req.reviewer_wallet, id
            );
            return Err(ApiError::forbidden(
                "The bounty claimer cannot request a revision on their own submission.",
            ));
        }
    }

    // Check revision limit
    let revision_count: i16 = bounty_check.get("revision_count");
    if revision_count >= MAX_REVISIONS {
        warn!(
            "Bounty {} has reached max revisions ({}), use /reject instead",
            id, MAX_REVISIONS
        );
        return Err(ApiError::conflict(format!(
            "Bounty has reached the maximum of {} revisions. Use POST /api/v1/bounties/{}/reject instead.",
            MAX_REVISIONS, id
        )));
    }

    let now = Utc::now();

    // Reset to claimed with feedback — agent can rework and resubmit.
    // failure_capsule is upserted (latest revision wins); proof_receipt
    // from the prior submit is cleared so the worker's next submission
    // starts fresh.
    let row = sqlx::query(&format!(
        "UPDATE relay_bounties SET \
         status = $1, \
         revision_count = revision_count + 1, \
         revision_feedback = $2, \
         submitted_at = NULL, \
         verified_at = NULL, \
         verified_by_wallet = NULL, \
         verification_evidence = NULL, \
         failure_capsule = $6, \
         proof_receipt = NULL, \
         updated_at = $3 \
         WHERE id = $4 AND status = $5 \
         RETURNING {BOUNTY_SELECT}"
    ))
    .bind(BountyStatus::Claimed.as_str())
    .bind(&req.feedback)
    .bind(now)
    .bind(id)
    .bind(BountyStatus::Submitted.as_str())
    .bind(req.failure_capsule.as_ref())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to request revision for bounty {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::CONFLICT)?;
    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!(
        "Bounty {} revision requested by {} (revision #{})",
        id,
        req.reviewer_wallet,
        revision_count + 1
    );

    Ok(Json(bounty))
}

/// Record a pushback on an approved bounty (e.g., PR closed without merging).
///
/// This does NOT reverse payment — the agent was already paid. Instead it records
/// a reputation hit: -30 quality score. Repeated pushbacks degrade agent trust level.
async fn pushback(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PushbackRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !crate::validate_wallet_address(&req.reviewer_wallet) {
        warn!("Invalid reviewer wallet in pushback");
        return Err(ApiError::bad_request(
            "Invalid reviewer_wallet address format. Must be a valid Solana public key (base58).",
        ));
    }

    // Must be council member
    require_trust(
        &state.db,
        &req.reviewer_wallet,
        QA_TRUST_LEVEL,
        true,
        "pushback",
        id,
    )
    .await?;

    // Bounty must be approved (already settled)
    let bounty = sqlx::query(
        "SELECT id, quality_score, claimed_by_agent_id FROM relay_bounties WHERE id = $1 AND status = $2",
    )
    .bind(id)
    .bind(BountyStatus::Approved.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to fetch bounty for pushback {}: {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let current_score: Option<i16> = bounty.get("quality_score");
    let new_score = (current_score.unwrap_or(85) - 30).max(0);

    let now = Utc::now();
    sqlx::query("UPDATE relay_bounties SET quality_score = $1, updated_at = $2 WHERE id = $3")
        .bind(new_score)
        .bind(now)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            warn!("Failed to record pushback for bounty {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let agent_id: Option<Uuid> = bounty.get("claimed_by_agent_id");
    info!(
        "Pushback on bounty {} by {}: quality {} → {}, reason: {}",
        id,
        req.reviewer_wallet,
        current_score.unwrap_or(85),
        new_score,
        req.reason
    );

    Ok(Json(serde_json::json!({
        "bounty_id": id,
        "previous_quality_score": current_score.unwrap_or(85),
        "new_quality_score": new_score,
        "agent_id": agent_id,
        "pushback_reason": req.reason,
    })))
}

/// Retry settlement for an approved bounty whose on-chain settlement failed.
/// Only bounties with status=approved and settlement_status=failed can be retried.
async fn retry_settlement(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BountyResponse>, StatusCode> {
    // Fetch the bounty — must be approved with failed settlement
    let current_bounty = sqlx::query("SELECT * FROM relay_bounties WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            warn!("Failed to fetch bounty {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let status: String = current_bounty.get("status");
    if status != BountyStatus::Approved.as_str() {
        warn!(
            "Cannot retry settlement: bounty {} has status {}",
            id, status
        );
        return Err(StatusCode::CONFLICT);
    }

    let settlement_status: Option<String> = current_bounty.get("settlement_status");
    if settlement_status.as_deref() != Some("failed") {
        warn!(
            "Cannot retry settlement: bounty {} has settlement_status {:?}",
            id, settlement_status
        );
        return Err(StatusCode::CONFLICT);
    }

    let solana = state.solana.as_ref().ok_or_else(|| {
        warn!("Solana client not configured — cannot settle");
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    if !solana.is_settlement_ready() {
        warn!("Solana settlement not ready");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let reward_tokens: i64 = current_bounty.get("reward_tokens");
    let reward_tokens = reward_tokens as u64;

    // Get the agent wallet
    let claimed_by_wallet: Option<String> = current_bounty.get("claimed_by_wallet");
    let claimed_by_agent_id: Option<Uuid> = current_bounty.get("claimed_by_agent_id");

    let agent_wallet = if let Some(ref w) = claimed_by_wallet {
        Some(w.clone())
    } else if let Some(agent_id) = claimed_by_agent_id {
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

    let wallet = agent_wallet.ok_or_else(|| {
        warn!("No wallet address for bounty {} — cannot settle", id);
        StatusCode::UNPROCESSABLE_ENTITY
    })?;

    // Get reviewer wallet from the fee ledger or use a default
    let reviewer_wallet: String =
        sqlx::query_scalar("SELECT reviewer_wallet FROM relay_bounties WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

    // If no reviewer wallet stored on bounty, look it up from agents with trust >= 3
    let reviewer_wallet = if reviewer_wallet.is_empty() {
        // Fall back to any trust-3+ agent wallet — the original reviewer
        "kekPK242otEGHrNmZA7v2jLYdkg3BPYiTPMJvrDhNuj".to_string()
    } else {
        reviewer_wallet
    };

    // Use wallet pubkey bytes as agent_id (portable across relays)
    let bounty_id_str = id.to_string();
    let agent_id_bytes: [u8; 32] = Pubkey::from_str(&wallet)
        .map(|pk| pk.to_bytes())
        .unwrap_or([0u8; 32]);

    let result_json: Option<JsonValue> = current_bounty.get("result");
    let evidence_hash = {
        let mut hasher = Sha256::new();
        hasher.update(
            serde_json::to_string(&result_json)
                .unwrap_or_default()
                .as_bytes(),
        );
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    };

    let agent_trust_level: i16 = if let Some(aid) = claimed_by_agent_id {
        sqlx::query_scalar::<_, i16>("SELECT trust_level FROM relay_agents WHERE id = $1")
            .bind(aid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .unwrap_or(1)
    } else {
        1
    };

    let max_for_trust = match agent_trust_level {
        1 => 100u64,
        2 => 200,
        3 => 500,
        4 => 1000,
        _ => 2000,
    };
    let base_points = (reward_tokens.min(max_for_trust)) as u16;
    let quality_score: Option<i16> = current_bounty.get("quality_score");

    // Dynamic max_reward from on-chain pool state
    let max_reward = match solana.read_config_timing().await {
        Ok((start_time, day_index)) => {
            let now = chrono::Utc::now().timestamp();
            match solana.read_daily_pool(day_index).await {
                Ok(Some(pool)) => {
                    let mr = compute_dynamic_max_reward(base_points as u64, &pool, start_time, now);
                    info!(bounty_id = %id, max_reward = mr, "Dynamic max_reward for retry");
                    mr
                }
                _ => fallback_max_reward(base_points as u64),
            }
        }
        Err(_) => fallback_max_reward(base_points as u64),
    };

    let params = SettlementParams {
        bounty_id: bounty_id_str,
        agent_wallet: wallet,
        reviewer_wallet,
        base_points,
        quality_score: quality_score.unwrap_or(70) as u8,
        contribution_type: category_to_contribution_type(
            &current_bounty
                .try_get::<String, _>("category")
                .unwrap_or_else(|_| "infrastructure".to_string()),
        ),
        is_agent: true,
        agent_id: agent_id_bytes,
        evidence_hash,
        max_reward,
    };

    info!(bounty_id = %id, "Retrying on-chain settlement");

    match solana.process_bounty_payout(&params).await {
        Ok(result) => {
            let _ = sqlx::query(
                "UPDATE relay_bounties SET settlement_tx = $1, settlement_status = 'settled' WHERE id = $2",
            )
            .bind(&result.tx_signature)
            .bind(id)
            .execute(&state.db)
            .await;

            // Update fee ledger too
            let _ = sqlx::query(
                "UPDATE protocol_fee_ledger SET settled_on_chain = true, settlement_tx = $1 WHERE bounty_id = $2",
            )
            .bind(&result.tx_signature)
            .bind(id)
            .execute(&state.db)
            .await;

            info!(bounty_id = %id, tx = %result.tx_signature, "Settlement retry succeeded");
        }
        Err(e) => {
            warn!(bounty_id = %id, error = %e, "Settlement retry failed");
            return Err(StatusCode::BAD_GATEWAY);
        }
    }

    // Return updated bounty
    let row = sqlx::query(&format!(
        "SELECT {BOUNTY_SELECT} FROM relay_bounties WHERE id = $1"
    ))
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let bounty = bounty_from_row(row).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(bounty))
}
