//! Precedent retrieval + event log.
//!
//! Every decision the Oracle makes is written to a durable event log, keyed by
//! `decision_id`. Downstream outcomes (council overrides, claim success,
//! settlement result, subsequent-bounty-chain outcomes) are joined back to the
//! same id. This is what powers:
//!
//! - **Precedent retrieval** — before every decision, retrieve N=5 semantically
//!   similar past decisions and factor them into the prompt for consistency.
//! - **Drift detection** — a scheduled job computes calibration (predicted
//!   confidence vs. actual outcome match rate), category drift, systematic
//!   bias. Drift metrics feed back into the Oracle's confidence threshold.
//!
//! MVP event log uses Postgres (same DB as relay). Retrieval is keyword-based
//! initially; pgvector-backed semantic retrieval is a follow-up once the
//! corpus is large enough to matter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::decision::Decision;
use crate::Result;

/// Outcome records joined back to a decision via `decision_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Outcome {
    CouncilOverride {
        original_verdict: String,
        override_verdict: String,
        override_reasoning: String,
    },
    CommissionedBountyClaimed {
        bounty_id: Uuid,
    },
    CommissionedBountySettled {
        bounty_id: Uuid,
        settlement_tx: String,
    },
    ApprovedBountySettled {
        bounty_id: Uuid,
        settlement_tx: String,
    },
    /// Second-order: a decision Oracle made produced a later bounty that
    /// itself settled. Strongest signal for "did this actually advance the
    /// mission in the long run."
    DownstreamChainSettled {
        chain_length: u32,
        last_bounty_id: Uuid,
    },
}

/// Event log interface. Oracle writes every decision; a downstream join writes
/// outcomes.
#[async_trait]
pub trait EventLog: Send + Sync {
    async fn record_decision(&self, decision: &Decision) -> Result<()>;
    async fn record_outcome(&self, decision_id: Uuid, outcome: Outcome) -> Result<()>;

    /// Retrieve N semantically-similar past decisions for the given query. MVP
    /// uses keyword/category/verdict filtering; upgrade path is
    /// pgvector-embedding-based similarity.
    async fn similar_decisions(&self, query: &str, n: usize) -> Result<Vec<Decision>>;

    /// Fetch a single decision by id (for audit / replay).
    async fn decision_by_id(&self, decision_id: Uuid) -> Result<Option<Decision>>;
}

/// Postgres-backed event log.
///
/// Schema (migration to be added in amos-relay/migrations/ or a dedicated
/// amos-oracle/migrations/ once we wire up sqlx there):
///
/// ```sql
/// CREATE TABLE oracle_decisions (
///     decision_id  UUID PRIMARY KEY,
///     path         TEXT NOT NULL,
///     payload      JSONB NOT NULL,
///     decided_at   TIMESTAMPTZ NOT NULL,
///     prompt_version TEXT NOT NULL,
///     model_version  TEXT NOT NULL
/// );
///
/// CREATE TABLE oracle_outcomes (
///     outcome_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
///     decision_id   UUID NOT NULL REFERENCES oracle_decisions(decision_id),
///     outcome_kind  TEXT NOT NULL,
///     payload       JSONB NOT NULL,
///     recorded_at   TIMESTAMPTZ NOT NULL DEFAULT now()
/// );
/// ```
pub struct PgEventLog {
    pub pool: sqlx::PgPool,
}

impl PgEventLog {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventLog for PgEventLog {
    async fn record_decision(&self, _decision: &Decision) -> Result<()> {
        // MVP: not yet wired up. Implementation sketch:
        //   INSERT INTO oracle_decisions (decision_id, path, payload, ...)
        Err(crate::OracleError::EventLog(
            "PgEventLog::record_decision not yet implemented — see precedent.rs".into(),
        ))
    }

    async fn record_outcome(&self, _decision_id: Uuid, _outcome: Outcome) -> Result<()> {
        Err(crate::OracleError::EventLog(
            "PgEventLog::record_outcome not yet implemented — see precedent.rs".into(),
        ))
    }

    async fn similar_decisions(&self, _query: &str, _n: usize) -> Result<Vec<Decision>> {
        // MVP returns empty; corpus is empty pre-Oracle-live anyway.
        Ok(vec![])
    }

    async fn decision_by_id(&self, _decision_id: Uuid) -> Result<Option<Decision>> {
        Ok(None)
    }
}

/// Helper for "joined" views used by the drift monitor.
#[derive(Debug, Clone)]
pub struct DecisionWithOutcomes {
    pub decision: Decision,
    pub outcomes: Vec<(DateTime<Utc>, Outcome)>,
}
