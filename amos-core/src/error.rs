//! Unified error types for the AMOS platform.
//!
//! Every subsystem maps its errors into [`AmosError`] so callers get
//! a single, consistent error surface.

use thiserror::Error;

/// Convenience alias used throughout the workspace.
pub type Result<T> = std::result::Result<T, AmosError>;

/// Top-level error enum for the AMOS platform.
#[derive(Error, Debug)]
pub enum AmosError {
    // ── Token Economics ──────────────────────────────────────────────
    #[error("Arithmetic overflow in token calculation: {context}")]
    ArithmeticOverflow { context: String },

    #[error("Insufficient stake: have {have}, need {need}")]
    InsufficientStake { have: u64, need: u64 },

    #[error("Stake too recent: {days_held} days held, need {days_required}")]
    StakeTooRecent { days_held: u64, days_required: u64 },

    #[error("Decay rate {rate_bps} bps out of allowed range [{min_bps}, {max_bps}]")]
    DecayRateOutOfRange {
        rate_bps: u64,
        min_bps: u64,
        max_bps: u64,
    },

    #[error("No revenue available to claim")]
    NoRevenueToClaim,

    #[error("Treasury exhausted: {remaining} tokens remaining")]
    TreasuryExhausted { remaining: u64 },

    #[error("Trust level insufficient: level {current}, need {required}")]
    TrustLevelInsufficient { current: u8, required: u8 },

    #[error("Trust upgrade not eligible: {reason}")]
    TrustUpgradeNotEligible { reason: String },

    #[error("Already at maximum trust level ({level})")]
    AlreadyMaxTrust { level: u8 },

    #[error("Within grace period: {days_remaining} days remaining")]
    WithinGracePeriod { days_remaining: u64 },

    #[error("At decay floor: balance already at minimum preserved amount")]
    AtDecayFloor,

    // ── Agent Runtime ────────────────────────────────────────────────
    #[error("Tool not found: {name}")]
    ToolNotFound { name: String },

    #[error("Tool execution failed: {tool} - {reason}")]
    ToolExecutionFailed { tool: String, reason: String },

    #[error("Model invocation failed: {model} - {reason}")]
    ModelInvocationFailed { model: String, reason: String },

    #[error("Model escalation exhausted after trying: {models_tried:?}")]
    ModelEscalationExhausted { models_tried: Vec<String> },

    #[error("Agent loop exceeded maximum iterations ({max})")]
    AgentLoopExceeded { max: usize },

    #[error("Context window exceeded: {tokens} tokens, max {max_tokens}")]
    ContextWindowExceeded { tokens: usize, max_tokens: usize },

    // ── Database ─────────────────────────────────────────────────────
    #[cfg(feature = "db")]
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[cfg(not(feature = "db"))]
    #[error("Database error: {0}")]
    Database(String),

    // ── HTTP / Network ───────────────────────────────────────────────
    #[cfg(feature = "http")]
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[cfg(not(feature = "http"))]
    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("Solana RPC error: {0}")]
    SolanaRpc(String),

    // ── Configuration ────────────────────────────────────────────────
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    // ── Authorization ────────────────────────────────────────────────
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    // ── Generic ──────────────────────────────────────────────────────
    #[error("Not found: {entity} with id {id}")]
    NotFound { entity: String, id: String },

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AmosError {
    /// Client-safe message for API response bodies.
    ///
    /// Infrastructure errors (database, HTTP, Solana RPC, config, internal)
    /// wrap raw lower-level errors whose `Display` output can leak SQL
    /// fragments, schema names, file paths, or upstream URLs. Those map to
    /// generic messages here; the full error must still be logged
    /// server-side by the caller. Domain errors carry messages written for
    /// API consumers and pass through unchanged.
    pub fn client_message(&self) -> String {
        match self {
            Self::Database(_) => "A database error occurred".to_string(),
            Self::Http(_) => "An upstream request failed".to_string(),
            Self::SolanaRpc(_) => "A blockchain RPC error occurred".to_string(),
            Self::Config(_) | Self::MissingEnvVar(_) => {
                "A server configuration error occurred".to_string()
            }
            Self::Internal(_) | Self::Other(_) => "An internal error occurred".to_string(),
            other => other.to_string(),
        }
    }

    /// HTTP-style status code for API layer mapping.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Unauthorized(_) => 401,
            Self::Forbidden(_) => 403,
            Self::NotFound { .. } => 404,
            Self::Validation(_) => 422,
            Self::InsufficientStake { .. }
            | Self::StakeTooRecent { .. }
            | Self::DecayRateOutOfRange { .. }
            | Self::TrustLevelInsufficient { .. }
            | Self::TrustUpgradeNotEligible { .. }
            | Self::WithinGracePeriod { .. }
            | Self::AtDecayFloor => 422,
            Self::NoRevenueToClaim | Self::TreasuryExhausted { .. } => 409,
            _ => 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_hides_internal_detail() {
        let err = AmosError::Internal("disk full at /var/lib/postgresql/data".to_string());
        let msg = err.client_message();
        assert_eq!(msg, "An internal error occurred");
        assert!(!msg.contains("/var/lib"));
    }

    #[test]
    fn client_message_hides_anyhow_detail() {
        let err = AmosError::Other(anyhow::anyhow!(
            "SELECT * FROM secrets failed: relation does not exist"
        ));
        let msg = err.client_message();
        assert_eq!(msg, "An internal error occurred");
        assert!(!msg.contains("SELECT"));
    }

    #[test]
    fn client_message_hides_rpc_and_config_detail() {
        let rpc = AmosError::SolanaRpc("https://internal-rpc:8899 connection refused".to_string());
        assert!(!rpc.client_message().contains("internal-rpc"));

        let cfg = AmosError::Config("missing key solana.oracle_keypair_path".to_string());
        assert!(!cfg.client_message().contains("oracle_keypair_path"));

        let env = AmosError::MissingEnvVar("AMOS__DATABASE__URL".to_string());
        assert!(!env.client_message().contains("AMOS__DATABASE__URL"));
    }

    #[cfg(not(feature = "db"))]
    #[test]
    fn client_message_hides_database_detail() {
        let err = AmosError::Database("duplicate key violates unique constraint".to_string());
        assert_eq!(err.client_message(), "A database error occurred");
    }

    #[cfg(feature = "db")]
    #[test]
    fn client_message_hides_database_detail() {
        let err = AmosError::Database(sqlx::Error::PoolTimedOut);
        assert_eq!(err.client_message(), "A database error occurred");
    }

    #[test]
    fn client_message_passes_domain_errors_through() {
        let not_found = AmosError::NotFound {
            entity: "collection".to_string(),
            id: "leads".to_string(),
        };
        assert_eq!(not_found.client_message(), not_found.to_string());

        let validation = AmosError::Validation("field 'email' is required".to_string());
        assert_eq!(validation.client_message(), validation.to_string());

        let unauthorized = AmosError::Unauthorized("missing bearer token".to_string());
        assert_eq!(unauthorized.client_message(), unauthorized.to_string());
    }
}
