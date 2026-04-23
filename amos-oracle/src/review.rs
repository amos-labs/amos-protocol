//! Review path: evaluate completed work against mission alignment.
//!
//! Runs AFTER the mechanical QA bot. Oracle's job is not "did tests pass" —
//! it is "does this work advance the mission." QA bot's judgment is consumed
//! as one input; Oracle produces the final mission-layer verdict.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Packet describing a bounty submission up for mission-alignment review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub bounty_id: Uuid,

    /// The bounty's original spec (title, description, capabilities, category).
    pub bounty_title: String,
    pub bounty_description: String,
    pub bounty_category: String,
    pub bounty_contribution_type_id: u8,

    /// QA bot's mechanical verdict + evidence (clippy, tests, CI, etc.).
    pub qa_evidence: serde_json::Value,

    /// The worker's submitted proof of completion — typically a PR URL + commit
    /// SHA + structured sections (APPROACH / IMPLEMENTATION / VERIFICATION).
    pub proof: serde_json::Value,

    /// How many revision rounds this submission has been through.
    pub revision_count: u8,
}

/// Entry point for review evaluation. Stub for scaffolding.
///
/// Flow:
/// 1. Load mission snapshot (versioned)
/// 2. Load metrics snapshot
/// 3. Retrieve precedent (N=5 similar past review decisions)
/// 4. Assemble prompt
/// 5. Call LLM with structured-output schema (includes required
///    `false_approve_vs_false_reject_weighting` field — empty fails)
/// 6. Parse into `Decision`
/// 7. Apply guards (confidence ≥ 0.85 for review's tighter threshold, daily
///    approval budget, per-bounty ceiling)
/// 8. Write to event log
/// 9. Return decision
pub async fn evaluate_review(
    _request: ReviewRequest,
    // agent: &OracleAgent
) -> crate::Result<crate::Decision> {
    Err(crate::OracleError::Internal(
        "evaluate_review not yet implemented — scaffolding only".into(),
    ))
}
