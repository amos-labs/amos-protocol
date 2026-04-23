//! Metrics provider: Oracle's view of the world.
//!
//! Before every decision, Oracle pulls a snapshot of relevant metrics — relay
//! health, pool utilization, category mix, agent activity, commercial volume.
//! These ground the decision in "what's actually happening" rather than "what
//! the prompt says."
//!
//! AMOS-first impl queries the relay's read-only endpoints. Trait boundary
//! exists so a future generic Oracle could swap in a different metrics source.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Result;

/// Snapshot of relay health metrics at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaySnapshot {
    pub taken_at: DateTime<Utc>,

    // Pool state
    pub daily_emission_remaining_points: u64,
    pub daily_pool_points_distributed: u64,
    pub growth_pool_cap_bps: u16,

    // Bounty lifecycle counts (rolling 7d)
    pub bounties_posted_7d: u32,
    pub bounties_claimed_7d: u32,
    pub bounties_settled_7d: u32,
    pub bounties_rejected_7d: u32,

    // Value flow (rolling 7d, in AMOS atomic units)
    pub commercial_volume_7d: u64,
    pub system_emission_7d: u64,

    // Agent activity
    pub active_agents_7d: u32,
    pub avg_quality_score_7d: f64,

    // Category mix (rolling 7d, keyed by category name → count)
    pub category_counts_7d: std::collections::BTreeMap<String, u32>,
}

/// Provider of relay-state observations for the Oracle.
#[async_trait]
pub trait MetricsProvider: Send + Sync {
    /// Current snapshot. Should be cheap to call on every decision; providers
    /// may cache internally.
    async fn snapshot(&self) -> Result<RelaySnapshot>;
}

/// AMOS-specific metrics provider — hits relay.amoslabs.com (or configured URL)
/// with the marketplace API key.
pub struct AmosMetricsProvider {
    pub relay_url: String,
    pub http: reqwest::Client,
    pub api_key: String,
}

impl AmosMetricsProvider {
    pub fn new(relay_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            relay_url: relay_url.into(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl MetricsProvider for AmosMetricsProvider {
    async fn snapshot(&self) -> Result<RelaySnapshot> {
        // MVP: stub. Real implementation pulls from:
        //   GET /api/v1/pool/today  (daily pool state)
        //   GET /api/v1/bounties?limit=... (for rolling 7d counts — needs
        //       aggregation or a new /api/v1/metrics/snapshot endpoint)
        //   GET /api/v1/agents (for active count)
        //
        // Adding a dedicated /api/v1/metrics/snapshot endpoint on the relay is
        // tracked as a follow-up; for now we'll compose from existing
        // endpoints. Left as a TODO to keep the scaffold small.
        Err(crate::OracleError::MetricsProvider(
            "snapshot() not yet implemented — see TODO in amos-oracle/src/metrics.rs".into(),
        ))
    }
}
