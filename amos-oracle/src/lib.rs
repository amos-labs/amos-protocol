//! # amos-oracle
//!
//! Mission-aware decision layer for AMOS. Two paths, same reasoning:
//!
//! - **Intake** — a submission arrives (customer request, bug report, agent-proposed
//!   bounty). Oracle decides: commission it as a system bounty, refine the scope,
//!   reject it, or escalate to council.
//! - **Review** — a bounty was completed and mechanically verified by the QA bot.
//!   Oracle decides: approve (mission-aligned, pays out), reject (failed
//!   mission-alignment), request revision (fixable), or escalate to council.
//!
//! ## Design principles
//!
//! 1. **Plural from day one.** Any trust-5 council-flagged agent with accumulated
//!    `oracle_review` contribution points can operate. Routing is by reputation.
//! 2. **Confidence is a first-class output.** Every decision carries a [0.0, 1.0]
//!    confidence score. Below threshold → auto-escalate to council.
//! 3. **Precedent-aware, not stateless.** Before deciding, Oracle retrieves N=5
//!    semantically-similar past decisions and factors them into the prompt.
//! 4. **Dual-horizon reasoning.** Every decision separates `short_term_value` (30-90
//!    days) from `long_term_value` (3-10 years) and resolves tensions explicitly.
//! 5. **Built for drift detection.** Every decision writes to a durable event log.
//!    A scheduled drift job joins decisions to downstream outcomes and feeds drift
//!    metrics back into the Oracle's confidence threshold (auto-tighten on
//!    calibration degradation).
//!
//! ## AMOS-first; extension-aware
//!
//! The crate is built for AMOS first — the concrete types and default impls target
//! AMOS's mission, metrics, and contribution taxonomy. Trait boundaries exist
//! where the coupling is natural ([`MissionSource`], [`MetricsProvider`],
//! [`ContributionRegistry`], [`EventLog`]) so that a future extraction into a
//! substrate-generic `oracle-core` is tractable, but the current crate does not
//! pre-generalize. One implementation per trait, for AMOS.
//!
//! See `docs/OPS_ORACLE_001_DRAFT.md` for the spec this implements.

pub mod agent;
pub mod bedrock;
pub mod decision;
pub mod error;
pub mod intake;
pub mod llm;
pub mod metrics;
pub mod mission;
pub mod precedent;
pub mod prompt;
pub mod registry;
pub mod review;

pub use agent::OracleAgent;
pub use decision::{Confidence, Decision, IntakeVerdict, ReviewVerdict};
pub use error::OracleError;
pub use llm::LlmClient;

/// Crate-level result alias.
pub type Result<T> = std::result::Result<T, OracleError>;
