//! The Oracle agent itself.
//!
//! Wires together [`MissionSource`], [`MetricsProvider`], [`ContributionRegistry`],
//! and [`EventLog`] into a single callable. Callers construct an [`OracleAgent`]
//! and call [`OracleAgent::intake`] or [`OracleAgent::review`] with the
//! appropriate input type.

use std::sync::Arc;

use crate::metrics::MetricsProvider;
use crate::mission::MissionSource;
use crate::precedent::EventLog;
use crate::registry::ContributionRegistry;

/// Confidence + budget thresholds for this Oracle instance.
///
/// Defaults mirror OPS_ORACLE_001_DRAFT.md section "Treasury safety" for
/// intake and "Safety (v1 must-haves)" for review.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Minimum confidence for self-authorization on intake. Starts strict
    /// (0.80) and loosens as precedent accumulates.
    pub intake_self_auth_confidence: f64,

    /// Minimum confidence for self-authorization on review. Tighter than
    /// intake because review is immediate spend.
    pub review_self_auth_confidence: f64,

    /// Daily commissioning budget cap, in points, expressed as fraction of
    /// daily emission. Oracle can auto-commission up to this share before
    /// auto-escalating. Default 0.10 = 10%, sub-cap of protocol-level 15%.
    pub intake_daily_budget_fraction: f64,

    /// Daily approval budget cap. Default 0.40 = 40%.
    pub review_daily_budget_fraction: f64,

    /// Per-bounty ceiling for intake self-authorization (points). Above this
    /// → escalate. Default 500.
    pub intake_per_bounty_ceiling: u64,

    /// Per-bounty ceiling for review self-authorization (points).
    pub review_per_bounty_ceiling: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            intake_self_auth_confidence: 0.80,
            review_self_auth_confidence: 0.85,
            intake_daily_budget_fraction: 0.10,
            review_daily_budget_fraction: 0.40,
            intake_per_bounty_ceiling: 500,
            review_per_bounty_ceiling: 500,
        }
    }
}

/// A single Oracle operator.
///
/// Multiple `OracleAgent` instances can run concurrently (plural Oracles).
/// Each instance reads its own configured mission source + metrics provider +
/// registry; shares the event log if routing is centralized, or has its own
/// event log if fully independent.
pub struct OracleAgent {
    pub mission: Arc<dyn MissionSource>,
    pub metrics: Arc<dyn MetricsProvider>,
    pub registry: Arc<dyn ContributionRegistry>,
    pub event_log: Arc<dyn EventLog>,
    pub thresholds: Thresholds,
    pub prompt_version: String,
    pub model_version: String,
}

impl OracleAgent {
    pub fn builder() -> OracleAgentBuilder {
        OracleAgentBuilder::default()
    }

    /// Intake path — delegates to `intake::evaluate_intake`.
    pub async fn intake(
        &self,
        submission: crate::intake::IntakeSubmission,
    ) -> crate::Result<crate::Decision> {
        crate::intake::evaluate_intake(submission).await
    }

    /// Review path — delegates to `review::evaluate_review`.
    pub async fn review(
        &self,
        request: crate::review::ReviewRequest,
    ) -> crate::Result<crate::Decision> {
        crate::review::evaluate_review(request).await
    }
}

#[derive(Default)]
pub struct OracleAgentBuilder {
    mission: Option<Arc<dyn MissionSource>>,
    metrics: Option<Arc<dyn MetricsProvider>>,
    registry: Option<Arc<dyn ContributionRegistry>>,
    event_log: Option<Arc<dyn EventLog>>,
    thresholds: Option<Thresholds>,
    prompt_version: Option<String>,
    model_version: Option<String>,
}

impl OracleAgentBuilder {
    pub fn mission(mut self, m: Arc<dyn MissionSource>) -> Self {
        self.mission = Some(m);
        self
    }
    pub fn metrics(mut self, m: Arc<dyn MetricsProvider>) -> Self {
        self.metrics = Some(m);
        self
    }
    pub fn registry(mut self, r: Arc<dyn ContributionRegistry>) -> Self {
        self.registry = Some(r);
        self
    }
    pub fn event_log(mut self, e: Arc<dyn EventLog>) -> Self {
        self.event_log = Some(e);
        self
    }
    pub fn thresholds(mut self, t: Thresholds) -> Self {
        self.thresholds = Some(t);
        self
    }
    pub fn prompt_version(mut self, v: impl Into<String>) -> Self {
        self.prompt_version = Some(v.into());
        self
    }
    pub fn model_version(mut self, v: impl Into<String>) -> Self {
        self.model_version = Some(v.into());
        self
    }

    pub fn build(self) -> crate::Result<OracleAgent> {
        Ok(OracleAgent {
            mission: self
                .mission
                .ok_or_else(|| crate::OracleError::Internal("mission source required".into()))?,
            metrics: self
                .metrics
                .ok_or_else(|| crate::OracleError::Internal("metrics provider required".into()))?,
            registry: self.registry.ok_or_else(|| {
                crate::OracleError::Internal("contribution registry required".into())
            })?,
            event_log: self
                .event_log
                .ok_or_else(|| crate::OracleError::Internal("event log required".into()))?,
            thresholds: self.thresholds.unwrap_or_default(),
            prompt_version: self
                .prompt_version
                .ok_or_else(|| crate::OracleError::Internal("prompt_version required".into()))?,
            model_version: self
                .model_version
                .ok_or_else(|| crate::OracleError::Internal("model_version required".into()))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_match_spec() {
        let t = Thresholds::default();
        assert_eq!(t.intake_self_auth_confidence, 0.80);
        assert_eq!(t.review_self_auth_confidence, 0.85);
        assert_eq!(t.intake_daily_budget_fraction, 0.10);
        assert_eq!(t.review_daily_budget_fraction, 0.40);
        assert_eq!(t.intake_per_bounty_ceiling, 500);
    }

    #[test]
    fn builder_rejects_missing_fields() {
        let r = OracleAgent::builder().build();
        assert!(r.is_err());
    }
}
