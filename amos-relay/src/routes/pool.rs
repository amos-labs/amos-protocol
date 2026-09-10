//! Pool status routes — exposes the on-chain daily emission pool state.

use crate::{
    solana::{compute_dynamic_max_reward, new_daily_pool},
    state::RelayState,
};
use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::Serialize;
use tracing::warn;

/// One whole AMOS token in lamports.
const ONE_TOKEN_F64: f64 = 1_000_000_000.0;

/// Build pool routes.
pub fn routes() -> Router<RelayState> {
    Router::new().route("/today", get(pool_today))
}

#[derive(Serialize)]
struct PoolStatusResponse {
    day_index: u32,
    /// Total daily emission in AMOS (whole tokens, e.g. 15990.0)
    daily_emission_amos: f64,
    /// Emission that has dripped so far today (based on time elapsed)
    emission_available_amos: f64,
    /// Tokens already distributed today
    tokens_distributed_amos: f64,
    /// Remaining available pool (emission_available - distributed)
    remaining_amos: f64,
    /// Total points accumulated today
    total_points: u64,
    /// Virtual points base added to denominator
    virtual_points_base: u64,
    /// Number of bounty proofs submitted today
    proof_count: u32,
    /// Estimated AMOS payout for a 1000-point bounty at current pool state
    estimated_amos_per_1000_points: f64,
    /// Seconds elapsed since day start
    seconds_elapsed: u64,
}

async fn pool_today(
    State(state): State<RelayState>,
) -> Result<Json<PoolStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    let solana = state.solana.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Solana not configured"})),
        )
    })?;

    let (start_time, day_index) = solana.read_config_timing().await.map_err(|e| {
        warn!(error = %e, "Failed to read config timing for pool status");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("Failed to read on-chain config: {}", e)})),
        )
    })?;

    let now = chrono::Utc::now().timestamp();
    let day_start = start_time as i128 + day_index as i128 * 86400;
    let seconds_elapsed = (now as i128 - day_start).clamp(0, 86400) as u64;

    let pool = solana.read_daily_pool(day_index).await.map_err(|e| {
        warn!(error = %e, "Failed to read daily pool");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("Failed to read on-chain pool: {}", e)})),
        )
    })?;

    let pool = pool.unwrap_or_else(|| new_daily_pool(day_index));

    let daily_emission_amos = pool.daily_emission as f64 / ONE_TOKEN_F64;
    let emission_available = (pool.daily_emission as u128) * (seconds_elapsed as u128) / 86400u128;
    let emission_available_amos = emission_available as f64 / ONE_TOKEN_F64;
    let tokens_distributed_amos = pool.tokens_distributed as f64 / ONE_TOKEN_F64;
    let remaining_amos = emission_available_amos - tokens_distributed_amos;

    // Estimate payout for 1000 points at current pool state
    let est_reward = compute_dynamic_max_reward(1000, &pool, start_time, now);
    let estimated_amos_per_1000_points = est_reward as f64 / ONE_TOKEN_F64;

    Ok(Json(PoolStatusResponse {
        day_index,
        daily_emission_amos,
        emission_available_amos,
        tokens_distributed_amos,
        remaining_amos: remaining_amos.max(0.0),
        total_points: pool.total_points,
        virtual_points_base: 10_000,
        proof_count: pool.proof_count,
        estimated_amos_per_1000_points,
        seconds_elapsed,
    }))
}
