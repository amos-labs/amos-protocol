//! Automatic bounty pointing engine.
//!
//! Calculates point values for bounties based on complexity, strategic
//! importance, and estimated time-to-complete. Points determine the bounty's
//! share of the daily emission pool — higher points attract agents faster.
//!
//! The system is designed so META-001 (the autonomous network growth agent) can
//! generate bounties with auto-calculated point values that reflect the
//! protocol's actual priorities without human intervention.
//!
//! # Formula
//!
//! ```text
//! points = base_effort × importance_multiplier × specialization_bonus
//! ```
//!
//! Clamped to [MIN_POINTS, MAX_POINTS] range.

use tracing::debug;

/// Minimum auto-calculated points for any bounty.
const MIN_POINTS: u64 = 100;

/// Maximum auto-calculated points for any bounty.
///
/// **Pinned to the on-chain `MAX_BOUNTY_POINTS` constant in
/// `amos-solana/programs/amos-bounty/src/constants.rs`.** The on-chain
/// `distribution::submit_bounty_proof` rejects any `base_points > 2000`
/// with `InvalidBountyPoints (6004)`, so auto-pointing above this value
/// produces bounties where the advertised points never equal the
/// settleable points — a silent mismatch.
///
/// If the on-chain constant changes, update here and re-check the
/// `on_chain_pointing_cap_is_pinned` test below.
const MAX_POINTS: u64 = 2_000;

/// Input signals the pointing engine uses to calculate a score.
#[derive(Debug, Clone)]
pub struct PointingInput {
    /// Bounty title (used for pattern matching, e.g. "AMOS-SECURE-*")
    pub title: String,
    /// Bounty description (scope, acceptance criteria, dependencies)
    pub description: String,
    /// Category: infrastructure, research, growth, content
    pub category: String,
    /// Required capabilities/tools
    pub capabilities: Vec<String>,
    /// Days until deadline from creation
    pub deadline_days: f64,
}

/// Breakdown of the scoring factors (returned for transparency/logging).
#[derive(Debug, Clone)]
pub struct PointingBreakdown {
    /// Base effort score from complexity analysis (100–2000)
    pub effort_score: u64,
    /// Importance multiplier (1.0–3.0)
    pub importance_mult: f64,
    /// Specialization bonus multiplier (1.0–1.5)
    pub specialization_mult: f64,
    /// Time pressure factor (0.8–1.3)
    pub time_factor: f64,
    /// Final calculated points
    pub points: u64,
}

/// Calculate points for a bounty automatically.
pub fn calculate_points(input: &PointingInput) -> PointingBreakdown {
    let effort_score = score_effort(input);
    let importance_mult = score_importance(input);
    let specialization_mult = score_specialization(input);
    let time_factor = score_time_pressure(input);

    let raw = effort_score as f64 * importance_mult * specialization_mult * time_factor;
    let points = (raw as u64).clamp(MIN_POINTS, MAX_POINTS);

    let breakdown = PointingBreakdown {
        effort_score,
        importance_mult,
        specialization_mult,
        time_factor,
        points,
    };

    debug!(
        title = %input.title,
        effort = breakdown.effort_score,
        importance = format!("{:.2}", breakdown.importance_mult),
        specialization = format!("{:.2}", breakdown.specialization_mult),
        time = format!("{:.2}", breakdown.time_factor),
        points = breakdown.points,
        "Auto-pointed bounty"
    );

    breakdown
}

/// Score the effort/complexity of the bounty (100–2000).
///
/// Signals:
/// - Description length (proxy for scope — more acceptance criteria = more work)
/// - Number of required capabilities (more tools = more integration complexity)
/// - Presence of keywords indicating multi-phase or multi-deliverable work
fn score_effort(input: &PointingInput) -> u64 {
    let mut score: f64 = 200.0; // base

    // Description scope: longer descriptions with more acceptance criteria = more work
    let desc_len = input.description.len() as f64;
    // Logarithmic scaling: 100 chars → +0, 500 → +100, 2000 → +200, 10000 → +300
    score += (desc_len / 100.0).ln().max(0.0) * 65.0;

    // Capability count: each required tool adds integration complexity
    let cap_count = input.capabilities.len() as f64;
    score += cap_count * 50.0; // 3 tools → +150, 5 tools → +250

    // Keyword complexity signals
    let desc_lower = input.description.to_lowercase();

    // Multi-deliverable work
    let deliverable_count = desc_lower.matches("deliverable").count()
        + desc_lower.matches("phase ").count()
        + desc_lower.matches("step ").count();
    score += (deliverable_count as f64) * 80.0;

    // Integration/system work is harder than standalone
    if desc_lower.contains("on-chain") || desc_lower.contains("solana") {
        score += 200.0;
    }
    if desc_lower.contains("migration") || desc_lower.contains("migrate") {
        score += 150.0;
    }
    if desc_lower.contains("end-to-end") || desc_lower.contains("full lifecycle") {
        score += 120.0;
    }

    // Test/verification requirements add effort
    if desc_lower.contains("test suite") || desc_lower.contains("test coverage") {
        score += 100.0;
    }
    if desc_lower.contains("fuzz test") {
        score += 80.0;
    }

    score.min(2000.0) as u64
}

/// Score the strategic importance of the bounty (1.0–3.0 multiplier).
///
/// Signals:
/// - Category priority: security > revenue/infrastructure > research > growth > content
/// - Dependency position: genesis bounties and blockers rank higher
/// - Revenue impact: commercial architecture is the highest priority
fn score_importance(input: &PointingInput) -> f64 {
    let mut mult = 1.0_f64;
    let title_lower = input.title.to_lowercase();
    let desc_lower = input.description.to_lowercase();

    // Category base importance
    mult += match input.category.as_str() {
        "infrastructure" => 0.6,
        "research" => 0.4,
        "growth" => 0.2,
        "content" => 0.1,
        _ => 0.0,
    };

    // Security bounties: critical path for production readiness
    if title_lower.contains("secure") || title_lower.contains("security") {
        mult += 0.8;
    }

    // Revenue engine: commercial bounty architecture
    if desc_lower.contains("revenue engine")
        || desc_lower.contains("commercial bounty")
        || desc_lower.contains("protocol fee")
    {
        mult += 0.6;
    }

    // Genesis bounties (no dependencies) unlock everything downstream
    if !desc_lower.contains("depends on:") {
        mult += 0.3;
    }

    // Blockers: bounties that other work depends on
    if desc_lower.contains("critical") || desc_lower.contains("blocking") {
        mult += 0.2;
    }

    // SDK/framework work: distribution multiplier
    if title_lower.contains("sdk") || title_lower.contains("framework") {
        mult += 0.2;
    }

    mult.min(3.0)
}

/// Score specialization requirements (1.0–1.5 multiplier).
///
/// Rarer skills should command higher points to attract qualified agents.
fn score_specialization(input: &PointingInput) -> f64 {
    let mut mult = 1.0_f64;

    let specialized_tools = [
        "solana_development",
        "security_analysis",
        "mathematical_analysis",
        "database_migration",
        "infrastructure_config",
        "package_publishing",
    ];

    for cap in &input.capabilities {
        if specialized_tools.contains(&cap.as_str()) {
            mult += 0.08;
        }
    }

    mult.min(1.5)
}

/// Score time pressure (0.8–1.3 multiplier).
///
/// Tighter deadlines need higher points to attract agents quickly.
/// Very distant deadlines get a slight discount.
fn score_time_pressure(input: &PointingInput) -> f64 {
    match input.deadline_days {
        d if d <= 3.0 => 1.3,   // urgent: 3 days or less
        d if d <= 7.0 => 1.15,  // tight: within a week
        d if d <= 14.0 => 1.0,  // normal: two weeks
        d if d <= 30.0 => 0.95, // relaxed: a month
        _ => 0.9,               // distant: discount slightly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(
        title: &str,
        desc: &str,
        category: &str,
        caps: &[&str],
        days: f64,
    ) -> PointingInput {
        PointingInput {
            title: title.to_string(),
            description: desc.to_string(),
            category: category.to_string(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            deadline_days: days,
        }
    }

    #[test]
    fn test_security_bounty_scores_high() {
        let input = make_input(
            "AMOS-SECURE-001: Compute Budget",
            "Add ComputeBudgetProgram instructions to all Solana transactions. Test suite required.",
            "infrastructure",
            &["code_execution", "solana_development", "file_write"],
            14.0,
        );
        let b = calculate_points(&input);
        assert!(
            b.points >= 1000,
            "Security bounty should score >= 1000, got {}",
            b.points
        );
        assert!(
            b.importance_mult >= 2.0,
            "Security importance should be >= 2.0"
        );
    }

    #[test]
    fn test_simple_content_bounty_scores_low() {
        let input = make_input(
            "AMOS-GROWTH-001: Social Media Content",
            "Produce weekly content across social media. 4 posts per week minimum.",
            "growth",
            &["content_generation"],
            30.0,
        );
        let b = calculate_points(&input);
        assert!(
            b.points <= 600,
            "Simple content bounty should score <= 600, got {}",
            b.points
        );
    }

    #[test]
    fn test_revenue_bounty_scores_highest() {
        let input = make_input(
            "AMOS-INFRA-006: Commercial Bounty Architecture",
            "CRITICAL: This is the revenue engine. Commercial bounty lifecycle on-chain. \
             Full lifecycle end-to-end. Protocol fee extraction. Test suite. Deliverable: \
             Solana program. Depends on: INFRA-001.",
            "infrastructure",
            &["code_execution", "solana_development", "file_write"],
            14.0,
        );
        let b = calculate_points(&input);
        assert!(
            b.points >= 2000,
            "Revenue bounty should score >= 2000, got {}",
            b.points
        );
    }

    #[test]
    fn test_genesis_bounty_beats_dependent() {
        let genesis = make_input(
            "AMOS-RESEARCH-001: Simulation Framework",
            "Build simulation framework with 6 agent population models. Test suite required.",
            "research",
            &["code_execution", "mathematical_analysis", "file_write"],
            14.0,
        );
        let dependent = make_input(
            "AMOS-RESEARCH-002: Agent Behavior Taxonomy",
            "Classify agent strategies. Depends on: RESEARCH-001-P2.",
            "research",
            &["code_execution", "mathematical_analysis"],
            14.0,
        );
        let g = calculate_points(&genesis);
        let d = calculate_points(&dependent);
        assert!(
            g.points > d.points,
            "Genesis bounty ({}) should beat dependent ({})",
            g.points,
            d.points
        );
    }

    #[test]
    fn test_urgent_deadline_increases_points() {
        let normal = make_input(
            "Fix bug",
            "Fix the login bug.",
            "infrastructure",
            &["code_execution"],
            14.0,
        );
        let urgent = make_input(
            "Fix bug",
            "Fix the login bug.",
            "infrastructure",
            &["code_execution"],
            2.0,
        );
        let n = calculate_points(&normal);
        let u = calculate_points(&urgent);
        assert!(
            u.points > n.points,
            "Urgent ({}) should beat normal ({})",
            u.points,
            n.points
        );
    }

    #[test]
    fn test_points_within_bounds() {
        // Minimal bounty
        let minimal = make_input("X", "x", "content", &[], 90.0);
        let m = calculate_points(&minimal);
        assert!(m.points >= MIN_POINTS);

        // Maximal bounty
        let maximal = make_input(
            "AMOS-SECURE-999: Security Critical Migration",
            &"On-chain Solana migration with test suite and fuzz testing. \
              Critical blocking revenue engine commercial bounty. \
              Deliverable: Phase 1 end-to-end. Deliverable: Phase 2 full lifecycle. \
              Deliverable: Phase 3 deployment. Database migration required. "
                .repeat(10),
            "infrastructure",
            &[
                "solana_development",
                "security_analysis",
                "database_migration",
                "mathematical_analysis",
                "code_execution",
                "file_write",
            ],
            1.0,
        );
        let mx = calculate_points(&maximal);
        assert!(mx.points <= MAX_POINTS);
    }

    #[test]
    fn test_specialization_bonus() {
        let general = make_input(
            "Task",
            "Do the thing.",
            "infrastructure",
            &["code_execution", "file_write"],
            14.0,
        );
        let specialized = make_input(
            "Task",
            "Do the thing.",
            "infrastructure",
            &[
                "solana_development",
                "security_analysis",
                "database_migration",
            ],
            14.0,
        );
        let g = calculate_points(&general);
        let s = calculate_points(&specialized);
        assert!(
            s.specialization_mult > g.specialization_mult,
            "Specialized ({:.2}) should beat general ({:.2})",
            s.specialization_mult,
            g.specialization_mult
        );
    }

    #[test]
    fn test_breakdown_fields_populated() {
        let input = make_input(
            "Test bounty",
            "Some work to do.",
            "infrastructure",
            &["code_execution"],
            14.0,
        );
        let b = calculate_points(&input);
        assert!(b.effort_score > 0);
        assert!(b.importance_mult >= 1.0);
        assert!(b.specialization_mult >= 1.0);
        assert!(b.time_factor > 0.0);
        assert!(b.points > 0);
    }

    #[test]
    fn on_chain_pointing_cap_is_pinned() {
        // This test is the coupling between the off-chain pointing engine
        // and the on-chain `amos-bounty` program. If the on-chain
        // `MAX_BOUNTY_POINTS` constant changes, update `MAX_POINTS` at the
        // top of this file AND this test, or auto-pointed bounties will
        // once again advertise settle-incompatible values.
        //
        // On-chain source: amos-solana/programs/amos-bounty/src/constants.rs
        //   pub const MAX_BOUNTY_POINTS: u16 = 2000;
        //   pub const TRUST_LEVEL_MAX_POINTS: [u16; 5] = [100, 200, 500, 1000, 2000];
        assert_eq!(
            MAX_POINTS, 2000,
            "MAX_POINTS must match on-chain MAX_BOUNTY_POINTS (2000). \
             If the on-chain constant changed, update both and re-run tests."
        );
    }

    #[test]
    fn extreme_bounty_clamps_at_on_chain_cap() {
        // An extraordinarily large bounty (many capabilities, long
        // description, security + revenue signals) must still be clamped
        // to at most MAX_POINTS so it can actually settle on-chain.
        let big_desc = "AMOS-SECURE-999: Comprehensive protocol audit with full threat model, \
                       security testing, formal verification, fuzzing, vulnerability disclosure, \
                       remediation plan, on-chain settlement logic review, dispute mechanism \
                       hardening, and stress testing across all edge cases. Revenue-critical. \
                       Genesis bounty. Affects every settlement path in the protocol. Multi-phase \
                       delivery required with dependency graph and test suite coverage reports."
            .repeat(5);
        let input = make_input(
            "AMOS-SECURE-999: Full Protocol Audit",
            &big_desc,
            "infrastructure",
            &[
                "solana_development",
                "security_analysis",
                "cryptography",
                "formal_verification",
                "file_write",
                "code_execution",
            ],
            1.0, // Very urgent → time_factor max
        );
        let b = calculate_points(&input);
        assert!(
            b.points <= MAX_POINTS,
            "Extreme bounty exceeded MAX_POINTS cap: got {}, cap {}",
            b.points,
            MAX_POINTS
        );
        // And it should actually hit the cap, not accidentally come in under
        // (otherwise this test isn't exercising the clamp).
        assert_eq!(
            b.points, MAX_POINTS,
            "Extreme bounty should hit the clamp exactly, got {}",
            b.points
        );
    }
}
