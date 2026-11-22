//! Intake submission queue — the Oracle's input stream.
//!
//! A submission is anything that might become a system bounty: a customer
//! request, a bug report, an agent-proposed bounty. The Oracle daemon polls
//! `GET /api/v1/intakes?status=pending`, evaluates each, and posts the
//! verdict. This route owns the CRUD surface around the queue.
//!
//! All intake routes require a credential. Creating records binds the submitter
//! to that principal; recording an evaluation also requires its service scope.

use crate::identity::Principal;
use crate::state::RelayState;
use axum::Extension;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::Row;
use tracing::warn;
use uuid::Uuid;

pub fn routes() -> Router<RelayState> {
    Router::new()
        .route("/", post(create_intake).get(list_intakes))
        .route("/{id}", get(get_intake))
        .route("/{id}/evaluation", post(record_evaluation))
}

// ─── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateIntakeRequest {
    pub title: String,
    pub body: String,
    pub submitter: String,
    #[serde(default)]
    pub parent_submission_id: Option<Uuid>,
    #[serde(default)]
    pub suggested_category: Option<String>,
    #[serde(default)]
    pub suggested_capabilities: Vec<String>,
    /// Optional Solana wallet — when set, submitter is eligible for a
    /// finder's fee on commissioned + settled bounties derived from this
    /// intake. Authenticated reports without a payout wallet are accepted.
    #[serde(default)]
    pub submitter_wallet: Option<String>,
}

/// Oracle-daemon-facing shape. Matches `amos_oracle::intake::IntakeSubmission`.
#[derive(Debug, Serialize)]
pub struct IntakeResponse {
    pub submission_id: Uuid,
    pub title: String,
    pub body: String,
    pub submitter: String,
    pub parent_submission_id: Option<Uuid>,
    pub suggested_category: Option<String>,
    pub suggested_capabilities: Vec<String>,
    pub status: String,
    pub verdict: Option<String>,
    pub decision_id: Option<Uuid>,
    pub commissioned_bounty_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub evaluated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitter_wallet: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListIntakesQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Oracle calls this after making a decision to close out an intake.
/// `decision_id` links back to oracle_decisions. If `verdict=commission`,
/// `commissioned_bounty_id` is set after the daemon creates the bounty.
#[derive(Debug, Deserialize)]
pub struct RecordEvaluationRequest {
    pub verdict: String,
    pub decision_id: Uuid,
    #[serde(default)]
    pub commissioned_bounty_id: Option<Uuid>,
}

// ─── Handlers ────────────────────────────────────────────────────────────

async fn create_intake(
    State(state): State<RelayState>,
    Extension(principal): Extension<Principal>,
    Json(mut req): Json<CreateIntakeRequest>,
) -> Result<(StatusCode, Json<IntakeResponse>), StatusCode> {
    req.submitter = principal.audit_id();
    if let Some(wallet) = &req.submitter_wallet {
        principal.require_wallet(wallet)?;
    }
    if req.title.trim().is_empty() || req.title.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.body.trim().is_empty() || req.body.len() > 50_000 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.submitter.trim().is_empty() || req.submitter.len() > 128 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.suggested_capabilities.len() > 20 {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Validate an optional authenticated submitter payout wallet.
    if let Some(ref w) = req.submitter_wallet {
        if !crate::validate_wallet_address(w) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let submission_id = Uuid::new_v4();
    let caps =
        serde_json::to_value(&req.suggested_capabilities).unwrap_or(JsonValue::Array(vec![]));

    let row = sqlx::query(
        r#"
        INSERT INTO oracle_intakes (
            submission_id, title, body, submitter,
            parent_submission_id, suggested_category, suggested_capabilities,
            submitter_wallet
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING
            submission_id, title, body, submitter, parent_submission_id,
            suggested_category, suggested_capabilities, status, verdict,
            decision_id, commissioned_bounty_id, created_at, evaluated_at,
            submitter_wallet
        "#,
    )
    .bind(submission_id)
    .bind(&req.title)
    .bind(&req.body)
    .bind(&req.submitter)
    .bind(req.parent_submission_id)
    .bind(&req.suggested_category)
    .bind(&caps)
    .bind(&req.submitter_wallet)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        warn!(error = %e, "create_intake insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let resp = intake_from_row(row).map_err(|e| {
        warn!(error = %e, "create_intake row map failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(resp)))
}

async fn list_intakes(
    State(state): State<RelayState>,
    Query(q): Query<ListIntakesQuery>,
) -> Result<Json<Vec<IntakeResponse>>, StatusCode> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);

    let rows = match q.status.as_deref() {
        Some(s) if s == "pending" || s == "evaluated" => {
            sqlx::query(
                r#"
                SELECT
                    submission_id, title, body, submitter, parent_submission_id,
                    suggested_category, suggested_capabilities, status, verdict,
                    decision_id, commissioned_bounty_id, created_at, evaluated_at,
                    submitter_wallet
                FROM oracle_intakes
                WHERE status = $1
                ORDER BY created_at ASC
                LIMIT $2
                "#,
            )
            .bind(s)
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
        _ => {
            sqlx::query(
                r#"
                SELECT
                    submission_id, title, body, submitter, parent_submission_id,
                    suggested_category, suggested_capabilities, status, verdict,
                    decision_id, commissioned_bounty_id, created_at, evaluated_at,
                    submitter_wallet
                FROM oracle_intakes
                ORDER BY created_at DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
    }
    .map_err(|e| {
        warn!(error = %e, "list_intakes query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        match intake_from_row(row) {
            Ok(r) => out.push(r),
            Err(e) => {
                warn!(error = %e, "list_intakes: skipping malformed row");
            }
        }
    }
    Ok(Json(out))
}

async fn get_intake(
    State(state): State<RelayState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<Json<IntakeResponse>, StatusCode> {
    let row = sqlx::query(
        r#"
        SELECT
            submission_id, title, body, submitter, parent_submission_id,
            suggested_category, suggested_capabilities, status, verdict,
            decision_id, commissioned_bounty_id, created_at, evaluated_at
        FROM oracle_intakes
        WHERE submission_id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!(error = %e, "get_intake query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let resp = intake_from_row(row).map_err(|e| {
        warn!(error = %e, "get_intake row map failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(resp))
}

async fn record_evaluation(
    State(state): State<RelayState>,
    Extension(principal): Extension<Principal>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(req): Json<RecordEvaluationRequest>,
) -> Result<Json<IntakeResponse>, StatusCode> {
    principal.require_scope("intakes:evaluate")?;
    let valid_verdicts = ["commission", "reject", "refine", "escalate"];
    if !valid_verdicts.contains(&req.verdict.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let row = sqlx::query(
        r#"
        UPDATE oracle_intakes
        SET status = 'evaluated',
            verdict = $2,
            decision_id = $3,
            commissioned_bounty_id = $4,
            evaluated_at = now()
        WHERE submission_id = $1
          AND status = 'pending'
        RETURNING
            submission_id, title, body, submitter, parent_submission_id,
            suggested_category, suggested_capabilities, status, verdict,
            decision_id, commissioned_bounty_id, created_at, evaluated_at
        "#,
    )
    .bind(id)
    .bind(&req.verdict)
    .bind(req.decision_id)
    .bind(req.commissioned_bounty_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        warn!(error = %e, "record_evaluation update failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match row {
        None => Err(StatusCode::CONFLICT), // already evaluated or not found
        Some(row) => {
            let resp = intake_from_row(row).map_err(|e| {
                warn!(error = %e, "record_evaluation row map failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            Ok(Json(resp))
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────

fn intake_from_row(row: sqlx::postgres::PgRow) -> Result<IntakeResponse, String> {
    let caps_json: JsonValue = row
        .try_get("suggested_capabilities")
        .map_err(|e| e.to_string())?;
    let suggested_capabilities: Vec<String> = match caps_json {
        JsonValue::Array(arr) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    };

    Ok(IntakeResponse {
        submission_id: row.try_get("submission_id").map_err(|e| e.to_string())?,
        title: row.try_get("title").map_err(|e| e.to_string())?,
        body: row.try_get("body").map_err(|e| e.to_string())?,
        submitter: row.try_get("submitter").map_err(|e| e.to_string())?,
        parent_submission_id: row
            .try_get("parent_submission_id")
            .map_err(|e| e.to_string())?,
        suggested_category: row
            .try_get("suggested_category")
            .map_err(|e| e.to_string())?,
        suggested_capabilities,
        status: row.try_get("status").map_err(|e| e.to_string())?,
        verdict: row.try_get("verdict").map_err(|e| e.to_string())?,
        decision_id: row.try_get("decision_id").map_err(|e| e.to_string())?,
        commissioned_bounty_id: row
            .try_get("commissioned_bounty_id")
            .map_err(|e| e.to_string())?,
        created_at: row.try_get("created_at").map_err(|e| e.to_string())?,
        evaluated_at: row.try_get("evaluated_at").map_err(|e| e.to_string())?,
        submitter_wallet: row.try_get("submitter_wallet").ok().flatten(),
    })
}
