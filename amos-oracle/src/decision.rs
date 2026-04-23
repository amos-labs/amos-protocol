//! Structured output types for Oracle decisions.
//!
//! Every Oracle decision (intake or review) produces a [`Decision`] — an
//! auditable record with verdict, confidence, dual-horizon reasoning, mission
//! alignment notes, and similar-past-decisions cited. The same shape is used
//! for both paths; only the `verdict` enum differs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Confidence score in the range [0.0, 1.0].
///
/// Oracle self-authorizes only when confidence is at or above the current
/// threshold (see `agent::thresholds`). Below threshold → verdict becomes
/// `Escalate` regardless of what the reasoning produced.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Confidence(pub f64);

impl Confidence {
    pub const ZERO: Self = Self(0.0);
    pub const HALF: Self = Self(0.5);
    pub const ONE: Self = Self(1.0);

    pub fn new(v: f64) -> Self {
        Self(v.clamp(0.0, 1.0))
    }

    pub fn at_least(self, threshold: f64) -> bool {
        self.0 >= threshold
    }
}

/// Intake path verdicts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntakeVerdict {
    /// Commission: post this submission as a system bounty.
    ///
    /// Requires `proposed_bounty_spec` to be present.
    Commission,
    /// Reject: submission does not advance the mission. Submitter free to build
    /// independently; the system will not fund it.
    Reject,
    /// Refine: submission has merit but scope/phrasing needs tightening. Oracle
    /// emits structured feedback; submitter has 14 days to re-submit with a
    /// `parent_submission_id` linking to this one.
    Refine,
    /// Escalate: confidence too low, budget exceeded, scope too large, or novel
    /// territory. Decision routes to council for sign-off.
    Escalate,
}

/// Review path verdicts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewVerdict {
    /// Approve: mechanical QA passed AND mission-alignment passed. Triggers
    /// on-chain settlement.
    Approve,
    /// Reject: terminal rejection. Mechanical QA passed but mission alignment
    /// failed, or too many revision rounds.
    Reject,
    /// Revise: fixable failure. Oracle emits mission-oriented feedback; worker
    /// gets another attempt.
    Revise,
    /// Escalate: confidence too low, budget exceeded, or novel mission-alignment
    /// question. Decision routes to council.
    Escalate,
}

/// The shared structured output.
///
/// The `verdict` field is serialized as one of the two enums above. Intake
/// decisions use `IntakeVerdict`, review decisions use `ReviewVerdict`. Callers
/// match on [`DecisionPath`] to dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Unique id for joining with downstream outcomes in the drift log.
    pub decision_id: Uuid,

    /// Which path produced this decision.
    pub path: DecisionPath,

    /// The verdict, serialized by path.
    pub verdict: serde_json::Value,

    /// Confidence in [0.0, 1.0].
    pub confidence: Confidence,

    /// How this advances the next 30-90 days.
    pub short_term_value: String,

    /// How this advances the 3-10 year direction.
    pub long_term_value: String,

    /// Where short-term and long-term disagree, and how the Oracle resolved the
    /// tension. If no tension, the literal string "no tension".
    pub tension_resolution: String,

    /// 1-2 paragraphs explaining the mission-alignment judgment.
    pub mission_alignment_notes: String,

    /// For review path: required field explaining how the decision weighted the
    /// asymmetric cost of false-approve (drains treasury) vs false-reject
    /// (angers workers, recoverable). Empty for intake.
    pub false_approve_vs_false_reject_weighting: String,

    /// For intake/commission: the proposed bounty to post.
    pub proposed_bounty_spec: Option<ProposedBountySpec>,

    /// For intake/refine or review/revise: structured feedback to send back.
    pub feedback: Option<String>,

    /// Similar past decisions retrieved from the precedent corpus (N=5 typical).
    pub similar_past_decisions: Vec<PrecedentRef>,

    /// When this decision was made.
    pub decided_at: DateTime<Utc>,

    /// Version of the Oracle's constitutional prompt + model used.
    pub prompt_version: String,
    pub model_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionPath {
    Intake,
    Review,
}

/// A bounty proposal the Oracle wants to commission from an intake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedBountySpec {
    pub title: String,
    pub description: String,
    pub category: String,
    pub required_capabilities: Vec<String>,
    pub reward_points: u64,
    pub reasoning_for_points: String,
    pub deadline_days: u32,
}

/// Reference to a past decision retrieved as precedent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecedentRef {
    pub decision_id: Uuid,
    pub summary: String,
    pub verdict: String,
    pub outcome: Option<String>,
    pub similarity_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_clamps_to_unit_interval() {
        assert_eq!(Confidence::new(-0.1).0, 0.0);
        assert_eq!(Confidence::new(1.5).0, 1.0);
        assert_eq!(Confidence::new(0.7).0, 0.7);
    }

    #[test]
    fn confidence_threshold_check() {
        assert!(Confidence::new(0.85).at_least(0.80));
        assert!(!Confidence::new(0.79).at_least(0.80));
    }

    #[test]
    fn intake_verdict_roundtrip() {
        let v = IntakeVerdict::Commission;
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, r#""commission""#);
        let back: IntakeVerdict = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn review_verdict_roundtrip() {
        let v = ReviewVerdict::Revise;
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, r#""revise""#);
    }

    #[test]
    fn decision_path_roundtrip() {
        assert_eq!(
            serde_json::to_string(&DecisionPath::Intake).unwrap(),
            r#""intake""#
        );
        assert_eq!(
            serde_json::to_string(&DecisionPath::Review).unwrap(),
            r#""review""#
        );
    }
}
