//! Metrics snapshot — the Oracle's view of relay state.
//!
//! Composes on-chain pool state with DB aggregations to produce
//! `amos_oracle::metrics::RelaySnapshot`. Unknown economic measurements are
//! null; contribution points are never reported as settled AMOS revenue.

use crate::{solana::new_daily_pool, state::RelayState};
use axum::{extract::State, response::Json, routing::get, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use tracing::warn;

pub fn routes() -> Router<RelayState> {
    Router::new().route("/snapshot", get(snapshot))
}

#[derive(Debug, Serialize)]
struct RelaySnapshotResponse {
    taken_at: DateTime<Utc>,

    // Pool state
    daily_emission_remaining_points: Option<u64>,
    daily_pool_points_distributed: Option<u64>,
    growth_pool_cap_bps: Option<u16>,

    // Bounty lifecycle counts (rolling 7d)
    bounties_posted_7d: u32,
    bounties_claimed_7d: u32,
    bounties_settled_7d: u32,
    bounties_rejected_7d: u32,

    // Value flow (rolling 7d, in AMOS atomic units)
    commercial_volume_7d: Option<u64>,
    system_emission_7d: Option<u64>,

    // Agent activity
    active_agents_7d: u32,
    avg_quality_score_7d: f64,

    // Category mix (rolling 7d)
    category_counts_7d: BTreeMap<String, u32>,
}

async fn snapshot(State(state): State<RelayState>) -> Json<RelaySnapshotResponse> {
    let taken_at = Utc::now();

    // ── Pool state (on-chain) ──────────────────────────────────────────
    let (distributed_points, growth_cap) = if let Some(solana) = &state.solana {
        match solana.read_config_timing().await {
            Ok((_, day)) => match solana.read_daily_pool(day).await {
                Ok(pool) => (
                    Some(pool.unwrap_or_else(|| new_daily_pool(day)).total_points),
                    Some(amos_protocol_math::growth_cap_bps(day as u64)),
                ),
                Err(error) => {
                    warn!(%error, "Economic pool measurement unavailable");
                    (None, None)
                }
            },
            Err(error) => {
                warn!(%error, "Economic clock measurement unavailable");
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    // ── Bounty lifecycle counts over the last 7 days ───────────────────
    let seven_days_ago = taken_at - chrono::Duration::days(7);

    let posted_7d = count_bounties_since(&state, "created_at", seven_days_ago, None).await;
    let claimed_7d = count_bounties_since(&state, "claimed_at", seven_days_ago, None).await;
    let settled_7d = count_bounties_since(
        &state,
        "approved_at",
        seven_days_ago,
        Some("status = 'approved'"),
    )
    .await;
    let rejected_7d = count_bounties_since(
        &state,
        "rejected_at",
        seven_days_ago,
        Some("status = 'rejected'"),
    )
    .await;

    // No authoritative source-tagged settlement ledger is available here.
    // reward_tokens are contribution points, not paid AMOS. Never duplicate
    // those points into both commercial revenue and system emission.
    let commercial_volume_7d = None;
    let system_emission_7d = None;

    // Active agents = distinct agents that claimed OR submitted in window.
    let active_agents_7d: u32 = sqlx::query_scalar::<_, Option<i64>>(
        r#"
        SELECT COUNT(DISTINCT claimed_by_agent_id)::bigint
        FROM relay_bounties
        WHERE claimed_at >= $1
          AND claimed_by_agent_id IS NOT NULL
        "#,
    )
    .bind(seven_days_ago)
    .fetch_one(&state.db)
    .await
    .ok()
    .flatten()
    .unwrap_or(0)
    .max(0)
    .min(u32::MAX as i64) as u32;

    // Average quality score over approved bounties in window.
    let avg_quality_score_7d: f64 = sqlx::query_scalar::<_, Option<f64>>(
        r#"
        SELECT AVG(quality_score::double precision)
        FROM relay_bounties
        WHERE approved_at >= $1
          AND quality_score IS NOT NULL
        "#,
    )
    .bind(seven_days_ago)
    .fetch_one(&state.db)
    .await
    .ok()
    .flatten()
    .unwrap_or(0.0);

    // Category mix.
    let category_counts_7d = fetch_category_counts(&state, seven_days_ago).await;

    Json(RelaySnapshotResponse {
        taken_at,
        daily_emission_remaining_points: None,
        daily_pool_points_distributed: distributed_points,
        growth_pool_cap_bps: growth_cap,
        bounties_posted_7d: posted_7d,
        bounties_claimed_7d: claimed_7d,
        bounties_settled_7d: settled_7d,
        bounties_rejected_7d: rejected_7d,
        commercial_volume_7d,
        system_emission_7d,
        active_agents_7d,
        avg_quality_score_7d,
        category_counts_7d,
    })
}

async fn count_bounties_since(
    state: &RelayState,
    column: &str,
    since: DateTime<Utc>,
    extra_predicate: Option<&str>,
) -> u32 {
    // Column names are caller-controlled and constant (never user input), so
    // format!-ing here is safe.
    let predicate = extra_predicate
        .map(|p| format!("AND {}", p))
        .unwrap_or_default();
    let sql = format!(
        "SELECT COUNT(*)::bigint FROM relay_bounties WHERE {} >= $1 {}",
        column, predicate
    );

    let count: i64 = sqlx::query_scalar::<_, Option<i64>>(&sql)
        .bind(since)
        .fetch_one(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);
    count.max(0).min(u32::MAX as i64) as u32
}

async fn fetch_category_counts(state: &RelayState, since: DateTime<Utc>) -> BTreeMap<String, u32> {
    let rows = sqlx::query_as::<_, (Option<String>, i64)>(
        r#"
        SELECT category, COUNT(*)::bigint
        FROM relay_bounties
        WHERE created_at >= $1
        GROUP BY category
        "#,
    )
    .bind(since)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut out = BTreeMap::new();
    for (cat, n) in rows {
        let key = cat.unwrap_or_else(|| "uncategorized".to_string());
        let n_u32 = n.max(0).min(u32::MAX as i64) as u32;
        out.insert(key, n_u32);
    }
    out
}
