//! Cross-harness reputation oracle routes.

use crate::identity::Principal;
use crate::{reputation::ReputationEngine, state::RelayState};
use axum::Extension;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

/// Build reputation routes.
pub fn routes() -> Router<RelayState> {
    Router::new()
        .route("/{agent_id}", get(get_reputation))
        .route("/report", post(report_outcome))
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct ReportOutcomeRequest {
    pub harness_id: String,
    pub agent_id: Uuid,
    pub task_id: String,
    pub outcome: TaskOutcome,
    pub quality_score: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TaskOutcome {
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReputationResponse {
    pub agent_id: Uuid,
    pub trust_level: u8,
    pub completion_rate: f64,
    pub quality_score: f64,
    pub total_completed: i32,
    pub total_failed: i32,
    pub total_tasks: i32,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct OutcomeReportResponse {
    pub id: Uuid,
    pub harness_id: String,
    pub agent_id: Uuid,
    pub task_id: String,
    pub outcome: TaskOutcome,
    pub quality_score: Option<i16>,
    pub reported_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct OutcomeRow {
    outcome: TaskOutcome,
    quality_score: Option<i16>,
}

// =============================================================================
// HANDLERS
// =============================================================================

/// Get reputation metrics for an agent.
async fn get_reputation(
    State(state): State<RelayState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<ReputationResponse>, StatusCode> {
    // Fetch all outcome reports for this agent
    let reports = sqlx::query_as::<_, OutcomeRow>(
        r#"
        SELECT
            outcome::text AS outcome,
            quality_score
        FROM relay_reputation_reports
        WHERE agent_id = $1 AND authenticated_reporter IS NOT NULL
        "#,
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        warn!(
            "Failed to fetch reputation reports for agent {}: {}",
            agent_id, e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let total_tasks = reports.len() as i32;
    let total_completed = reports
        .iter()
        .filter(|r| matches!(r.outcome, TaskOutcome::Completed))
        .count() as i32;
    let total_failed = reports
        .iter()
        .filter(|r| matches!(r.outcome, TaskOutcome::Failed))
        .count() as i32;

    let completion_rate = if total_tasks > 0 {
        (total_completed as f64) / (total_tasks as f64)
    } else {
        0.0
    };

    // Calculate average quality score (only from completed tasks)
    let quality_scores: Vec<i16> = reports
        .iter()
        .filter(|r| matches!(r.outcome, TaskOutcome::Completed))
        .filter_map(|r| r.quality_score)
        .collect();

    let avg_quality = if !quality_scores.is_empty() {
        quality_scores.iter().map(|s| *s as i64).sum::<i64>() as f64 / quality_scores.len() as f64
    } else {
        0.0
    };

    // Compute trust level using the reputation engine
    let trust_level = ReputationEngine::compute_trust_level(
        total_completed as u32,
        total_failed as u32,
        avg_quality,
    );

    Ok(Json(ReputationResponse {
        agent_id,
        trust_level,
        completion_rate,
        quality_score: avg_quality,
        total_completed,
        total_failed,
        total_tasks,
    }))
}

/// Report a task outcome from a harness.
async fn report_outcome(
    State(state): State<RelayState>,
    Extension(principal): Extension<Principal>,
    Json(req): Json<ReportOutcomeRequest>,
) -> Result<(StatusCode, Json<OutcomeReportResponse>), StatusCode> {
    let reporter = principal.require_scope("reputation:write")?;
    // Input validation
    if req.harness_id.is_empty() || req.harness_id.len() > 255 {
        warn!(
            "Invalid harness_id length in outcome report: {}",
            req.harness_id.len()
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.task_id.is_empty() || req.task_id.len() > 255 {
        warn!(
            "Invalid task_id length in outcome report: {}",
            req.task_id.len()
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(score) = req.quality_score {
        if score > 100 {
            warn!("Quality score out of range: {} (must be 0-100)", score);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Anti-farming: verify the agent exists and the harness is registered
    let agent_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM relay_agents WHERE id = $1 AND wallet_verified AND status = 'active')",
    )
    .bind(req.agent_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if !agent_exists {
        warn!(
            "Reputation report rejected: agent {} not found or inactive",
            req.agent_id
        );
        return Err(StatusCode::NOT_FOUND);
    }

    let harness: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM relay_harnesses WHERE harness_id=$1 AND status='active'",
    )
    .bind(&req.harness_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let harness = harness.ok_or(StatusCode::NOT_FOUND)?;
    let report_id = Uuid::new_v4();
    let now = Utc::now();

    let report = sqlx::query_as::<_, OutcomeReportResponse>(
        r#"
        WITH report AS (
            INSERT INTO relay_reputation_reports (
                id, harness_id, agent_id, task_id, outcome,
                quality_score, reported_at, authenticated_reporter
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
        )
        SELECT report.id, h.harness_id, report.agent_id, report.task_id,
               report.outcome::text AS outcome, report.quality_score, report.reported_at
        FROM report JOIN relay_harnesses h ON h.id=report.harness_id
        "#,
    )
    .bind(report_id)
    .bind(harness)
    .bind(req.agent_id)
    .bind(&req.task_id)
    .bind(match req.outcome {
        TaskOutcome::Completed => "completed",
        TaskOutcome::Failed => "failed",
    })
    .bind(req.quality_score.map(|s| s as i16))
    .bind(now)
    .bind(reporter)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        warn!("Failed to report outcome: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!(
        "Reported {:?} outcome for agent {} on task {} from harness {}",
        req.outcome, req.agent_id, req.task_id, req.harness_id
    );

    // Update agent's aggregated stats
    let _ = update_agent_stats(&state, req.agent_id).await;

    Ok((StatusCode::CREATED, Json(report)))
}

/// Helper function to update agent's cached reputation stats.
async fn update_agent_stats(state: &RelayState, agent_id: Uuid) -> Result<(), StatusCode> {
    // Fetch all reports for this agent
    let reports = sqlx::query_as::<_, OutcomeRow>(
        r#"
        SELECT
            outcome::text AS outcome,
            quality_score
        FROM relay_reputation_reports
        WHERE agent_id = $1 AND authenticated_reporter IS NOT NULL
        "#,
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total_completed = reports
        .iter()
        .filter(|r| matches!(r.outcome, TaskOutcome::Completed))
        .count() as i32;

    let total_failed = reports
        .iter()
        .filter(|r| matches!(r.outcome, TaskOutcome::Failed))
        .count() as i32;

    let quality_scores: Vec<i16> = reports
        .iter()
        .filter(|r| matches!(r.outcome, TaskOutcome::Completed))
        .filter_map(|r| r.quality_score)
        .collect();

    let avg_quality = if !quality_scores.is_empty() {
        Some(
            quality_scores.iter().map(|s| *s as i64).sum::<i64>() as f64
                / quality_scores.len() as f64,
        )
    } else {
        None
    };

    // Compute trust level
    let trust_level = ReputationEngine::compute_trust_level(
        total_completed as u32,
        total_failed as u32,
        avg_quality.unwrap_or(0.0),
    );

    // Update the agent record
    sqlx::query(
        r#"
        UPDATE relay_agents
        SET
            total_bounties_completed = $1,
            avg_quality_score = $2,
            trust_level = $3,
            total_bounties_failed = $5,
            completion_rate = $6
        WHERE id = $4
        "#,
    )
    .bind(total_completed)
    .bind(avg_quality.unwrap_or(0.0))
    .bind(trust_level as i16)
    .bind(agent_id)
    .bind(total_failed)
    .bind(if total_completed + total_failed == 0 {
        0.0
    } else {
        total_completed as f64 / (total_completed + total_failed) as f64
    })
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}
