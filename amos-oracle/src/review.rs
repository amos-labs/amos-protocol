//! Review path: evaluate completed work against mission alignment.
//!
//! Runs AFTER the mechanical QA bot. Oracle's job is not "did tests pass" —
//! it is "does this work advance the mission." QA bot's judgment is consumed
//! as one input; Oracle produces the final mission-layer verdict.
//!
//! Structure mirrors [`crate::intake`] — shared [`prompt::assemble`] for the
//! prompt body, LLM call, structured parse, guards, event log write. Review
//! specifics:
//!
//! - Confidence threshold is tighter (0.85 vs intake's 0.80) because approval
//!   is immediate on-chain spend; false-approve drains the treasury.
//! - Output must include `false_approve_vs_false_reject_weighting` — empty or
//!   generic text fails validation.
//! - No `proposed_bounty_spec` (that's intake-only); instead a
//!   `quality_score_adjustment` in [-10, 10].

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::agent::OracleAgent;
use crate::decision::{Confidence, Decision, DecisionPath, ReviewVerdict};
use crate::prompt::{self, REVIEW_SCHEMA};
use crate::{OracleError, Result};

/// A completed bounty arriving at the review path.
///
/// `qa_evidence` is the mechanical QA bot's output (tests passed, lint clean,
/// etc.); `proof` is the worker's submission (commit hash, diff summary,
/// externally verifiable links). Both are passed through to the LLM — Oracle
/// weighs them together with mission alignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub bounty_id: Uuid,
    pub bounty_title: String,
    pub bounty_description: String,
    pub bounty_category: String,
    pub bounty_contribution_type_id: u8,
    pub qa_evidence: serde_json::Value,
    pub proof: serde_json::Value,
    pub revision_count: u8,
}

/// Raw LLM output shape for review decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewLlmOutput {
    pub verdict: ReviewVerdict,
    pub confidence: f64,
    pub short_term_value: String,
    pub long_term_value: String,
    pub tension_resolution: String,
    pub mission_alignment_notes: String,
    pub false_approve_vs_false_reject_weighting: String,
    pub revise_feedback: Option<String>,
    pub quality_score_adjustment: i8,
}

pub async fn evaluate(agent: &OracleAgent, request: ReviewRequest) -> Result<Decision> {
    let bounty_id = request.bounty_id;
    debug!(%bounty_id, "review: starting evaluation");

    // 1. Mission snapshot
    let mission = agent.mission.current().await?;

    // 2. Metrics snapshot — degrade gracefully
    let metrics = agent.metrics.snapshot().await.ok();
    if metrics.is_none() {
        warn!(%bounty_id, "review: metrics snapshot unavailable; proceeding without");
    }

    // 3. Precedent retrieval — unitary corpus across paths (constitutional §8)
    let precedent_query = format!(
        "review: {} | {}",
        request.bounty_title, request.bounty_description
    );
    let similar = agent
        .event_log
        .similar_decisions(&precedent_query, 5)
        .await
        .unwrap_or_default();

    // 4. Assemble prompt
    let system_prompt = mission.constitutional_provisions.clone();
    let input_block = render_review_input_block(&request);
    let user_message = prompt::assemble(
        &mission,
        metrics.as_ref(),
        &similar,
        &input_block,
        REVIEW_SCHEMA,
    );

    // 5. LLM call
    let raw_response = agent.llm.complete(&system_prompt, &user_message).await?;
    debug!(
        %bounty_id,
        response_len = raw_response.len(),
        "review: LLM responded"
    );

    // 6. Parse
    let llm_out: ReviewLlmOutput = parse_review_output(&raw_response)?;
    validate_required_fields(&llm_out)?;

    // 7. Apply guards — may rewrite verdict to Escalate
    let (final_verdict, confidence) = apply_review_guards(agent, &llm_out)?;

    // 8. Build canonical Decision
    let mut decision = Decision {
        decision_id: Uuid::new_v4(),
        path: DecisionPath::Review,
        verdict: serde_json::to_value(&final_verdict)?,
        confidence,
        short_term_value: llm_out.short_term_value,
        long_term_value: llm_out.long_term_value,
        tension_resolution: llm_out.tension_resolution,
        mission_alignment_notes: llm_out.mission_alignment_notes,
        false_approve_vs_false_reject_weighting: llm_out.false_approve_vs_false_reject_weighting,
        proposed_bounty_spec: None, // review path never proposes bounties
        feedback: llm_out.revise_feedback,
        similar_past_decisions: similar
            .iter()
            .map(|d| crate::decision::PrecedentRef {
                decision_id: d.decision_id,
                summary: d.mission_alignment_notes.chars().take(200).collect(),
                verdict: d.verdict.to_string(),
                outcome: None,
                similarity_score: 0.0,
            })
            .collect(),
        decided_at: Utc::now(),
        prompt_version: agent.prompt_version.clone(),
        model_version: agent.llm.model_version(),
    };

    // Escalated reviews scrub feedback — council redrafts.
    if matches!(final_verdict, ReviewVerdict::Escalate) {
        decision.feedback = None;
    }

    // 9. Write to event log
    if let Err(e) = agent.event_log.record_decision(&decision).await {
        warn!(
            decision_id = %decision.decision_id,
            error = %e,
            "review: event log write failed; decision still returned"
        );
    }

    info!(
        %bounty_id,
        decision_id = %decision.decision_id,
        verdict = ?final_verdict,
        confidence = decision.confidence.0,
        "review: decision made"
    );

    Ok(decision)
}

fn render_review_input_block(request: &ReviewRequest) -> String {
    use std::fmt::Write;
    let mut b = String::with_capacity(4096);

    let _ = writeln!(b, "## Completed bounty under review");
    let _ = writeln!(b, "**Bounty ID:** {}", request.bounty_id);
    let _ = writeln!(b, "**Title:** {}", request.bounty_title);
    let _ = writeln!(
        b,
        "**Category:** {} (type_id={})",
        request.bounty_category, request.bounty_contribution_type_id
    );
    let _ = writeln!(b, "**Revision count:** {}", request.revision_count);
    let _ = writeln!(
        b,
        "\n**Original description:**\n```\n{}\n```",
        request.bounty_description
    );

    let _ = writeln!(b, "\n## QA bot evidence (mechanical verification)");
    let _ = writeln!(
        b,
        "```json\n{}\n```",
        serde_json::to_string_pretty(&request.qa_evidence).unwrap_or_default()
    );

    let _ = writeln!(b, "\n## Worker proof of completion");
    let _ = writeln!(
        b,
        "```json\n{}\n```",
        serde_json::to_string_pretty(&request.proof).unwrap_or_default()
    );

    let _ = writeln!(
        b,
        "\n**Reminder:** QA bot judges mechanical checks. You judge whether \
         this completed work advances the mission. Do not re-run QA's checks — \
         your verdict layers on top."
    );

    b
}

fn parse_review_output(raw: &str) -> Result<ReviewLlmOutput> {
    let trimmed = strip_code_fences(raw.trim());
    serde_json::from_str(trimmed).map_err(|e| {
        OracleError::InvalidDecision(format!(
            "review LLM output not valid ReviewLlmOutput JSON: {} (raw: {})",
            e,
            trimmed.chars().take(500).collect::<String>()
        ))
    })
}

fn strip_code_fences(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json") {
        rest.trim_end_matches("```").trim()
    } else if let Some(rest) = s.strip_prefix("```") {
        rest.trim_end_matches("```").trim()
    } else {
        s
    }
}

fn validate_required_fields(out: &ReviewLlmOutput) -> Result<()> {
    for (name, v) in [
        ("short_term_value", &out.short_term_value),
        ("long_term_value", &out.long_term_value),
        ("tension_resolution", &out.tension_resolution),
        ("mission_alignment_notes", &out.mission_alignment_notes),
        (
            "false_approve_vs_false_reject_weighting",
            &out.false_approve_vs_false_reject_weighting,
        ),
    ] {
        if v.trim().is_empty() {
            return Err(OracleError::InvalidDecision(format!(
                "required field '{}' is empty",
                name
            )));
        }
    }

    // Generic/empty `false_approve_vs_false_reject_weighting` fails validation
    // per constitutional prompt §5. Heuristic: too short to be a real paragraph.
    if out.false_approve_vs_false_reject_weighting.trim().len() < 40 {
        return Err(OracleError::InvalidDecision(
            "false_approve_vs_false_reject_weighting is too short to be substantive \
             (under 40 chars); generic text fails validation per constitutional §5"
                .into(),
        ));
    }

    // Revise verdicts MUST carry revise_feedback.
    if matches!(out.verdict, ReviewVerdict::Revise)
        && out
            .revise_feedback
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(OracleError::InvalidDecision(
            "verdict=revise requires non-empty revise_feedback".into(),
        ));
    }

    // Confidence must be a valid probability.
    if !(0.0..=1.0).contains(&out.confidence) || out.confidence.is_nan() {
        return Err(OracleError::InvalidDecision(format!(
            "confidence {} is outside [0.0, 1.0]",
            out.confidence
        )));
    }

    // Quality score adjustment bounded to [-10, 10].
    if !(-10..=10).contains(&out.quality_score_adjustment) {
        return Err(OracleError::InvalidDecision(format!(
            "quality_score_adjustment {} is outside [-10, 10]",
            out.quality_score_adjustment
        )));
    }

    Ok(())
}

/// Apply confidence guard. Approve triggers on-chain settlement so confidence
/// must be at or above [`Thresholds::review_self_auth_confidence`] (0.85).
/// Reject/revise do not require high confidence — they are conservative and
/// recoverable via dispute / re-submission.
///
/// Budget enforcement (daily approval fraction) is layered by the daemon loop
/// against live daily-spent tracking; this function only does per-decision
/// guards.
fn apply_review_guards(
    agent: &OracleAgent,
    out: &ReviewLlmOutput,
) -> Result<(ReviewVerdict, Confidence)> {
    let confidence = Confidence::new(out.confidence);
    let t = &agent.thresholds;

    // Already an escalate → pass through.
    if matches!(out.verdict, ReviewVerdict::Escalate) {
        return Ok((ReviewVerdict::Escalate, confidence));
    }

    // Confidence gate applies only to approve (the spend-moving verdict).
    if matches!(out.verdict, ReviewVerdict::Approve)
        && !confidence.at_least(t.review_self_auth_confidence)
    {
        info!(
            confidence = confidence.0,
            threshold = t.review_self_auth_confidence,
            "review: approve confidence below threshold → escalate"
        );
        return Ok((ReviewVerdict::Escalate, confidence));
    }

    Ok((out.verdict.clone(), confidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::StubLlmClient;

    fn sample_request() -> ReviewRequest {
        ReviewRequest {
            bounty_id: Uuid::new_v4(),
            bounty_title: "Add rate-limit headers to settlement endpoint".into(),
            bounty_description:
                "Emit X-RateLimit-Limit and X-RateLimit-Remaining on /settle responses.".into(),
            bounty_category: "infrastructure".into(),
            bounty_contribution_type_id: 7,
            qa_evidence: serde_json::json!({
                "tests_passed": true,
                "lint_clean": true,
                "coverage": 0.92
            }),
            proof: serde_json::json!({
                "commit": "abc123",
                "diff_summary": "+40 -0 in amos-relay/src/settle.rs"
            }),
            revision_count: 0,
        }
    }

    fn llm_output_approve_confident() -> &'static str {
        r#"{
  "verdict": "approve",
  "confidence": 0.90,
  "short_term_value": "Ships the requested headers; improves client back-off behavior.",
  "long_term_value": "Reinforces substrate reliability claims in the strategy doc.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Infrastructure hardening, within scope, cleanly delivered.",
  "false_approve_vs_false_reject_weighting": "Low stakes: bounty is small and deliverable is mechanically verifiable. False-approve risk is minimal given QA passed and scope is bounded; false-reject would unfairly penalize clean work.",
  "revise_feedback": null,
  "quality_score_adjustment": 2
}"#
    }

    fn llm_output_approve_low_confidence() -> &'static str {
        r#"{
  "verdict": "approve",
  "confidence": 0.70,
  "short_term_value": "Could be fine.",
  "long_term_value": "Ambiguous long-term.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Plausible but uncertain mission alignment.",
  "false_approve_vs_false_reject_weighting": "Approval moves tokens immediately, so I am being cautious; QA passed but I lack high confidence in mission alignment here.",
  "revise_feedback": null,
  "quality_score_adjustment": 0
}"#
    }

    fn llm_output_revise_missing_feedback() -> &'static str {
        r#"{
  "verdict": "revise",
  "confidence": 0.80,
  "short_term_value": "Partially delivered.",
  "long_term_value": "Would be aligned with fix.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Close but not ready.",
  "false_approve_vs_false_reject_weighting": "Revise is the softest nonterminal verdict; low risk of either false-approve or false-reject harm.",
  "revise_feedback": null,
  "quality_score_adjustment": -1
}"#
    }

    fn llm_output_empty_weighting() -> &'static str {
        r#"{
  "verdict": "reject",
  "confidence": 0.90,
  "short_term_value": "None.",
  "long_term_value": "None.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Does not advance mission.",
  "false_approve_vs_false_reject_weighting": "n/a",
  "revise_feedback": null,
  "quality_score_adjustment": -5
}"#
    }

    fn llm_output_reject_confident_low() -> &'static str {
        // Rejection with low confidence should NOT escalate (no spend on reject).
        r#"{
  "verdict": "reject",
  "confidence": 0.55,
  "short_term_value": "Not value-add.",
  "long_term_value": "Not mission-aligned.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Scope exceeded original bounty without authorization.",
  "false_approve_vs_false_reject_weighting": "False-reject here is recoverable via dispute; false-approve would lock treasury against work we do not want. Leaning reject is the conservative move.",
  "revise_feedback": null,
  "quality_score_adjustment": -3
}"#
    }

    async fn agent_with_canned_llm(canned: &str, prompt_version: &str) -> OracleAgent {
        use crate::metrics::MetricsProvider;
        use crate::mission::{MissionSnapshot, MissionSource};
        use crate::precedent::EventLog;
        use crate::registry::AmosContributionRegistry;
        use async_trait::async_trait;
        use std::sync::Arc;

        struct StubMission;
        #[async_trait]
        impl MissionSource for StubMission {
            async fn current(&self) -> Result<MissionSnapshot> {
                Ok(MissionSnapshot {
                    version: "stub".into(),
                    constitutional_provisions: "You are the AMOS Oracle.".into(),
                    strategic_thesis: String::new(),
                    operational_context: String::new(),
                })
            }
            async fn at_version(&self, _v: &str) -> Result<MissionSnapshot> {
                self.current().await
            }
        }

        struct StubMetrics;
        #[async_trait]
        impl MetricsProvider for StubMetrics {
            async fn snapshot(&self) -> Result<crate::metrics::RelaySnapshot> {
                Err(OracleError::MetricsProvider("stubbed".into()))
            }
        }

        struct StubLog;
        #[async_trait]
        impl EventLog for StubLog {
            async fn record_decision(&self, _d: &Decision) -> Result<()> {
                Ok(())
            }
            async fn record_outcome(&self, _id: Uuid, _o: crate::precedent::Outcome) -> Result<()> {
                Ok(())
            }
            async fn similar_decisions(&self, _q: &str, _n: usize) -> Result<Vec<Decision>> {
                Ok(vec![])
            }
            async fn decision_by_id(&self, _id: Uuid) -> Result<Option<Decision>> {
                Ok(None)
            }
        }

        OracleAgent::builder()
            .mission(Arc::new(StubMission))
            .metrics(Arc::new(StubMetrics))
            .registry(Arc::new(AmosContributionRegistry::new()))
            .event_log(Arc::new(StubLog))
            .llm(Arc::new(StubLlmClient::new(canned)))
            .prompt_version(prompt_version)
            .model_version("stub")
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn happy_path_approve() {
        let agent = agent_with_canned_llm(llm_output_approve_confident(), "v1").await;
        let d = evaluate(&agent, sample_request()).await.unwrap();
        assert_eq!(d.path, DecisionPath::Review);
        let v: ReviewVerdict = serde_json::from_value(d.verdict.clone()).unwrap();
        assert_eq!(v, ReviewVerdict::Approve);
        assert!(d.confidence.at_least(0.85));
        assert!(!d.false_approve_vs_false_reject_weighting.is_empty());
        assert!(d.proposed_bounty_spec.is_none());
    }

    #[tokio::test]
    async fn low_confidence_approve_rewrites_to_escalate() {
        let agent = agent_with_canned_llm(llm_output_approve_low_confidence(), "v1").await;
        let d = evaluate(&agent, sample_request()).await.unwrap();
        let v: ReviewVerdict = serde_json::from_value(d.verdict.clone()).unwrap();
        assert_eq!(v, ReviewVerdict::Escalate);
        assert!(d.feedback.is_none());
    }

    #[tokio::test]
    async fn revise_without_feedback_is_rejected() {
        let agent = agent_with_canned_llm(llm_output_revise_missing_feedback(), "v1").await;
        let err = evaluate(&agent, sample_request()).await.unwrap_err();
        assert!(matches!(err, OracleError::InvalidDecision(_)));
    }

    #[tokio::test]
    async fn empty_weighting_is_rejected() {
        let agent = agent_with_canned_llm(llm_output_empty_weighting(), "v1").await;
        let err = evaluate(&agent, sample_request()).await.unwrap_err();
        assert!(matches!(err, OracleError::InvalidDecision(_)));
    }

    #[tokio::test]
    async fn reject_at_low_confidence_does_not_escalate() {
        // Approval is the spend-moving verdict. Reject/revise are conservative
        // and do not require high confidence.
        let agent = agent_with_canned_llm(llm_output_reject_confident_low(), "v1").await;
        let d = evaluate(&agent, sample_request()).await.unwrap();
        let v: ReviewVerdict = serde_json::from_value(d.verdict.clone()).unwrap();
        assert_eq!(v, ReviewVerdict::Reject);
    }
}
