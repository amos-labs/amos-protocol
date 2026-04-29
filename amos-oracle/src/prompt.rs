//! Shared prompt assembly for intake + review paths.
//!
//! Oracle has **one brain**, two functions. The constitutional prompt is
//! the same for both paths; the user message differs only in (a) the
//! input shape being evaluated and (b) the output schema being requested.
//! Mission snapshot, metrics snapshot, and precedent retrieval are
//! path-agnostic — a review-path decision sees relevant intake precedents
//! and vice versa.
//!
//! The top-level [`assemble`] function composes: mission context, metrics,
//! precedent, the specific input block, and the path-specific output
//! schema instructions. Callers supply only the input-rendering closure
//! and the output-schema string.

use crate::decision::Decision;
use crate::metrics::RelaySnapshot;
use crate::mission::MissionSnapshot;

/// Build the full user message for an Oracle LLM call. The system prompt
/// (from the constitutional doc) is passed separately; this is only the
/// user-turn payload.
pub fn assemble(
    mission: &MissionSnapshot,
    metrics: Option<&RelaySnapshot>,
    precedent: &[Decision],
    input_block: &str,
    output_schema: &str,
) -> String {
    use std::fmt::Write;
    let mut msg = String::with_capacity(8192);

    // 1. Task framing — mode-neutral; the output_schema reveals the mode.
    let _ = writeln!(
        msg,
        "## Task\n\
         Evaluate the input below per your constitutional prompt. Produce \
         output in the exact JSON schema specified — no surrounding prose, \
         no markdown fences."
    );

    // 2. Operational context refresher (not the full thesis — just the
    //    version identifier so Oracle knows what state it's reasoning under).
    let _ = writeln!(
        msg,
        "\n## Mission context\n\
         - Mission version: {}",
        mission.version
    );

    // 3. Relay state (external signal — load-bearing per constitutional
    //    prompt §4; weight decisions harder toward escalate if absent).
    if let Some(m) = metrics {
        let _ = writeln!(msg, "\n## Relay state (past 7 days)");
        let _ = writeln!(
            msg,
            "- Commercial volume: {} atomic AMOS",
            m.commercial_volume_7d
        );
        let _ = writeln!(
            msg,
            "- System emission: {} atomic AMOS",
            m.system_emission_7d
        );
        let _ = writeln!(msg, "- Bounties posted: {}", m.bounties_posted_7d);
        let _ = writeln!(msg, "- Bounties settled: {}", m.bounties_settled_7d);
        let _ = writeln!(
            msg,
            "- Daily emission remaining: {} points",
            m.daily_emission_remaining_points
        );
        let _ = writeln!(msg, "- Active agents: {}", m.active_agents_7d);
        if m.commercial_volume_7d == 0 {
            let _ = writeln!(
                msg,
                "\n**External signal warning:** commercial volume is zero over the 7-day window. \
                 Per constitutional §4, weight decisions harder toward escalation and treasury preservation."
            );
        }
    } else {
        let _ = writeln!(
            msg,
            "\n## Relay state\n\
             (Metrics snapshot unavailable. Treat this as equivalent to zero commercial signal: \
             weight decisions harder toward escalate per constitutional §4.)"
        );
    }

    // 4. Precedent — unitary corpus per constitutional §8, both paths.
    if !precedent.is_empty() {
        let _ = writeln!(
            msg,
            "\n## Similar past decisions (precedent, unitary across paths)"
        );
        for (i, d) in precedent.iter().enumerate() {
            let _ = writeln!(
                msg,
                "{}. [{}] [{}] conf {:.2} — {}",
                i + 1,
                serde_json::to_string(&d.path).unwrap_or_default(),
                d.verdict,
                d.confidence.0,
                d.mission_alignment_notes
                    .chars()
                    .take(160)
                    .collect::<String>()
            );
        }
    }

    // 5. Path-specific input block.
    let _ = writeln!(msg, "\n{}", input_block);

    // 6. Path-specific output schema.
    let _ = writeln!(msg, "\n## Output schema (strict JSON)\n{}", output_schema);

    msg
}

/// Intake-path JSON schema that Oracle must conform to.
pub const INTAKE_SCHEMA: &str = r#"{
  "verdict": "commission" | "reject" | "refine" | "escalate",
  "confidence": <number in [0.0, 1.0]>,
  "short_term_value": "<1 paragraph>",
  "long_term_value": "<1 paragraph>",
  "tension_resolution": "<1 paragraph or 'no tension'>",
  "mission_alignment_notes": "<1-2 paragraphs>",
  "proposed_bounty_spec": {
    "title": "<string>",
    "description": "<string>",
    "category": "<one of the registered contribution types>",
    "required_capabilities": ["<string>", ...],
    "reward_points": <u64 — your judgment, not an auto-pointer's>,
    "reasoning_for_points": "<string>",
    "deadline_days": <u32>,
    "acceptance_criteria": [
      "<one assertion per array element. Concrete + checkable: 'GET /api/v1/bounties responds with X-Request-ID header on success and error responses', not 'request IDs work'. Workers read this when planning; QA bot uses it as the gate; you read it back at review.>",
      ...
    ] | null,
    "test_command": "<exact shell command the QA bot runs from repo root. Exits 0 iff acceptance_criteria pass. e.g. 'cargo test --test x_request_id_integration -- --include-ignored'. REQUIRED for code-class bounties (categories: infrastructure, research). NULL is acceptable for non-code bounties (docs, content).>" | null
  } | null,
  "refine_feedback": "<string>" | null
}"#;

/// Review-path JSON schema.
pub const REVIEW_SCHEMA: &str = r#"{
  "verdict": "approve" | "reject" | "revise" | "escalate",
  "confidence": <number in [0.0, 1.0]>,
  "short_term_value": "<1 paragraph>",
  "long_term_value": "<1 paragraph>",
  "tension_resolution": "<1 paragraph or 'no tension'>",
  "mission_alignment_notes": "<1-2 paragraphs>",
  "false_approve_vs_false_reject_weighting": "<1 paragraph — REQUIRED, not optional. Generic text fails validation.>",
  "revise_feedback": "<string>" | null,
  "quality_score_adjustment": <integer in [-10, 10]>
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> MissionSnapshot {
        MissionSnapshot {
            version: "v1-test".into(),
            constitutional_provisions: String::new(),
            strategic_thesis: String::new(),
            operational_context: String::new(),
        }
    }

    #[test]
    fn no_metrics_produces_explicit_zero_signal_note() {
        let msg = assemble(&snap(), None, &[], "## Input", INTAKE_SCHEMA);
        assert!(msg.contains("equivalent to zero commercial signal"));
        assert!(msg.contains("weight decisions harder toward escalate"));
    }

    #[test]
    fn zero_commercial_volume_triggers_warning() {
        let m = RelaySnapshot {
            taken_at: chrono::Utc::now(),
            daily_emission_remaining_points: 1000,
            daily_pool_points_distributed: 0,
            growth_pool_cap_bps: 2000,
            bounties_posted_7d: 2,
            bounties_claimed_7d: 2,
            bounties_settled_7d: 1,
            bounties_rejected_7d: 0,
            commercial_volume_7d: 0,
            system_emission_7d: 5000,
            active_agents_7d: 3,
            avg_quality_score_7d: 85.0,
            category_counts_7d: Default::default(),
        };
        let msg = assemble(&snap(), Some(&m), &[], "## Input", INTAKE_SCHEMA);
        assert!(msg.contains("External signal warning"));
        assert!(msg.contains("commercial volume is zero"));
    }

    #[test]
    fn nonzero_commercial_volume_no_warning() {
        let m = RelaySnapshot {
            taken_at: chrono::Utc::now(),
            daily_emission_remaining_points: 1000,
            daily_pool_points_distributed: 0,
            growth_pool_cap_bps: 2000,
            bounties_posted_7d: 2,
            bounties_claimed_7d: 2,
            bounties_settled_7d: 1,
            bounties_rejected_7d: 0,
            commercial_volume_7d: 50_000,
            system_emission_7d: 5000,
            active_agents_7d: 3,
            avg_quality_score_7d: 85.0,
            category_counts_7d: Default::default(),
        };
        let msg = assemble(&snap(), Some(&m), &[], "## Input", INTAKE_SCHEMA);
        assert!(!msg.contains("External signal warning"));
    }

    #[test]
    fn intake_and_review_schemas_differ_only_in_verdict_and_spec_fields() {
        assert!(INTAKE_SCHEMA.contains("commission"));
        assert!(INTAKE_SCHEMA.contains("proposed_bounty_spec"));
        assert!(REVIEW_SCHEMA.contains("approve"));
        assert!(REVIEW_SCHEMA.contains("false_approve_vs_false_reject_weighting"));
        assert!(!INTAKE_SCHEMA.contains("approve"));
        assert!(!REVIEW_SCHEMA.contains("proposed_bounty_spec"));
    }
}
