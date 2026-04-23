//! Intake path: evaluate a submission as a potential system bounty.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A submission arriving at the intake path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeSubmission {
    /// Unique id for tracking through the pipeline.
    pub submission_id: Uuid,

    /// Human-readable title.
    pub title: String,

    /// Free-form description of the idea / request / bug / proposal.
    pub body: String,

    /// Who submitted (wallet or operator-name).
    pub submitter: String,

    /// If this is a refinement of a prior submission, link back.
    pub parent_submission_id: Option<Uuid>,

    /// Optional capability hints / proposed category from the submitter.
    pub suggested_category: Option<String>,
    pub suggested_capabilities: Vec<String>,
}

/// Entry point for intake evaluation. Lives here rather than in `agent` so the
/// path-specific input shape is close to the rest of the intake code.
///
/// Not yet implemented — stub for scaffolding. The real evaluation flow:
/// 1. Load mission snapshot (versioned)
/// 2. Load metrics snapshot (current relay state)
/// 3. Retrieve precedent (N=5 similar past intake decisions)
/// 4. Assemble prompt (constitutional + thesis + context + metrics + precedent + submission)
/// 5. Call LLM with structured-output schema
/// 6. Parse into `Decision`
/// 7. Apply confidence threshold + budget + per-bounty ceiling checks
/// 8. If any guard trips: rewrite verdict to `Escalate`
/// 9. Write to event log
/// 10. Return decision
pub async fn evaluate_intake(
    _submission: IntakeSubmission,
    // agent: &OracleAgent — constructed with all the traits
) -> crate::Result<crate::Decision> {
    Err(crate::OracleError::Internal(
        "evaluate_intake not yet implemented — scaffolding only".into(),
    ))
}
