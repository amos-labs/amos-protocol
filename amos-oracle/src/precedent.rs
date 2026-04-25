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
/// Schema lives at
/// `amos-relay/migrations/20260423000001_oracle_decisions_and_outcomes.sql`
/// — Oracle shares the relay DB. Creating its own DB is a follow-up.
///
/// `similar_decisions` is MVP: returns N most-recent decisions on the same
/// path. Upgrade path is pgvector-embedding-based similarity over payload
/// text, but pre-live the corpus is empty, so recency is fine and will evolve.
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
    async fn record_decision(&self, decision: &Decision) -> Result<()> {
        let path = match decision.path {
            crate::decision::DecisionPath::Intake => "intake",
            crate::decision::DecisionPath::Review => "review",
        };
        let verdict = decision.verdict.as_str().unwrap_or("unknown").to_string();
        let payload = serde_json::to_value(decision)?;

        sqlx::query(
            r#"
            INSERT INTO oracle_decisions (
                decision_id, path, verdict, confidence,
                prompt_version, model_version, decided_at, payload
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (decision_id) DO NOTHING
            "#,
        )
        .bind(decision.decision_id)
        .bind(path)
        .bind(&verdict)
        .bind(decision.confidence.0)
        .bind(&decision.prompt_version)
        .bind(&decision.model_version)
        .bind(decision.decided_at)
        .bind(&payload)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn record_outcome(&self, decision_id: Uuid, outcome: Outcome) -> Result<()> {
        let outcome_kind = match &outcome {
            Outcome::CouncilOverride { .. } => "council_override",
            Outcome::CommissionedBountyClaimed { .. } => "commissioned_bounty_claimed",
            Outcome::CommissionedBountySettled { .. } => "commissioned_bounty_settled",
            Outcome::ApprovedBountySettled { .. } => "approved_bounty_settled",
            Outcome::DownstreamChainSettled { .. } => "downstream_chain_settled",
        };
        let payload = serde_json::to_value(&outcome)?;

        sqlx::query(
            r#"
            INSERT INTO oracle_outcomes (
                outcome_id, decision_id, outcome_kind, payload
            )
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(decision_id)
        .bind(outcome_kind)
        .bind(&payload)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn similar_decisions(&self, query: &str, n: usize) -> Result<Vec<Decision>> {
        // MVP: path-aware recency. We sniff whether the caller is on intake or
        // review from the query prefix ("intake: ..." / "review: ...") so the
        // precedent corpus is unitary in storage but the retriever returns
        // same-path examples first. If the sniff fails, fall back to most
        // recent across both paths.
        let path = if query.starts_with("intake:") {
            Some("intake")
        } else if query.starts_with("review:") {
            Some("review")
        } else {
            None
        };

        let rows: Vec<(serde_json::Value,)> = match path {
            Some(p) => {
                sqlx::query_as(
                    r#"
                    SELECT payload FROM oracle_decisions
                    WHERE path = $1
                    ORDER BY decided_at DESC
                    LIMIT $2
                    "#,
                )
                .bind(p)
                .bind(n as i64)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT payload FROM oracle_decisions
                    ORDER BY decided_at DESC
                    LIMIT $1
                    "#,
                )
                .bind(n as i64)
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut out = Vec::with_capacity(rows.len());
        for (payload,) in rows {
            match serde_json::from_value::<Decision>(payload) {
                Ok(d) => out.push(d),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "precedent: skipping malformed oracle_decisions row"
                    );
                }
            }
        }
        Ok(out)
    }

    async fn decision_by_id(&self, decision_id: Uuid) -> Result<Option<Decision>> {
        let row: Option<(serde_json::Value,)> = sqlx::query_as(
            r#"
            SELECT payload FROM oracle_decisions
            WHERE decision_id = $1
            "#,
        )
        .bind(decision_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(None),
            Some((payload,)) => Ok(Some(serde_json::from_value::<Decision>(payload)?)),
        }
    }
}

/// Helper for "joined" views used by the drift monitor.
#[derive(Debug, Clone)]
pub struct DecisionWithOutcomes {
    pub decision: Decision,
    pub outcomes: Vec<(DateTime<Utc>, Outcome)>,
}
