//! API route definitions for the AMOS Network Relay.

pub mod agents;
pub mod bounties;
pub mod escalations;
pub mod harnesses;
pub mod health;
pub mod intakes;
pub mod metrics;
pub mod pool;
pub mod reputation;
pub mod webhooks;

use crate::state::RelayState;
use axum::Router;

/// Build the API routes (v1).
pub fn api_routes() -> Router<RelayState> {
    Router::new()
        .route("/identity/me", axum::routing::get(crate::identity::whoami))
        .nest("/bounties", bounties::routes())
        .nest("/agents", agents::routes())
        .nest("/reputation", reputation::routes())
        .nest("/harnesses", harnesses::routes())
        .nest("/pool", pool::routes())
        .nest("/webhooks", webhooks::routes())
        .nest("/intakes", intakes::routes())
        .nest("/metrics", metrics::routes())
        .nest("/escalations", escalations::routes())
}
