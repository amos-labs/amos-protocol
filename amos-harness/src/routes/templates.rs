//! Template apply API routes (platform build-plan P1, slice 1).
//!
//! `POST /api/v1/templates/apply` applies a module/template (a composition of
//! components) to this environment. Idempotent. The same operation is exposed
//! to the actor over MCP as the `apply_template` tool.

use crate::state::AppState;
use crate::templates::{ApplyReport, Template, TemplateEngine};
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use std::sync::Arc;

pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new().route("/apply", post(apply_template))
}

async fn apply_template(
    State(state): State<Arc<AppState>>,
    Json(tmpl): Json<Template>,
) -> Result<Json<ApplyReport>, (StatusCode, String)> {
    let engine = TemplateEngine::new(state.db_pool.clone(), state.config.clone());
    let report = engine
        .apply(&tmpl)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(report))
}
