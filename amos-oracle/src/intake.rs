//! Intake path: evaluate a submission as a potential system bounty.
//!
//! Flow:
//! 1. Load mission snapshot (versioned)
//! 2. Load metrics snapshot (stubbed OK — degrades gracefully to "no metrics")
//! 3. Retrieve precedent (N=5 similar past intake decisions)
//! 4. Assemble prompt from mission + metrics + precedent + submission
//! 5. Call LLM with structured-output schema
//! 6. Parse response into `Decision`
//! 7. Apply guards (confidence, per-bounty ceiling, daily budget)
//! 8. If any guard trips: rewrite verdict to `Escalate`
//! 9. Write to event log
//! 10. Return decision

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::agent::OracleAgent;
use crate::decision::{Confidence, Decision, DecisionPath, IntakeVerdict, ProposedBountySpec};
use crate::prompt::{self, INTAKE_SCHEMA};
use crate::{OracleError, Result};

/// A submission arriving at the intake path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeSubmission {
    pub submission_id: Uuid,
    pub title: String,
    pub body: String,
    pub submitter: String,
    pub parent_submission_id: Option<Uuid>,
    pub suggested_category: Option<String>,
    pub suggested_capabilities: Vec<String>,
}

/// Raw LLM output shape for intake decisions. The LLM is prompted to emit
/// exactly this structure; we parse it then apply guards before turning it
/// into the canonical [`Decision`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeLlmOutput {
    pub verdict: IntakeVerdict,
    pub confidence: f64,
    pub short_term_value: String,
    pub long_term_value: String,
    pub tension_resolution: String,
    pub mission_alignment_notes: String,
    pub proposed_bounty_spec: Option<ProposedBountySpec>,
    pub refine_feedback: Option<String>,
}

pub async fn evaluate(agent: &OracleAgent, submission: IntakeSubmission) -> Result<Decision> {
    let submission_id = submission.submission_id;
    debug!(%submission_id, "intake: starting evaluation");

    // 1. Mission snapshot
    let mission = agent.mission.current().await?;

    // 2. Metrics snapshot — degrade gracefully if provider is stubbed
    let metrics = agent.metrics.snapshot().await.ok();
    if metrics.is_none() {
        warn!(%submission_id, "intake: metrics snapshot unavailable; proceeding without");
    }

    // 3. Precedent retrieval
    let precedent_query = format!("intake: {} | {}", submission.title, submission.body);
    let similar = agent
        .event_log
        .similar_decisions(&precedent_query, 5)
        .await
        .unwrap_or_default();

    // 4. Assemble prompt — shared assembly with intake-specific input block + schema.
    let system_prompt = mission.constitutional_provisions.clone();
    let input_block = render_intake_input_block(&submission);
    let user_message = prompt::assemble(
        &mission,
        metrics.as_ref(),
        &similar,
        &input_block,
        INTAKE_SCHEMA,
    );

    // 5. LLM call
    let raw_response = agent.llm.complete(&system_prompt, &user_message).await?;
    debug!(
        %submission_id,
        response_len = raw_response.len(),
        "intake: LLM responded"
    );

    // 6. Parse
    let llm_out: IntakeLlmOutput = parse_intake_output(&raw_response)?;
    validate_required_fields(&llm_out)?;

    // 7. Apply guards — may rewrite verdict to Escalate
    let (final_verdict, confidence) = apply_intake_guards(agent, &llm_out)?;

    // 8. Build canonical Decision
    let mut decision = Decision {
        decision_id: Uuid::new_v4(),
        path: DecisionPath::Intake,
        verdict: serde_json::to_value(&final_verdict)?,
        confidence,
        short_term_value: llm_out.short_term_value,
        long_term_value: llm_out.long_term_value,
        tension_resolution: llm_out.tension_resolution,
        mission_alignment_notes: llm_out.mission_alignment_notes,
        false_approve_vs_false_reject_weighting: String::new(), // review-only field
        proposed_bounty_spec: llm_out.proposed_bounty_spec,
        feedback: llm_out.refine_feedback,
        similar_past_decisions: similar
            .iter()
            .map(|d| crate::decision::PrecedentRef {
                decision_id: d.decision_id,
                summary: d.mission_alignment_notes.chars().take(200).collect(),
                verdict: d.verdict.to_string(),
                outcome: None,
                similarity_score: 0.0, // TODO: populate from real retrieval
            })
            .collect(),
        decided_at: Utc::now(),
        prompt_version: agent.prompt_version.clone(),
        model_version: agent.llm.model_version(),
    };

    // Escalate decisions preserve the proposed_bounty_spec (when Oracle
    // emitted one) as a council-reference draft. The LLM's suggestion is
    // useful input for council; carrying it forward is not the same as
    // Oracle authorizing it. Council may accept the draft as-is by passing
    // verdict=commission to resolve_escalation, or supply its own
    // proposed_bounty_spec override at resolve time.

    // 9. Write to event log (non-fatal if it fails — decision still returned,
    //    but log + alert for drift analysis).
    if let Err(e) = agent.event_log.record_decision(&decision).await {
        warn!(
            decision_id = %decision.decision_id,
            error = %e,
            "intake: event log write failed; decision still returned"
        );
    }

    info!(
        %submission_id,
        decision_id = %decision.decision_id,
        verdict = ?final_verdict,
        confidence = decision.confidence.0,
        "intake: decision made"
    );

    Ok(decision)
}

/// Render the intake-specific input section. The shared `prompt::assemble`
/// wraps this with mission context, metrics, precedent, and the output schema.
fn render_intake_input_block(submission: &IntakeSubmission) -> String {
    use std::fmt::Write;
    let mut b = String::with_capacity(2048);

    let _ = writeln!(b, "## Submission");
    let _ = writeln!(b, "**Submitter:** {}", submission.submitter);
    let _ = writeln!(b, "**Title:** {}", submission.title);
    if let Some(parent) = submission.parent_submission_id {
        let _ = writeln!(
            b,
            "**Re-submission of:** {} (consider prior refine_feedback)",
            parent
        );
    }
    if let Some(cat) = &submission.suggested_category {
        let _ = writeln!(b, "**Submitter-suggested category:** {}", cat);
    }
    if !submission.suggested_capabilities.is_empty() {
        let _ = writeln!(
            b,
            "**Submitter-suggested capabilities:** {}",
            submission.suggested_capabilities.join(", ")
        );
    }
    let _ = writeln!(b, "\n**Body:**\n```\n{}\n```", submission.body);

    b
}

fn parse_intake_output(raw: &str) -> Result<IntakeLlmOutput> {
    // Tolerate common LLM wrappers: code fences, leading/trailing prose.
    let trimmed = strip_code_fences(raw.trim());
    serde_json::from_str(trimmed).map_err(|e| {
        OracleError::InvalidDecision(format!(
            "intake LLM output not valid IntakeLlmOutput JSON: {} (raw: {})",
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

fn validate_required_fields(out: &IntakeLlmOutput) -> Result<()> {
    // Non-empty text fields.
    for (name, v) in [
        ("short_term_value", &out.short_term_value),
        ("long_term_value", &out.long_term_value),
        ("tension_resolution", &out.tension_resolution),
        ("mission_alignment_notes", &out.mission_alignment_notes),
    ] {
        if v.trim().is_empty() {
            return Err(OracleError::InvalidDecision(format!(
                "required field '{}' is empty",
                name
            )));
        }
    }

    // Commission verdicts MUST carry a proposed_bounty_spec.
    if matches!(out.verdict, IntakeVerdict::Commission) && out.proposed_bounty_spec.is_none() {
        return Err(OracleError::InvalidDecision(
            "verdict=commission requires proposed_bounty_spec".into(),
        ));
    }

    // Refine verdicts MUST carry refine_feedback.
    if matches!(out.verdict, IntakeVerdict::Refine)
        && out
            .refine_feedback
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(OracleError::InvalidDecision(
            "verdict=refine requires non-empty refine_feedback".into(),
        ));
    }

    // Confidence must be a valid probability.
    if !(0.0..=1.0).contains(&out.confidence) || out.confidence.is_nan() {
        return Err(OracleError::InvalidDecision(format!(
            "confidence {} is outside [0.0, 1.0]",
            out.confidence
        )));
    }

    Ok(())
}

/// Apply confidence / per-bounty-ceiling guards. Returns the final verdict
/// (possibly rewritten to Escalate) and the confidence value.
///
/// Budget enforcement (daily commissioning fraction) is **not** done here —
/// that needs a live daily-spent counter against on-chain / relay state.
/// The stub returns the LLM verdict for budget checks; the daemon loop will
/// layer budget enforcement on top using a separate tracker.
fn apply_intake_guards(
    agent: &OracleAgent,
    out: &IntakeLlmOutput,
) -> Result<(IntakeVerdict, Confidence)> {
    let confidence = Confidence::new(out.confidence);
    let t = &agent.thresholds;

    // Already an escalate from the LLM → pass through.
    if matches!(out.verdict, IntakeVerdict::Escalate) {
        return Ok((IntakeVerdict::Escalate, confidence));
    }

    // Confidence gate (applies only to commission — reject and refine are
    // conservative, don't require high confidence).
    if matches!(out.verdict, IntakeVerdict::Commission)
        && !confidence.at_least(t.intake_self_auth_confidence)
    {
        info!(
            confidence = confidence.0,
            threshold = t.intake_self_auth_confidence,
            "intake: commission confidence below threshold → escalate"
        );
        return Ok((IntakeVerdict::Escalate, confidence));
    }

    // Per-bounty points ceiling.
    if let Some(spec) = &out.proposed_bounty_spec {
        if matches!(out.verdict, IntakeVerdict::Commission)
            && spec.reward_points > t.intake_per_bounty_ceiling
        {
            info!(
                points = spec.reward_points,
                ceiling = t.intake_per_bounty_ceiling,
                "intake: proposed points above ceiling → escalate"
            );
            return Ok((IntakeVerdict::Escalate, confidence));
        }

        // Reasoning-substrate recursion guard (constitutional prompt §6).
        // Oracle may commission plumbing improvements to itself but may not
        // self-authorize changes to how it reasons. On-chain layer is the
        // authoritative floor (see OPS-ORACLE-ONCHAIN-GUARD-001); this is the
        // code-level mirror that trips before chain rejection.
        if matches!(out.verdict, IntakeVerdict::Commission)
            && is_reasoning_substrate_category(&spec.category)
        {
            info!(
                category = %spec.category,
                "intake: category is reasoning-substrate → escalate"
            );
            return Ok((IntakeVerdict::Escalate, confidence));
        }
    }

    Ok((out.verdict.clone(), confidence))
}

/// Categories the Oracle may never self-authorize against. Matches the
/// `forbidden_category_bitmap` intent of OPS-ORACLE-ONCHAIN-GUARD-001.
fn is_reasoning_substrate_category(category: &str) -> bool {
    matches!(
        category.trim().to_ascii_lowercase().as_str(),
        "oracle_substrate"
            | "oracle-substrate"
            | "constitutional"
            | "core_protocol"
            | "core-protocol"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::StubLlmClient;

    fn sample_submission() -> IntakeSubmission {
        IntakeSubmission {
            submission_id: Uuid::new_v4(),
            title: "Add rate-limit headers to settlement endpoint".into(),
            body: "The settlement endpoint currently does not emit X-RateLimit-* headers. \
                   This causes client back-off logic to miss timing info."
                .into(),
            submitter: "test-submitter".into(),
            parent_submission_id: None,
            suggested_category: Some("infrastructure".into()),
            suggested_capabilities: vec!["rust".into()],
        }
    }

    fn llm_output_commission_confident() -> &'static str {
        r#"{
  "verdict": "commission",
  "confidence": 0.90,
  "short_term_value": "Improves developer integration experience.",
  "long_term_value": "Aligns with substrate reliability claims in v2.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Infrastructure hardening, consistent with OPS track.",
  "proposed_bounty_spec": {
    "title": "Add rate-limit headers to settlement endpoint",
    "description": "Emit X-RateLimit-Limit and X-RateLimit-Remaining on /settle responses.",
    "category": "infrastructure",
    "required_capabilities": ["rust", "axum"],
    "reward_points": 200,
    "reasoning_for_points": "Small surface, clear deliverable, low risk.",
    "deadline_days": 3
  },
  "refine_feedback": null
}"#
    }

    fn llm_output_commission_low_confidence() -> &'static str {
        r#"{
  "verdict": "commission",
  "confidence": 0.55,
  "short_term_value": "Maybe useful.",
  "long_term_value": "Unclear long-term value.",
  "tension_resolution": "Short term benefit vs unclear alignment.",
  "mission_alignment_notes": "Plausible but uncertain.",
  "proposed_bounty_spec": {
    "title": "T", "description": "D", "category": "infrastructure",
    "required_capabilities": [], "reward_points": 200,
    "reasoning_for_points": "guess", "deadline_days": 3
  },
  "refine_feedback": null
}"#
    }

    fn llm_output_commission_over_ceiling() -> &'static str {
        r#"{
  "verdict": "commission",
  "confidence": 0.95,
  "short_term_value": "Valuable.",
  "long_term_value": "Mission-aligned.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Aligned and clear.",
  "proposed_bounty_spec": {
    "title": "T", "description": "D", "category": "infrastructure",
    "required_capabilities": [], "reward_points": 800,
    "reasoning_for_points": "big job", "deadline_days": 14
  },
  "refine_feedback": null
}"#
    }

    fn llm_output_refine_missing_feedback() -> &'static str {
        r#"{
  "verdict": "refine",
  "confidence": 0.70,
  "short_term_value": "Not yet clear.",
  "long_term_value": "Not yet clear.",
  "tension_resolution": "no tension",
  "mission_alignment_notes": "Needs tightening.",
  "proposed_bounty_spec": null,
  "refine_feedback": null
}"#
    }

    async fn agent_with_canned_llm(canned: &str, prompt_version: &str) -> OracleAgent {
        use crate::metrics::MetricsProvider;
        use crate::mission::{MissionSnapshot, MissionSource};
        use crate::precedent::EventLog;
        use crate::registry::AmosContributionRegistry;
        use async_trait::async_trait;
        use std::sync::Arc;

        // Minimal in-memory trait impls.
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
    async fn happy_path_commission() {
        let agent = agent_with_canned_llm(llm_output_commission_confident(), "v1").await;
        let d = evaluate(&agent, sample_submission()).await.unwrap();
        assert_eq!(d.path, DecisionPath::Intake);
        let v: IntakeVerdict = serde_json::from_value(d.verdict.clone()).unwrap();
        assert_eq!(v, IntakeVerdict::Commission);
        assert!(d.confidence.at_least(0.80));
        assert!(d.proposed_bounty_spec.is_some());
        assert!(!d.short_term_value.is_empty());
        assert!(!d.long_term_value.is_empty());
    }

    #[tokio::test]
    async fn low_confidence_rewrites_to_escalate() {
        let agent = agent_with_canned_llm(llm_output_commission_low_confidence(), "v1").await;
        let d = evaluate(&agent, sample_submission()).await.unwrap();
        let v: IntakeVerdict = serde_json::from_value(d.verdict.clone()).unwrap();
        assert_eq!(v, IntakeVerdict::Escalate);
        // Escalate decisions preserve the LLM's draft proposed_bounty_spec
        // as a council-reference. Council can accept or override at resolve.
        assert!(d.proposed_bounty_spec.is_some());
    }

    #[tokio::test]
    async fn over_ceiling_rewrites_to_escalate() {
        let agent = agent_with_canned_llm(llm_output_commission_over_ceiling(), "v1").await;
        let d = evaluate(&agent, sample_submission()).await.unwrap();
        let v: IntakeVerdict = serde_json::from_value(d.verdict.clone()).unwrap();
        assert_eq!(v, IntakeVerdict::Escalate);
    }

    #[tokio::test]
    async fn refine_without_feedback_is_rejected() {
        let agent = agent_with_canned_llm(llm_output_refine_missing_feedback(), "v1").await;
        let err = evaluate(&agent, sample_submission()).await.unwrap_err();
        assert!(matches!(err, OracleError::InvalidDecision(_)));
    }

    #[tokio::test]
    async fn strip_code_fences_handles_llm_markdown_wrappers() {
        let wrapped = "```json\n{\"verdict\":\"reject\",\"confidence\":0.9,\"short_term_value\":\"x\",\"long_term_value\":\"y\",\"tension_resolution\":\"no tension\",\"mission_alignment_notes\":\"z\",\"proposed_bounty_spec\":null,\"refine_feedback\":null}\n```";
        let out = parse_intake_output(wrapped).unwrap();
        assert_eq!(out.verdict, IntakeVerdict::Reject);
    }
}
