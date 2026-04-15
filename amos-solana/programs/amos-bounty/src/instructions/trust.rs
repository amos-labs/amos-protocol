/// AMOS Bounty Program - Trust System Instructions
///
/// This module implements the AI agent trust and reputation system.
/// Agents earn higher limits and capabilities through demonstrated performance.
use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Register Agent Trust
// ============================================================================

/// Register a new AI agent in the trust system.
/// Agents start at trust level 1 with limited capabilities.
///
/// # Arguments
/// * `agent_id` - Unique identifier for the agent (32 bytes, typically a hash)
///
/// # Initial State
/// - Trust Level: 1
/// - Max points per bounty: 100
/// - Daily bounty limit: 3
/// - Reputation: 0 (no history yet)
///
/// # Trustless Guarantees
/// - Anyone can register an agent (permissionless)
/// - Initial limits are protocol-defined (no favoritism)
/// - Upgrades are purely merit-based (on-chain verification)
/// - Complete history tracked on-chain (transparent)
#[derive(Accounts)]
#[instruction(agent_id: [u8; 32])]
pub struct RegisterAgentTrust<'info> {
    #[account(
        init,
        payer = operator,
        space = AgentTrustRecord::SIZE,
        seeds = [AGENT_TRUST_SEED, &agent_id],
        bump
    )]
    pub agent_trust: Account<'info, AgentTrustRecord>,

    #[account(mut)]
    pub operator: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler_register_agent(ctx: Context<RegisterAgentTrust>, agent_id: [u8; 32]) -> Result<()> {
    let agent_trust = &mut ctx.accounts.agent_trust;
    let clock = Clock::get()?;

    // Validate agent ID is not empty
    require!(agent_id != [0u8; 32], BountyError::InvalidAgentId);

    // Initialize agent trust record at Level 1
    agent_trust.agent_id = agent_id;
    agent_trust.operator = ctx.accounts.operator.key();
    agent_trust.trust_level = 1; // Start at Level 1
    agent_trust.total_completions = 0;
    agent_trust.total_rejections = 0;
    agent_trust.reputation_score = 0;
    agent_trust.total_tokens_earned = 0;
    agent_trust.total_points_earned = 0;
    agent_trust.created_at = clock.unix_timestamp;
    agent_trust.last_activity = clock.unix_timestamp;
    agent_trust.last_upgrade = clock.unix_timestamp;
    agent_trust.bump = ctx.bumps.agent_trust;
    agent_trust.reserved = [0; 16];

    msg!("Agent registered successfully");
    msg!("Agent ID: {:?}", agent_id);
    msg!("Trust Level: 1");
    msg!("Max points per bounty: 100");
    msg!("Daily limit: 3 bounties");

    emit!(AgentRegistered {
        agent_id,
        operator: ctx.accounts.operator.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ============================================================================
// Record Agent Completion
// ============================================================================

/// Record the outcome of an agent's bounty submission.
/// Updates completion/rejection counts and recalculates reputation.
///
/// # Arguments
/// * `agent_id` - The agent's unique identifier
/// * `approved` - Whether the bounty was approved (true) or rejected (false)
/// * `tokens_earned` - Tokens earned if approved (0 if rejected)
///
/// # Reputation Calculation
/// reputation = (completions × 10000) / (completions + rejections)
///
/// Examples:
/// - 10 completions, 0 rejections = 10000 (100%)
/// - 9 completions, 1 rejection = 9000 (90%)
/// - 5 completions, 5 rejections = 5000 (50%)
///
/// # Trustless Guarantees
/// - Oracle-only recording (validated work only)
/// - Transparent calculation (on-chain formula)
/// - Immutable history (cannot delete past performance)
/// - Automatic reputation updates (no manual intervention)
#[derive(Accounts)]
#[instruction(agent_id: [u8; 32])]
pub struct RecordAgentCompletion<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [AGENT_TRUST_SEED, &agent_id],
        bump = agent_trust.bump
    )]
    pub agent_trust: Account<'info, AgentTrustRecord>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_record_completion(
    ctx: Context<RecordAgentCompletion>,
    agent_id: [u8; 32],
    approved: bool,
    tokens_earned: u64,
) -> Result<()> {
    let agent_trust = &mut ctx.accounts.agent_trust;
    let clock = Clock::get()?;

    // Update completion/rejection counts
    if approved {
        agent_trust.total_completions = agent_trust
            .total_completions
            .checked_add(1)
            .ok_or(BountyError::ArithmeticOverflow)?;

        agent_trust.total_tokens_earned = agent_trust
            .total_tokens_earned
            .checked_add(tokens_earned)
            .ok_or(BountyError::ArithmeticOverflow)?;
    } else {
        agent_trust.total_rejections = agent_trust
            .total_rejections
            .checked_add(1)
            .ok_or(BountyError::ArithmeticOverflow)?;
    }

    // Recalculate reputation score
    agent_trust.reputation_score = AgentTrustRecord::calculate_reputation(
        agent_trust.total_completions,
        agent_trust.total_rejections,
    );

    // Update activity timestamp
    agent_trust.last_activity = clock.unix_timestamp;

    msg!("Agent completion recorded");
    msg!("Approved: {}", approved);
    msg!("Total completions: {}", agent_trust.total_completions);
    msg!("Total rejections: {}", agent_trust.total_rejections);
    msg!("Reputation score: {} / 10000", agent_trust.reputation_score);

    emit!(AgentCompletionRecorded {
        agent_id,
        approved,
        tokens_earned,
        new_reputation: agent_trust.reputation_score,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ============================================================================
// Upgrade Trust Level
// ============================================================================

/// Upgrade an agent's trust level if requirements are met.
/// This is a PERMISSIONLESS operation - anyone can trigger upgrades
/// when the on-chain thresholds are satisfied.
///
/// # Trust Level Requirements
///
/// Level 2: 3 completions, 5500 reputation (55%)
/// - Max points: 200
/// - Daily limit: 5 bounties
///
/// Level 3: 10 completions, 6500 reputation (65%)
/// - Max points: 500
/// - Daily limit: 10 bounties
///
/// Level 4: 25 completions, 7500 reputation (75%)
/// - Max points: 1000
/// - Daily limit: 15 bounties
///
/// Level 5: 50 completions, 8500 reputation (85%)
/// - Max points: 2000 (full access)
/// - Daily limit: 25 bounties
///
/// # Trustless Guarantees
/// - On-chain threshold verification (no subjective approval)
/// - Permissionless execution (anyone can trigger)
/// - Cannot downgrade (only up)
/// - Requirements are protocol constants (cannot be changed)
/// - Complete audit trail (all upgrades recorded)
#[derive(Accounts)]
#[instruction(agent_id: [u8; 32])]
pub struct UpgradeTrustLevel<'info> {
    #[account(
        mut,
        seeds = [AGENT_TRUST_SEED, &agent_id],
        bump = agent_trust.bump
    )]
    pub agent_trust: Account<'info, AgentTrustRecord>,

    /// The party triggering the upgrade (permissionless)
    pub trigger: Signer<'info>,
}

pub fn handler_upgrade_trust(ctx: Context<UpgradeTrustLevel>, agent_id: [u8; 32]) -> Result<()> {
    let agent_trust = &mut ctx.accounts.agent_trust;
    let clock = Clock::get()?;

    let current_level = agent_trust.trust_level;

    // Check if already at max level
    require!(current_level < 5, BountyError::AlreadyMaxTrustLevel);

    // Check if upgrade requirements are met
    let can_upgrade = can_upgrade_to_level(
        current_level,
        agent_trust.total_completions,
        agent_trust.reputation_score,
    )?;

    require!(can_upgrade, BountyError::TrustUpgradeNotAvailable);

    // Determine new level based on current level
    let new_level = match current_level {
        1 => {
            // Check Level 2 requirements
            require!(
                agent_trust.total_completions >= TRUST_LEVEL_2_MIN_COMPLETIONS
                    && agent_trust.reputation_score >= TRUST_LEVEL_2_MIN_REPUTATION,
                BountyError::TrustUpgradeNotAvailable
            );
            2
        }
        2 => {
            // Check Level 3 requirements
            require!(
                agent_trust.total_completions >= TRUST_LEVEL_3_MIN_COMPLETIONS
                    && agent_trust.reputation_score >= TRUST_LEVEL_3_MIN_REPUTATION,
                BountyError::TrustUpgradeNotAvailable
            );
            3
        }
        3 => {
            // Check Level 4 requirements
            require!(
                agent_trust.total_completions >= TRUST_LEVEL_4_MIN_COMPLETIONS
                    && agent_trust.reputation_score >= TRUST_LEVEL_4_MIN_REPUTATION,
                BountyError::TrustUpgradeNotAvailable
            );
            4
        }
        4 => {
            // Check Level 5 requirements
            require!(
                agent_trust.total_completions >= TRUST_LEVEL_5_MIN_COMPLETIONS
                    && agent_trust.reputation_score >= TRUST_LEVEL_5_MIN_REPUTATION,
                BountyError::TrustUpgradeNotAvailable
            );
            5
        }
        _ => return Err(BountyError::InvalidTrustLevel.into()),
    };

    // Apply upgrade
    agent_trust.trust_level = new_level;
    agent_trust.last_upgrade = clock.unix_timestamp;

    // Get new capabilities
    let max_points = get_max_points_for_trust_level(new_level)?;
    let daily_limit = get_daily_limit_for_trust_level(new_level)?;

    msg!("Agent trust level upgraded");
    msg!("Previous level: {}", current_level);
    msg!("New level: {}", new_level);
    msg!("New max points: {}", max_points);
    msg!("New daily limit: {}", daily_limit);

    emit!(TrustLevelUpgraded {
        agent_id,
        previous_level: current_level,
        new_level,
        completions: agent_trust.total_completions,
        reputation: agent_trust.reputation_score,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct AgentRegistered {
    pub agent_id: [u8; 32],
    pub operator: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct AgentCompletionRecorded {
    pub agent_id: [u8; 32],
    pub approved: bool,
    pub tokens_earned: u64,
    pub new_reputation: u32,
    pub timestamp: i64,
}

#[event]
pub struct TrustLevelUpgraded {
    pub agent_id: [u8; 32],
    pub previous_level: u8,
    pub new_level: u8,
    pub completions: u32,
    pub reputation: u32,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_level_requirements() {
        // Level 2 requirements
        assert_eq!(TRUST_LEVEL_2_MIN_COMPLETIONS, 3);
        assert_eq!(TRUST_LEVEL_2_MIN_REPUTATION, 5500);

        // Level 3 requirements
        assert_eq!(TRUST_LEVEL_3_MIN_COMPLETIONS, 10);
        assert_eq!(TRUST_LEVEL_3_MIN_REPUTATION, 6500);

        // Level 4 requirements
        assert_eq!(TRUST_LEVEL_4_MIN_COMPLETIONS, 25);
        assert_eq!(TRUST_LEVEL_4_MIN_REPUTATION, 7500);

        // Level 5 requirements
        assert_eq!(TRUST_LEVEL_5_MIN_COMPLETIONS, 50);
        assert_eq!(TRUST_LEVEL_5_MIN_REPUTATION, 8500);
    }

    #[test]
    fn test_trust_level_capabilities() {
        // Level 1: 100 points, 10 daily
        assert_eq!(TRUST_LEVEL_MAX_POINTS[0], 100);
        assert_eq!(TRUST_LEVEL_DAILY_LIMITS[0], 10);

        // Level 2: 200 points, 20 daily
        assert_eq!(TRUST_LEVEL_MAX_POINTS[1], 200);
        assert_eq!(TRUST_LEVEL_DAILY_LIMITS[1], 20);

        // Level 5: 2000 points (max), 100 daily
        assert_eq!(TRUST_LEVEL_MAX_POINTS[4], 2000);
        assert_eq!(TRUST_LEVEL_DAILY_LIMITS[4], 100);
    }

    #[test]
    fn test_reputation_scenarios() {
        // Perfect record
        let rep = AgentTrustRecord::calculate_reputation(10, 0);
        assert_eq!(rep, 10000); // 100%

        // 90% success
        let rep = AgentTrustRecord::calculate_reputation(9, 1);
        assert_eq!(rep, 9000); // 90%

        // 75% success
        let rep = AgentTrustRecord::calculate_reputation(75, 25);
        assert_eq!(rep, 7500); // 75%

        // 50% success
        let rep = AgentTrustRecord::calculate_reputation(5, 5);
        assert_eq!(rep, 5000); // 50%
    }

    #[test]
    fn test_upgrade_eligibility() {
        // Should be eligible for Level 2
        assert!(can_upgrade_to_level(1, 3, 5500).unwrap());
        assert!(can_upgrade_to_level(1, 10, 10000).unwrap());

        // Should NOT be eligible for Level 2
        assert!(!can_upgrade_to_level(1, 2, 5500).unwrap()); // Not enough completions
        assert!(!can_upgrade_to_level(1, 3, 5499).unwrap()); // Not enough reputation
        assert!(!can_upgrade_to_level(1, 2, 5000).unwrap()); // Neither

        // Should be eligible for Level 5
        assert!(can_upgrade_to_level(4, 50, 8500).unwrap());
        assert!(can_upgrade_to_level(4, 100, 9500).unwrap());

        // Should NOT be eligible for Level 5
        assert!(!can_upgrade_to_level(4, 49, 8500).unwrap()); // Not enough completions
        assert!(!can_upgrade_to_level(4, 50, 8499).unwrap()); // Not enough reputation

        // Already at max level
        assert!(!can_upgrade_to_level(5, 1000, 10000).unwrap());
    }
}
