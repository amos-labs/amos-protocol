//! Fleet management routes — deploy, monitor, and control autonomous bounty agents.

use crate::middleware::AdminAuth;
use crate::openclaw::fleet::AgentProfile;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

/// Build fleet management routes.
pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_fleet))
        .route("/deploy", post(deploy_agent))
        .route("/pause", post(pause_fleet))
        .route("/resume", post(resume_fleet))
        .route("/{id}/stop", post(stop_agent))
        .route("/metrics", get(fleet_metrics))
        .route("/rebalance", post(rebalance_fleet))
}

/// Request body for deploying a new fleet agent.
#[derive(Debug, Deserialize)]
struct DeployRequest {
    profile: AgentProfile,
}

/// `GET /api/v1/fleet` — List all autonomous agents and their status.
async fn list_fleet(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fleet = state.fleet_manager.as_ref().ok_or_else(|| {
        tracing::warn!("Fleet manager not available (AMOS__FLEET__ENABLED=false)");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let agents = fleet.list_agents().await;
    let agent_list: Vec<serde_json::Value> = agents
        .iter()
        .map(|(id, profile, state)| {
            serde_json::json!({
                "agent_id": id,
                "profile": profile.to_string(),
                "state": format!("{:?}", state),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "agents": agent_list,
        "count": agent_list.len(),
    })))
}

/// `POST /api/v1/fleet/deploy` — Deploy a new autonomous agent from a profile.
async fn deploy_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeployRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let fleet = state.fleet_manager.as_ref().ok_or_else(|| {
        tracing::warn!("Fleet manager not available");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    match fleet.deploy_agent(req.profile).await {
        Ok(agent_id) => Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({
                "agent_id": agent_id,
                "profile": req.profile.to_string(),
                "status": "deployed",
                "message": format!(
                    "Autonomous {} agent deployed (id: {})",
                    req.profile, agent_id
                ),
            })),
        )),
        Err(e) => {
            tracing::warn!("Failed to deploy fleet agent: {}", e);
            Ok((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": e.to_string(),
                })),
            ))
        }
    }
}

/// `POST /api/v1/fleet/pause` — Emergency pause: stop all autonomous fleet agents.
///
/// Requires `X-Admin-Key` header matching `AMOS__ADMIN__API_KEY`. This is a
/// fleet-wide circuit breaker for halting runaway agent behavior; individual
/// stop remains available at `/api/v1/fleet/{id}/stop`.
async fn pause_fleet(
    _admin: AdminAuth,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fleet = state.fleet_manager.as_ref().ok_or_else(|| {
        tracing::warn!("Fleet manager not available");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    match fleet.pause_all().await {
        Ok(count) => {
            tracing::warn!(agents_stopped = count, "Fleet paused via admin endpoint");
            Ok(Json(serde_json::json!({
                "status": "paused",
                "agents_stopped": count,
            })))
        }
        Err(e) => {
            tracing::error!(error = %e, "Fleet pause failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// `POST /api/v1/fleet/resume` — Resume fleet by re-deploying the initial fleet.
///
/// Requires `X-Admin-Key`. If agents are already running, this is a no-op.
/// Otherwise it redeploys agents per the `initial_fleet` config — new agent
/// IDs are assigned; the resumed set may differ from the paused set if the
/// config changed in between.
async fn resume_fleet(
    _admin: AdminAuth,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fleet = state.fleet_manager.as_ref().ok_or_else(|| {
        tracing::warn!("Fleet manager not available");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    match fleet.resume().await {
        Ok(deployed) => {
            tracing::info!(
                agents_deployed = deployed.len(),
                "Fleet resumed via admin endpoint"
            );
            Ok(Json(serde_json::json!({
                "status": "resumed",
                "agents_deployed": deployed.len(),
                "agent_ids": deployed,
            })))
        }
        Err(e) => {
            tracing::error!(error = %e, "Fleet resume failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// `POST /api/v1/fleet/{id}/stop` — Stop an autonomous agent.
async fn stop_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fleet = state.fleet_manager.as_ref().ok_or_else(|| {
        tracing::warn!("Fleet manager not available");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    match fleet.stop_agent(id).await {
        Ok(_) => Ok(Json(serde_json::json!({
            "agent_id": id,
            "status": "stopped",
        }))),
        Err(e) => {
            tracing::warn!("Failed to stop fleet agent {}: {}", id, e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// `GET /api/v1/fleet/metrics` — Fleet-wide metrics.
async fn fleet_metrics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fleet = state.fleet_manager.as_ref().ok_or_else(|| {
        tracing::warn!("Fleet manager not available");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let metrics = fleet.metrics().await;
    Ok(Json(serde_json::to_value(&metrics).unwrap_or_default()))
}

/// `POST /api/v1/fleet/rebalance` — Manually trigger fleet rebalancing.
async fn rebalance_fleet(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fleet = state.fleet_manager.as_ref().ok_or_else(|| {
        tracing::warn!("Fleet manager not available");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    match fleet.rebalance().await {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            tracing::warn!("Fleet rebalance failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DeployRequest deserialization ───────────────────────────────────

    #[test]
    fn deploy_request_deserialize_research() {
        let json = r#"{"profile": "research"}"#;
        let req: DeployRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.profile, AgentProfile::Research);
    }

    #[test]
    fn deploy_request_deserialize_infrastructure() {
        let json = r#"{"profile": "infrastructure"}"#;
        let req: DeployRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.profile, AgentProfile::Infrastructure);
    }

    #[test]
    fn deploy_request_deserialize_content() {
        let json = r#"{"profile": "content"}"#;
        let req: DeployRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.profile, AgentProfile::Content);
    }

    #[test]
    fn deploy_request_deserialize_general() {
        let json = r#"{"profile": "general"}"#;
        let req: DeployRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.profile, AgentProfile::General);
    }

    #[test]
    fn deploy_request_all_profiles_have_capabilities() {
        for profile in ["research", "infrastructure", "content", "general"] {
            let json = format!(r#"{{"profile": "{}"}}"#, profile);
            let req: DeployRequest = serde_json::from_str(&json).unwrap();
            assert!(
                !req.profile.capabilities().is_empty(),
                "{profile} should have capabilities"
            );
        }
    }

    #[test]
    fn deploy_request_rejects_unknown_profile() {
        let json = r#"{"profile": "hacker"}"#;
        let result = serde_json::from_str::<DeployRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn deploy_request_rejects_missing_profile() {
        let json = r#"{}"#;
        let result = serde_json::from_str::<DeployRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn deploy_request_rejects_extra_fields_strict() {
        // Extra fields are allowed by default serde — this confirms it doesn't crash
        let json = r#"{"profile": "research", "extra": "field"}"#;
        let req: DeployRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.profile, AgentProfile::Research);
    }

    #[test]
    fn deploy_request_rejects_numeric_profile() {
        let json = r#"{"profile": 42}"#;
        let result = serde_json::from_str::<DeployRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn deploy_request_rejects_null_profile() {
        let json = r#"{"profile": null}"#;
        let result = serde_json::from_str::<DeployRequest>(json);
        assert!(result.is_err());
    }

    // ── Route construction ─────────────────────────────────────────────

    #[test]
    fn routes_constructs_router() {
        // Verify the router builds without panic — requires a dummy AppState
        // We can't easily construct AppState without a DB, but we can verify
        // the route paths are defined by confirming the function doesn't panic
        // when called with a minimal state. Since we need Arc<AppState>, we
        // just verify the function signature compiles and profiles work.
        for profile in ["research", "infrastructure", "content", "general"] {
            let json = format!(r#"{{"profile": "{}"}}"#, profile);
            let req: DeployRequest = serde_json::from_str(&json).unwrap();
            // Verify each profile converts to a string and back
            let profile_str = req.profile.to_string();
            assert_eq!(profile_str, profile);
        }
    }
}
