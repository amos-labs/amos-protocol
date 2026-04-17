//! # External Agent Trust System
//!
//! On-chain trust levels for the External Agent Protocol (EAP).
//!
//! Trust levels (1-5):
//!   1. Newcomer  — max 100 pts/bounty, 3 bounties/day
//!   2. Verified  — max 200 pts, 5/day  (≥ 3 completions, ≥ 55% reputation)
//!   3. Trusted   — max 500 pts, 10/day (≥ 10 completions, ≥ 65% reputation)
//!   4. Expert    — max 1000 pts, 15/day (≥ 25 completions, ≥ 75% reputation)
//!   5. Elite     — max 2000 pts, 25/day (≥ 50 completions, ≥ 85% reputation)
//!
//! All thresholds are enforced on-chain. The platform can request upgrades
//! but cannot bypass the requirements.

use super::economics::*;
use crate::error::{AmosError, Result};
use serde::{Deserialize, Serialize};

/// Trust level constants mirroring on-chain values.
pub const TRUST_LEVEL_MAX_POINTS: [u64; 5] = [100, 200, 500, 1_000, 2_000];
pub const TRUST_LEVEL_DAILY_LIMITS: [u64; 5] = [3, 5, 10, 15, 25];

/// Minimum completions required for each trust upgrade (level → level + 1).
pub const TRUST_MIN_COMPLETIONS: [u64; 4] = [3, 10, 25, 50];

/// Minimum reputation (bps) required for each trust upgrade.
pub const TRUST_MIN_REPUTATION_BPS: [u64; 4] = [5_500, 6_500, 7_500, 8_500];

/// Maximum trust level.
pub const MAX_TRUST_LEVEL: u8 = 5;

/// An external agent's trust record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrust {
    pub agent_id: u64,
    pub trust_level: u8,
    pub total_completions: u64,
    pub total_rejections: u64,
    pub reputation_bps: u64,
    pub total_tokens_earned: u64,
}

impl AgentTrust {
    /// Create a new agent at trust level 1 with 50% base reputation.
    pub fn new(agent_id: u64) -> Self {
        Self {
            agent_id,
            trust_level: 1,
            total_completions: 0,
            total_rejections: 0,
            reputation_bps: 5_000, // 50% starting reputation
            total_tokens_earned: 0,
        }
    }

    /// Maximum points this agent can earn per bounty.
    pub fn max_points(&self) -> u64 {
        let idx = (self.trust_level as usize).saturating_sub(1).min(4);
        TRUST_LEVEL_MAX_POINTS[idx]
    }

    /// Maximum bounties this agent can complete per day.
    pub fn daily_limit(&self) -> u64 {
        let idx = (self.trust_level as usize).saturating_sub(1).min(4);
        TRUST_LEVEL_DAILY_LIMITS[idx]
    }

    /// Record a completed bounty and recalculate reputation.
    pub fn record_completion(&mut self, approved: bool, tokens_earned: u64) {
        if approved {
            self.total_completions += 1;
            self.total_tokens_earned += tokens_earned;
        } else {
            self.total_rejections += 1;
        }

        let total = self.total_completions + self.total_rejections;
        if let Some(bps) = (self.total_completions * BPS_DENOMINATOR).checked_div(total) {
            self.reputation_bps = bps;
        }
    }

    /// Check if this agent is eligible for a trust upgrade.
    pub fn can_upgrade(&self) -> Result<()> {
        if self.trust_level >= MAX_TRUST_LEVEL {
            return Err(AmosError::AlreadyMaxTrust {
                level: self.trust_level,
            });
        }

        let idx = (self.trust_level as usize).saturating_sub(1).min(3);
        let required_completions = TRUST_MIN_COMPLETIONS[idx];
        let required_reputation = TRUST_MIN_REPUTATION_BPS[idx];

        if self.total_completions < required_completions {
            return Err(AmosError::TrustUpgradeNotEligible {
                reason: format!(
                    "Need {} completions, have {}",
                    required_completions, self.total_completions
                ),
            });
        }
        if self.reputation_bps < required_reputation {
            return Err(AmosError::TrustUpgradeNotEligible {
                reason: format!(
                    "Need {} bps reputation, have {}",
                    required_reputation, self.reputation_bps
                ),
            });
        }

        Ok(())
    }

    /// Attempt to upgrade trust level. Returns new level on success.
    pub fn try_upgrade(&mut self) -> Result<u8> {
        self.can_upgrade()?;
        self.trust_level += 1;
        Ok(self.trust_level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_agent_starts_at_level_1() {
        let agent = AgentTrust::new(42);
        assert_eq!(agent.trust_level, 1);
        assert_eq!(agent.max_points(), 100);
        assert_eq!(agent.daily_limit(), 3);
    }

    #[test]
    fn reputation_updates_on_completion() {
        let mut agent = AgentTrust::new(1);
        agent.record_completion(true, 100);
        agent.record_completion(true, 100);
        agent.record_completion(false, 0);
        // 2 approved out of 3 = 66.6%
        assert_eq!(agent.reputation_bps, 6666);
    }

    #[test]
    fn upgrade_from_1_to_2() {
        let mut agent = AgentTrust::new(1);
        // Need 3 completions, >= 55% reputation
        for _ in 0..3 {
            agent.record_completion(true, 100);
        }
        assert!(agent.can_upgrade().is_ok());
        let new_level = agent.try_upgrade().unwrap();
        assert_eq!(new_level, 2);
        assert_eq!(agent.max_points(), 200);
    }

    #[test]
    fn cannot_upgrade_without_requirements() {
        let agent = AgentTrust::new(1);
        assert!(agent.can_upgrade().is_err());
    }

    #[test]
    fn cannot_exceed_max_trust() {
        let mut agent = AgentTrust::new(1);
        agent.trust_level = 5;
        assert!(matches!(
            agent.can_upgrade(),
            Err(AmosError::AlreadyMaxTrust { .. })
        ));
    }
}
