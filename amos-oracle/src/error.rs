//! Oracle error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OracleError {
    #[error("mission source failed: {0}")]
    MissionSource(String),

    #[error("metrics provider failed: {0}")]
    MetricsProvider(String),

    #[error("contribution registry failed: {0}")]
    ContributionRegistry(String),

    #[error("precedent retrieval failed: {0}")]
    PrecedentRetrieval(String),

    #[error("event log write failed: {0}")]
    EventLog(String),

    #[error("LLM call failed: {0}")]
    Llm(String),

    #[error("decision structure invalid: {0}")]
    InvalidDecision(String),

    #[error("daily budget exceeded: Oracle cannot self-authorize {attempted} on top of {spent_today} (cap: {cap})")]
    BudgetExceeded {
        attempted: u64,
        spent_today: u64,
        cap: u64,
    },

    #[error("confidence {confidence} below self-authorization threshold {threshold}; escalating")]
    ConfidenceBelowThreshold { confidence: f64, threshold: f64 },

    #[error(
        "per-bounty ceiling exceeded: {points} points > {ceiling} auto-self-authorization cap"
    )]
    PerBountyCeilingExceeded { points: u64, ceiling: u64 },

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("internal: {0}")]
    Internal(String),
}
