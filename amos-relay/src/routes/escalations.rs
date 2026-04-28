//! Escalation queue — where Oracle decisions go when the Oracle declines to
//! self-authorize (low confidence, above ceiling, novel territory, reasoning-
//! substrate touching, etc.). Council pulls from this queue, resolves with a
//! verdict + reasoning, and the resolution joins back to the original
//! decision via `oracle_outcomes`.
//!
//! All endpoints under this router require Bearer-token auth.

use crate::state::RelayState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::Row;
use tracing::{info, warn};
use uuid::Uuid;

pub fn routes() -> Router<RelayState> {
    Router::new()
        .route("/", post(create_escalation).get(list_escalations))
        .route("/{id}/resolve", post(resolve_escalation))
}

// ─── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateEscalationRequest {
    pub decision_id: Uuid,
    pub path: String, // "intake" or "review"
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct EscalationResponse {
    pub escalation_id: Uuid,
    pub decision_id: Uuid,
    pub path: String,
    pub reason: String,
    pub status: String,
    pub council_verdict: Option<String>,
    pub council_reasoning: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Populated when council resolution actually created a bounty
    /// (intake-path + verdict=commission). Null otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commissioned_bounty_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListEscalationsQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveEscalationRequest {
    pub council_verdict: String,
    pub council_reasoning: String,
    pub resolved_by: String,
    /// Required for intake-path resolutions with verdict=commission.
    /// Wallet that gets recorded as the bounty's poster — typically the
    /// founder wallet or a designated treasury wallet.
    #[serde(default)]
    pub poster_wallet: Option<String>,
    /// Optional spec override for intake-commission. When omitted, the handler
    /// uses Oracle's draft `proposed_bounty_spec` from the original Decision.
    /// When supplied, replaces it — useful when council wants to adjust scope,
    /// reward, or deadline, or when Oracle didn't emit one.
    #[serde(default)]
    pub proposed_bounty_spec: Option<JsonValue>,
}

// ─── Handlers ────────────────────────────────────────────────────────────

async fn create_escalation(
    State(state): State<RelayState>,
    Json(req): Json<CreateEscalationRequest>,
) -> Result<(StatusCode, Json<EscalationResponse>), StatusCode> {
    if req.path != "intake" && req.path != "review" {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.reason.trim().is_empty() || req.reason.len() > 10_000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify the decision exists before linking.
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT decision_id FROM oracle_decisions WHERE decision_id = $1")
            .bind(req.decision_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                warn!(error = %e, "create_escalation: decision lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if exists.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let row = sqlx::query(
        r#"
        INSERT INTO oracle_escalations (decision_id, path, reason)
        VALUES ($1, $2, $3)
        RETURNING escalation_id, decision_id, path, reason, status,
                  council_verdict, council_reasoning, resolved_by,
                  resolved_at, created_at
        "#,
    )
    .bind(req.decision_id)
    .bind(&req.path)
    .bind(&req.reason)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        warn!(error = %e, "create_escalation insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(escalation_from_row(row))))
}

async fn list_escalations(
    State(state): State<RelayState>,
    Query(q): Query<ListEscalationsQuery>,
) -> Result<Json<Vec<EscalationResponse>>, StatusCode> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);

    let rows = match q.status.as_deref() {
        Some(s) if s == "pending" || s == "resolved" => {
            sqlx::query(
                r#"
                SELECT escalation_id, decision_id, path, reason, status,
                       council_verdict, council_reasoning, resolved_by,
                       resolved_at, created_at
                FROM oracle_escalations
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
                SELECT escalation_id, decision_id, path, reason, status,
                       council_verdict, council_reasoning, resolved_by,
                       resolved_at, created_at
                FROM oracle_escalations
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
        warn!(error = %e, "list_escalations query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rows.into_iter().map(escalation_from_row).collect()))
}

/// Resolve a pending escalation with a council verdict.
///
/// Beyond the bookkeeping update, this dispatches the council's verdict to
/// the relevant downstream action so the loop closes:
///
/// - **intake-path + commission** — pull `proposed_bounty_spec` from the
///   original Decision, create the relay_bounty row, link it back to the
///   intake. The new bounty is now `open` and an agent can claim it.
/// - **intake-path + reject/refine** — record the council's verdict on
///   the intake row; no bounty action.
/// - **review-path** — TODO. Wires into the existing
///   `/bounties/{id}/{approve|reject|request_revision}` flows; tracked as a
///   follow-up so this commit stays small. Resolution is still recorded.
///
/// Always writes an `oracle_outcomes` `CouncilOverride` entry so the drift
/// monitor sees council activity against the original Oracle decision.
///
/// All steps run in a single transaction.
async fn resolve_escalation(
    State(state): State<RelayState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ResolveEscalationRequest>,
) -> Result<Json<EscalationResponse>, StatusCode> {
    if req.council_reasoning.trim().is_empty() || req.council_reasoning.len() > 10_000 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.resolved_by.trim().is_empty() || req.resolved_by.len() > 128 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        warn!(error = %e, "resolve_escalation: tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 1. Mark the escalation resolved.
    let row = sqlx::query(
        r#"
        UPDATE oracle_escalations
        SET status = 'resolved',
            council_verdict = $2,
            council_reasoning = $3,
            resolved_by = $4,
            resolved_at = now()
        WHERE escalation_id = $1
          AND status = 'pending'
        RETURNING escalation_id, decision_id, path, reason, status,
                  council_verdict, council_reasoning, resolved_by,
                  resolved_at, created_at
        "#,
    )
    .bind(id)
    .bind(&req.council_verdict)
    .bind(&req.council_reasoning)
    .bind(&req.resolved_by)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        warn!(error = %e, "resolve_escalation update failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let row = match row {
        Some(r) => r,
        None => return Err(StatusCode::CONFLICT), // already resolved or not found
    };
    let mut resp = escalation_from_row(row);

    // 2. Load the original Decision payload — the proposed_bounty_spec lives
    //    inside it for intake-path commissions, and we need original verdict
    //    for the outcome record.
    let decision_payload: Option<JsonValue> =
        sqlx::query_scalar("SELECT payload FROM oracle_decisions WHERE decision_id = $1")
            .bind(resp.decision_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                warn!(error = %e, "resolve_escalation: decision lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let decision_payload = decision_payload.ok_or_else(|| {
        warn!(decision_id = %resp.decision_id, "resolve_escalation: decision row missing");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let original_verdict = decision_payload
        .get("verdict")
        .and_then(JsonValue::as_str)
        .unwrap_or("escalate")
        .to_string();

    // 3. Dispatch on (path, verdict). For now intake-path is fully wired;
    //    review-path resolution falls through to outcome-only recording.
    match (resp.path.as_str(), req.council_verdict.as_str()) {
        ("intake", "commission") => {
            // Spec sourcing priority:
            //   1. council-supplied override in the resolve request
            //   2. Oracle's draft from the original Decision
            //   3. error if neither
            let spec = match req.proposed_bounty_spec.clone() {
                Some(s) if !s.is_null() => Some(s),
                _ => decision_payload.get("proposed_bounty_spec").cloned(),
            };
            let spec = match spec {
                Some(s) if !s.is_null() => s,
                _ => {
                    warn!(
                        decision_id = %resp.decision_id,
                        "council voted commission but neither override nor Oracle draft \
                         provided a proposed_bounty_spec"
                    );
                    return Err(StatusCode::UNPROCESSABLE_ENTITY);
                }
            };
            let poster_wallet = req.poster_wallet.as_deref().ok_or_else(|| {
                warn!("resolve: intake commission requires poster_wallet");
                StatusCode::BAD_REQUEST
            })?;
            if !crate::validate_wallet_address(poster_wallet) {
                return Err(StatusCode::BAD_REQUEST);
            }
            let bounty_id = create_bounty_from_spec(&mut tx, &spec, poster_wallet)
                .await
                .map_err(|e| {
                    warn!(error = %e, "resolve: bounty creation failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            link_intake_to_bounty(&mut tx, resp.decision_id, "commission", Some(bounty_id))
                .await
                .map_err(|e| {
                    warn!(error = %e, "resolve: linking intake to bounty failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            resp.commissioned_bounty_id = Some(bounty_id);
            info!(
                escalation_id = %resp.escalation_id,
                bounty_id = %bounty_id,
                "council commissioned bounty"
            );
        }
        ("intake", verdict) if verdict == "reject" || verdict == "refine" => {
            link_intake_to_bounty(&mut tx, resp.decision_id, verdict, None)
                .await
                .map_err(|e| {
                    warn!(error = %e, "resolve: intake update failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }
        ("review", _) => {
            // TODO: dispatch into bounty approve/reject/request_revision.
            // Tracked as a follow-up. Resolution + outcome are still recorded.
            warn!(
                escalation_id = %resp.escalation_id,
                verdict = %req.council_verdict,
                "review-path council resolution not yet wired to bounty action"
            );
        }
        _ => {
            // Unrecognized verdict; outcome still recorded so drift monitor sees.
            warn!(
                path = %resp.path,
                verdict = %req.council_verdict,
                "resolve: no downstream action for this (path, verdict)"
            );
        }
    }

    // 4. Write the outcome row regardless of dispatch path.
    let outcome_payload = serde_json::json!({
        "CouncilOverride": {
            "original_verdict": original_verdict,
            "override_verdict": req.council_verdict,
            "override_reasoning": req.council_reasoning,
        }
    });
    sqlx::query(
        r#"
        INSERT INTO oracle_outcomes (outcome_id, decision_id, outcome_kind, payload)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(resp.decision_id)
    .bind("council_override")
    .bind(&outcome_payload)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        warn!(error = %e, "resolve: outcome insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        warn!(error = %e, "resolve_escalation: tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(resp))
}

/// Create a relay_bounty from a Decision's `proposed_bounty_spec` JSON.
/// Uses the same column shape as the public `POST /api/v1/bounties` handler;
/// returns the new bounty id.
async fn create_bounty_from_spec(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    spec: &JsonValue,
    poster_wallet: &str,
) -> Result<Uuid, sqlx::Error> {
    let title = spec
        .get("title")
        .and_then(JsonValue::as_str)
        .unwrap_or("Council-commissioned bounty");
    let description = spec
        .get("description")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    let category = spec
        .get("category")
        .and_then(JsonValue::as_str)
        .unwrap_or("infrastructure");
    let reward_tokens = spec
        .get("reward_points")
        .and_then(JsonValue::as_u64)
        .unwrap_or(0) as i64;
    let deadline_days = spec
        .get("deadline_days")
        .and_then(JsonValue::as_u64)
        .unwrap_or(7)
        .max(1) as i64;
    let caps = spec
        .get("required_capabilities")
        .cloned()
        .unwrap_or_else(|| JsonValue::Array(vec![]));

    let bounty_id = Uuid::new_v4();
    let now = Utc::now();
    let deadline_at = now + chrono::Duration::days(deadline_days);

    sqlx::query(
        r#"
        INSERT INTO relay_bounties (
            id, title, description, reward_tokens, deadline_at,
            required_capabilities, poster_wallet, status, category,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'open', $8, $9, $9)
        "#,
    )
    .bind(bounty_id)
    .bind(title)
    .bind(description)
    .bind(reward_tokens)
    .bind(deadline_at)
    .bind(&caps)
    .bind(poster_wallet)
    .bind(category)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    Ok(bounty_id)
}

/// Update the oracle_intakes row whose decision_id matches, recording the
/// council's final verdict and the commissioned bounty id (if any).
async fn link_intake_to_bounty(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    decision_id: Uuid,
    verdict: &str,
    bounty_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE oracle_intakes
        SET verdict = $2,
            commissioned_bounty_id = $3
        WHERE decision_id = $1
        "#,
    )
    .bind(decision_id)
    .bind(verdict)
    .bind(bounty_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────

fn escalation_from_row(row: sqlx::postgres::PgRow) -> EscalationResponse {
    EscalationResponse {
        escalation_id: row.try_get("escalation_id").unwrap_or_else(|_| Uuid::nil()),
        decision_id: row.try_get("decision_id").unwrap_or_else(|_| Uuid::nil()),
        path: row.try_get("path").unwrap_or_default(),
        reason: row.try_get("reason").unwrap_or_default(),
        status: row.try_get("status").unwrap_or_default(),
        council_verdict: row.try_get("council_verdict").ok(),
        council_reasoning: row.try_get("council_reasoning").ok(),
        resolved_by: row.try_get("resolved_by").ok(),
        resolved_at: row.try_get("resolved_at").ok(),
        created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
        commissioned_bounty_id: None, // populated by resolve handler when applicable
    }
}
