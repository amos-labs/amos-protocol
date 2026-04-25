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
        // Single call to a dedicated relay endpoint. If the endpoint doesn't
        // yet exist (tracked as a separate relay-side task), this errors and
        // the Oracle's prompt-assembly path treats it as "zero commercial
        // signal" — weighting decisions harder toward escalate per
        // constitutional §4. That graceful degradation is intentional.
        let url = format!(
            "{}/api/v1/metrics/snapshot",
            self.relay_url.trim_end_matches('/')
        );

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| crate::OracleError::MetricsProvider(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::OracleError::MetricsProvider(format!(
                "relay returned {status}: {}",
                body.chars().take(300).collect::<String>()
            )));
        }

        let snapshot: RelaySnapshot = resp.json().await.map_err(|e| {
            crate::OracleError::MetricsProvider(format!("response not a valid RelaySnapshot: {e}"))
        })?;

        Ok(snapshot)
    }
}
