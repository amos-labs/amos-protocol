/// AMOS Bounty Program - Contribution Type Registry Instructions
///
/// Governance-updatable registry of contribution types with graduated freeze.
/// Initial 11 types (8 technical + 3 growth). Governance can add/update types.
/// Individual entries can be frozen (one-way). Full registry can be frozen.
/// Auto-freeze after 3 years. Max 2 one-year extensions. Absolute max: 5 years.
///
/// There is NO unfreeze instruction. Immutability is intentional and irreversible.
use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Initialize Registry
// ============================================================================

/// Initialize the contribution type registry with the 11 seed types.
/// Called once after program deployment.
#[derive(Accounts)]
pub struct InitializeRegistry<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        init,
        payer = oracle_authority,
        space = ContributionTypeRegistry::SIZE,
        seeds = [CONTRIBUTION_REGISTRY_SEED],
        bump,
    )]
    pub registry: Box<Account<'info, ContributionTypeRegistry>>,

    #[account(mut)]
    pub oracle_authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Helper to create a name array from a string
fn name_from_str(s: &str) -> [u8; 32] {
    let mut name = [0u8; 32];
    let bytes = s.as_bytes();
    let len = bytes.len().min(32);
    name[..len].copy_from_slice(&bytes[..len]);
    name
}

pub fn handler_initialize_registry(ctx: Context<InitializeRegistry>) -> Result<()> {
    let clock = Clock::get()?;
    let registry = &mut ctx.accounts.registry;
    let config = &ctx.accounts.config;

    registry.bump = ctx.bumps.registry;
    registry.authority = ctx.accounts.oracle_authority.key();
    registry.registry_frozen = false;
    registry.registry_frozen_at = 0;
    registry.entry_count = 11;

    // Initialize all entries to default
    registry.entries = [ContributionTypeEntry::default(); 16];

    // Technical pool (0-7)
    let technical_types: [(u8, &str, u16); 8] = [
        (0, "bug_fix", MULTIPLIER_BUG_FIX_BPS),
        (1, "feature", MULTIPLIER_FEATURE_BPS),
        (2, "documentation", MULTIPLIER_DOCUMENTATION_BPS),
        (3, "content", MULTIPLIER_CONTENT_BPS),
        (4, "support", MULTIPLIER_SUPPORT_BPS),
        (5, "testing_qa", MULTIPLIER_TESTING_BPS),
        (6, "design", MULTIPLIER_DESIGN_BPS),
        (7, "infrastructure", MULTIPLIER_INFRASTRUCTURE_BPS),
    ];

    for (id, name, multiplier) in technical_types.iter() {
        registry.entries[*id as usize] = ContributionTypeEntry {
            type_id: *id,
            name: name_from_str(name),
            multiplier_bps: *multiplier,
            pool_category: PoolCategory::Technical,
            is_active: true,
            frozen: false,
            added_at: clock.unix_timestamp,
            frozen_at: 0,
        };
    }

    // Growth pool (8-10)
    let growth_types: [(u8, &str, u16); 3] = [
        (8, "bug_report", MULTIPLIER_BUG_REPORT_BPS),
        (9, "referral", MULTIPLIER_REFERRAL_BPS),
        (10, "signup", MULTIPLIER_SIGNUP_BPS),
    ];

    for (id, name, multiplier) in growth_types.iter() {
        registry.entries[*id as usize] = ContributionTypeEntry {
            type_id: *id,
            name: name_from_str(name),
            multiplier_bps: *multiplier,
            pool_category: PoolCategory::Growth,
            is_active: true,
            frozen: false,
            added_at: clock.unix_timestamp,
            frozen_at: 0,
        };
    }

    // Pool separation settings
    registry.pool_technical_min_bps = 8000; // 80%
    registry.pool_growth_max_bps = 2000; // 20%

    // Freeze mechanism
    let launch_time = config.start_time;
    registry.freeze_deadline = launch_time + REGISTRY_AUTO_FREEZE_SECONDS;
    registry.extensions_used = 0;
    registry.max_extensions = REGISTRY_MAX_EXTENSIONS;
    registry.extension_duration_seconds = REGISTRY_EXTENSION_DURATION_SECONDS;

    // Growth phase timestamps (relative to launch)
    registry.growth_phase_1_end = launch_time + GROWTH_PHASE_1_DURATION_SECONDS;
    registry.growth_phase_1_cap_bps = GROWTH_PHASE_1_CAP_BPS;
    registry.growth_phase_2_end =
        launch_time + GROWTH_PHASE_1_DURATION_SECONDS + GROWTH_PHASE_2_DURATION_SECONDS;
    registry.growth_phase_2_cap_bps = GROWTH_PHASE_2_CAP_BPS;
    registry.growth_phase_3_end = launch_time
        + GROWTH_PHASE_1_DURATION_SECONDS
        + GROWTH_PHASE_2_DURATION_SECONDS
        + GROWTH_PHASE_3_DURATION_SECONDS;
    registry.growth_phase_3_cap_bps = GROWTH_PHASE_3_CAP_BPS;
    registry.growth_phase_4_cap_bps = GROWTH_PHASE_4_CAP_BPS;

    registry.reserved = [0; 16];

    msg!("Contribution type registry initialized with 11 types (8 technical + 3 growth)");
    msg!(
        "Freeze deadline: {} (3 years from launch)",
        registry.freeze_deadline
    );

    emit!(RegistryInitialized {
        entry_count: 11,
        freeze_deadline: registry.freeze_deadline,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ============================================================================
// Add Contribution Type
// ============================================================================

/// Governance adds a new contribution type to the registry.
/// Requires: registry not frozen, entry_count < 32.
#[derive(Accounts)]
pub struct AddContributionType<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [CONTRIBUTION_REGISTRY_SEED],
        bump = registry.bump,
    )]
    pub registry: Box<Account<'info, ContributionTypeRegistry>>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_add_contribution_type(
    ctx: Context<AddContributionType>,
    name: [u8; 32],
    multiplier_bps: u16,
    pool_category: u8, // 0 = Technical, 1 = Growth
) -> Result<()> {
    let clock = Clock::get()?;
    let registry = &mut ctx.accounts.registry;

    require!(!registry.registry_frozen, BountyError::RegistryFrozen);
    require!(
        registry.entry_count < MAX_CONTRIBUTION_TYPES,
        BountyError::RegistryFull
    );

    let category = match pool_category {
        0 => PoolCategory::Technical,
        1 => PoolCategory::Growth,
        _ => return Err(BountyError::InvalidContributionType.into()),
    };

    let type_id = registry.entry_count;
    registry.entries[type_id as usize] = ContributionTypeEntry {
        type_id,
        name,
        multiplier_bps,
        pool_category: category,
        is_active: true,
        frozen: false,
        added_at: clock.unix_timestamp,
        frozen_at: 0,
    };
    registry.entry_count = type_id + 1;

    emit!(ContributionTypeAdded {
        type_id,
        multiplier_bps,
        timestamp: clock.unix_timestamp,
    });

    msg!("Contribution type {} added", type_id);
    Ok(())
}

// ============================================================================
// Update Contribution Type
// ============================================================================

/// Governance updates a contribution type's multiplier or pool category.
/// Requires: entry not frozen, registry not frozen.
#[derive(Accounts)]
pub struct UpdateContributionType<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [CONTRIBUTION_REGISTRY_SEED],
        bump = registry.bump,
    )]
    pub registry: Box<Account<'info, ContributionTypeRegistry>>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_update_contribution_type(
    ctx: Context<UpdateContributionType>,
    type_id: u8,
    multiplier_bps: u16,
    pool_category: u8,
) -> Result<()> {
    let registry = &mut ctx.accounts.registry;

    require!(!registry.registry_frozen, BountyError::RegistryFrozen);
    require!(
        (type_id as usize) < registry.entry_count as usize,
        BountyError::InvalidContributionType
    );

    let entry = &registry.entries[type_id as usize];
    require!(entry.is_active, BountyError::EntryInactive);
    require!(!entry.frozen, BountyError::EntryFrozen);

    let category = match pool_category {
        0 => PoolCategory::Technical,
        1 => PoolCategory::Growth,
        _ => return Err(BountyError::InvalidContributionType.into()),
    };

    registry.entries[type_id as usize].multiplier_bps = multiplier_bps;
    registry.entries[type_id as usize].pool_category = category;

    msg!(
        "Contribution type {} updated: multiplier={}bps",
        type_id,
        multiplier_bps
    );
    Ok(())
}

// ============================================================================
// Freeze Entry (One-Way, Irreversible)
// ============================================================================

/// Governance freezes a single contribution type entry.
/// ONE-WAY. No unfreeze instruction exists.
#[derive(Accounts)]
pub struct FreezeEntry<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [CONTRIBUTION_REGISTRY_SEED],
        bump = registry.bump,
    )]
    pub registry: Box<Account<'info, ContributionTypeRegistry>>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_freeze_entry(ctx: Context<FreezeEntry>, type_id: u8) -> Result<()> {
    let clock = Clock::get()?;
    let registry = &mut ctx.accounts.registry;

    require!(!registry.registry_frozen, BountyError::RegistryFrozen);
    require!(
        (type_id as usize) < registry.entry_count as usize,
        BountyError::InvalidContributionType
    );

    let entry = &registry.entries[type_id as usize];
    require!(entry.is_active, BountyError::EntryInactive);

    // Idempotent: if already frozen, just return OK
    if entry.frozen {
        return Ok(());
    }

    registry.entries[type_id as usize].frozen = true;
    registry.entries[type_id as usize].frozen_at = clock.unix_timestamp;

    emit!(EntryFrozenEvent {
        type_id,
        frozen_at: clock.unix_timestamp,
    });

    msg!("Contribution type {} frozen permanently", type_id);
    Ok(())
}

// ============================================================================
// Freeze Registry (One-Way, Nuclear Option)
// ============================================================================

/// Governance freezes the entire registry. ONE-WAY.
/// All entries become immutable. The full table becomes an immutable social contract.
#[derive(Accounts)]
pub struct FreezeRegistry<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [CONTRIBUTION_REGISTRY_SEED],
        bump = registry.bump,
    )]
    pub registry: Box<Account<'info, ContributionTypeRegistry>>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_freeze_registry(ctx: Context<FreezeRegistry>) -> Result<()> {
    let clock = Clock::get()?;
    let registry = &mut ctx.accounts.registry;

    require!(!registry.registry_frozen, BountyError::AlreadyFrozen);

    registry.registry_frozen = true;
    registry.registry_frozen_at = clock.unix_timestamp;

    // Freeze all active entries
    for entry in registry.entries.iter_mut() {
        if entry.is_active && !entry.frozen {
            entry.frozen = true;
            entry.frozen_at = clock.unix_timestamp;
        }
    }

    emit!(RegistryFrozenEvent {
        frozen_at: clock.unix_timestamp,
        entry_count: registry.entry_count,
    });

    msg!(
        "Entire registry frozen permanently ({} entries)",
        registry.entry_count
    );
    Ok(())
}

// ============================================================================
// Auto-Freeze Registry (Permissionless — After Deadline)
// ============================================================================

/// Anyone can call this after the freeze deadline.
/// No governance vote needed. The registry just locks.
#[derive(Accounts)]
pub struct AutoFreezeRegistry<'info> {
    #[account(
        mut,
        seeds = [CONTRIBUTION_REGISTRY_SEED],
        bump = registry.bump,
    )]
    pub registry: Box<Account<'info, ContributionTypeRegistry>>,

    /// Anyone can trigger — permissionless
    pub caller: Signer<'info>,
}

pub fn handler_auto_freeze_registry(ctx: Context<AutoFreezeRegistry>) -> Result<()> {
    let clock = Clock::get()?;
    let registry = &mut ctx.accounts.registry;

    require!(
        clock.unix_timestamp > registry.freeze_deadline,
        BountyError::DeadlineNotReached
    );
    require!(!registry.registry_frozen, BountyError::AlreadyFrozen);

    registry.registry_frozen = true;
    registry.registry_frozen_at = clock.unix_timestamp;

    for entry in registry.entries.iter_mut() {
        if entry.is_active && !entry.frozen {
            entry.frozen = true;
            entry.frozen_at = clock.unix_timestamp;
        }
    }

    emit!(RegistryAutoFrozenEvent {
        frozen_at: clock.unix_timestamp,
    });

    msg!("Registry auto-frozen after deadline");
    Ok(())
}

// ============================================================================
// Extend Freeze Deadline (Governance Only)
// ============================================================================

/// Governance extends the freeze deadline by exactly 1 year.
/// Maximum 2 extensions. Absolute maximum: 5 years from launch.
#[derive(Accounts)]
pub struct ExtendFreezeDeadline<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [CONTRIBUTION_REGISTRY_SEED],
        bump = registry.bump,
    )]
    pub registry: Box<Account<'info, ContributionTypeRegistry>>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_extend_freeze_deadline(ctx: Context<ExtendFreezeDeadline>) -> Result<()> {
    let registry = &mut ctx.accounts.registry;

    require!(!registry.registry_frozen, BountyError::RegistryFrozen);
    require!(
        registry.extensions_used < registry.max_extensions,
        BountyError::MaxExtensionsReached
    );

    registry.freeze_deadline += registry.extension_duration_seconds;
    registry.extensions_used += 1;

    emit!(DeadlineExtended {
        new_deadline: registry.freeze_deadline,
        extensions_remaining: registry.max_extensions - registry.extensions_used,
    });

    msg!(
        "Freeze deadline extended to {}. Extensions remaining: {}",
        registry.freeze_deadline,
        registry.max_extensions - registry.extensions_used
    );
    Ok(())
}

// ============================================================================
// Helper: Get Current Growth Cap BPS
// ============================================================================

/// Returns the current growth pool cap in BPS based on the phase.
pub fn current_growth_cap_bps(registry: &ContributionTypeRegistry, now: i64) -> u16 {
    if now < registry.growth_phase_1_end {
        registry.growth_phase_1_cap_bps // Phase 1: 10%
    } else if now < registry.growth_phase_2_end {
        registry.growth_phase_2_cap_bps // Phase 2: 20% (peak)
    } else if now < registry.growth_phase_3_end {
        registry.growth_phase_3_cap_bps // Phase 3: 10% (taper)
    } else {
        registry.growth_phase_4_cap_bps // Phase 4: 5% (permanent)
    }
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct RegistryInitialized {
    pub entry_count: u8,
    pub freeze_deadline: i64,
    pub timestamp: i64,
}

#[event]
pub struct ContributionTypeAdded {
    pub type_id: u8,
    pub multiplier_bps: u16,
    pub timestamp: i64,
}

#[event]
pub struct EntryFrozenEvent {
    pub type_id: u8,
    pub frozen_at: i64,
}

#[event]
pub struct RegistryFrozenEvent {
    pub frozen_at: i64,
    pub entry_count: u8,
}

#[event]
pub struct RegistryAutoFrozenEvent {
    pub frozen_at: i64,
}

#[event]
pub struct DeadlineExtended {
    pub new_deadline: i64,
    pub extensions_remaining: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_growth_phase_constants() {
        assert_eq!(GROWTH_PHASE_1_CAP_BPS, 1000); // 10%
        assert_eq!(GROWTH_PHASE_2_CAP_BPS, 2000); // 20%
        assert_eq!(GROWTH_PHASE_3_CAP_BPS, 1000); // 10%
        assert_eq!(GROWTH_PHASE_4_CAP_BPS, 500); // 5%
    }

    #[test]
    fn test_registry_freeze_timeline() {
        // 3 years = 94,608,000 seconds
        assert_eq!(REGISTRY_AUTO_FREEZE_SECONDS, 94_608_000);
        // Max 2 extensions of 1 year each
        assert_eq!(REGISTRY_MAX_EXTENSIONS, 2);
        assert_eq!(REGISTRY_EXTENSION_DURATION_SECONDS, 31_536_000);
        // Absolute max: 3 + 2 = 5 years
        let absolute_max = REGISTRY_AUTO_FREEZE_SECONDS
            + (REGISTRY_MAX_EXTENSIONS as i64) * REGISTRY_EXTENSION_DURATION_SECONDS;
        assert_eq!(absolute_max, 157_680_000); // 5 years
    }

    #[test]
    fn test_initial_types_count() {
        assert_eq!(CONTRIBUTION_TYPE_COUNT, 11); // 8 technical + 3 growth
        assert_eq!(MAX_CONTRIBUTION_TYPES, 16);
    }

    #[test]
    fn test_growth_types_are_identified() {
        // Technical types (0-7)
        for i in 0..=7 {
            assert!(!is_growth_contribution(i), "Type {} should be technical", i);
        }
        // Growth types (8-10)
        for i in 8..=10 {
            assert!(is_growth_contribution(i), "Type {} should be growth", i);
        }
    }

    #[test]
    fn test_name_from_str() {
        let name = name_from_str("infrastructure");
        assert_eq!(&name[..14], b"infrastructure");
        assert_eq!(name[14], 0); // padded with zeros
    }
}
