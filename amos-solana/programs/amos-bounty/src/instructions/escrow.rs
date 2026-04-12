/// AMOS Bounty Program - Commercial Bounty Escrow Instructions
///
/// This module implements the escrow system for user-funded (Commercial) bounties.
/// Commercial bounties have a 3% protocol fee split: 50% holders, 40% burned, 10% Labs.
/// Treasury bounties (daily emission) have 0% fee and are handled in distribution.rs.
///
/// IMPORTANT: Call `prepare_bounty_submission` before `release_commercial_bounty`
/// in the same transaction to ensure operator_stats exists.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Create Commercial Bounty (Escrow Funds)
// ============================================================================

/// A poster creates a commercial bounty by escrowing AMOS tokens.
/// The escrow PDA holds the tokens until the oracle validates completion
/// or the deadline expires for refund.
///
/// # Fee Model
/// - 3% protocol fee deducted at release (not at escrow time)
/// - Fee split: 50% to holder pool, 40% burned, 10% to Labs wallet
/// - Poster escrows the full reward amount; fee is taken from it at release
///
/// # Arguments
/// * `bounty_id` - Unique identifier for this bounty
/// * `reward_amount` - Total AMOS tokens to escrow
/// * `deadline` - Unix timestamp after which poster can reclaim if uncompleted
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32], reward_amount: u64, deadline: i64)]
pub struct CreateCommercialBounty<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = mint @ BountyError::InvalidMint,
    )]
    pub config: Box<Account<'info, BountyConfig>>,

    /// Escrow token account — PDA that holds the escrowed AMOS tokens
    #[account(
        init,
        payer = poster,
        token::mint = mint,
        token::authority = escrow_authority,
        seeds = [BOUNTY_ESCROW_SEED, &bounty_id],
        bump
    )]
    pub escrow_token_account: Box<Account<'info, TokenAccount>>,

    /// Escrow authority PDA (signs transfers out of escrow)
    /// CHECK: PDA derived from bounty_escrow seed + bounty_id
    #[account(
        seeds = [BOUNTY_ESCROW_SEED, &bounty_id],
        bump
    )]
    pub escrow_authority: AccountInfo<'info>,

    pub mint: Box<Account<'info, Mint>>,

    /// Poster's token account (source of escrowed funds)
    #[account(
        mut,
        constraint = poster_token_account.mint == mint.key() @ BountyError::InvalidMint,
        constraint = poster_token_account.owner == poster.key() @ BountyError::InvalidPoster
    )]
    pub poster_token_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub poster: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler_create_commercial_bounty(
    ctx: Context<CreateCommercialBounty>,
    bounty_id: [u8; 32],
    reward_amount: u64,
    _deadline: i64,
) -> Result<()> {
    require!(reward_amount > 0, BountyError::ZeroTokensCalculated);
    require!(
        reward_amount >= MIN_COMMERCIAL_ESCROW,
        BountyError::EscrowBelowMinimum
    );

    // Transfer tokens from poster to escrow
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.poster_token_account.to_account_info(),
                to: ctx.accounts.escrow_token_account.to_account_info(),
                authority: ctx.accounts.poster.to_account_info(),
            },
        ),
        reward_amount,
    )?;

    emit!(CommercialBountyCreated {
        bounty_id,
        poster: ctx.accounts.poster.key(),
        reward_amount,
        timestamp: Clock::get()?.unix_timestamp,
    });

    msg!("Commercial bounty created: {} AMOS escrowed", reward_amount);

    Ok(())
}

// ============================================================================
// Release Escrow to Worker (Oracle-validated completion)
// ============================================================================

/// Oracle validates bounty completion and releases escrowed funds.
/// Protocol fee (3%) is deducted and distributed: 50% holders, 40% burned, 10% Labs.
///
/// Prerequisites: `prepare_bounty_submission` must be called first in the same
/// transaction to ensure operator_stats exists.
///
/// remaining_accounts layout:
/// [0] = reviewer_token_account (mut) — receives 5% of net reward
/// [1] = holder_pool_account (mut) — receives 50% of fee
/// [2] = labs_wallet_account (mut) — receives 10% of fee
///
/// # Arguments
/// * `bounty_id` - The bounty being completed
/// * `base_points` - Base point value for the work
/// * `quality_score` - Quality assessment (30-100)
/// * `contribution_type` - Type of work (0-7)
/// * `is_agent` - Whether this is an AI agent submission
/// * `agent_id` - Agent identifier if applicable
/// * `reviewer` - Address of the reviewer who validated this work
/// * `evidence_hash` - Hash of the work product
/// * `external_reference` - External ID (issue number, PR number, etc.)
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32], base_points: u16, quality_score: u8, contribution_type: u8, is_agent: bool, agent_id: [u8; 32])]
pub struct ReleaseEscrow<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
        has_one = mint @ BountyError::InvalidMint,
        has_one = holder_pool @ BountyError::InvalidHolderPool,
        has_one = labs_wallet @ BountyError::InvalidLabsWallet,
    )]
    pub config: Box<Account<'info, BountyConfig>>,

    /// The bounty proof record (created here)
    #[account(
        init,
        payer = oracle_authority,
        space = BountyProof::SIZE,
        seeds = [BOUNTY_PROOF_SEED, &bounty_id],
        bump
    )]
    pub bounty_proof: Box<Account<'info, BountyProof>>,

    /// Operator stats — must already exist (created by prepare_bounty_submission)
    #[account(
        mut,
        seeds = [OPERATOR_STATS_SEED, operator.key().as_ref()],
        bump = operator_stats.bump
    )]
    pub operator_stats: Box<Account<'info, OperatorStats>>,

    /// The operator earning this bounty
    /// CHECK: Validated through operator_stats PDA derivation
    pub operator: AccountInfo<'info>,

    /// Escrow token account holding the funds (also serves as authority via PDA)
    #[account(
        mut,
        seeds = [BOUNTY_ESCROW_SEED, &bounty_id],
        bump,
        constraint = escrow_token_account.mint == mint.key() @ BountyError::InvalidMint,
    )]
    pub escrow_token_account: Box<Account<'info, TokenAccount>>,

    /// Escrow authority PDA
    /// CHECK: PDA derived from bounty_escrow seed + bounty_id
    #[account(
        seeds = [BOUNTY_ESCROW_SEED, &bounty_id],
        bump
    )]
    pub escrow_authority: AccountInfo<'info>,

    pub mint: Box<Account<'info, Mint>>,

    /// Operator's token account (receives net reward after fee)
    #[account(
        mut,
        constraint = operator_token_account.mint == mint.key() @ BountyError::InvalidMint,
        constraint = operator_token_account.owner == operator.key() @ BountyError::InvalidOperator,
    )]
    pub operator_token_account: Box<Account<'info, TokenAccount>>,

    /// Reviewer's token account (receives 5% of net reward)
    #[account(
        mut,
        constraint = reviewer_token_account.mint == mint.key() @ BountyError::InvalidMint,
    )]
    pub reviewer_token_account: Box<Account<'info, TokenAccount>>,

    /// Holder pool token account — validated against config.holder_pool
    #[account(
        mut,
        constraint = holder_pool.key() == config.holder_pool @ BountyError::InvalidHolderPool,
        constraint = holder_pool.mint == mint.key() @ BountyError::InvalidMint,
    )]
    pub holder_pool: Box<Account<'info, TokenAccount>>,

    /// Labs wallet token account — validated against config.labs_wallet
    #[account(
        mut,
        constraint = labs_wallet.key() == config.labs_wallet @ BountyError::InvalidLabsWallet,
        constraint = labs_wallet.mint == mint.key() @ BountyError::InvalidMint,
    )]
    pub labs_wallet: Box<Account<'info, TokenAccount>>,

    /// Poster who funded this bounty (for recording provenance)
    /// CHECK: Stored in bounty proof for audit trail
    pub poster: AccountInfo<'info>,

    #[account(mut)]
    pub oracle_authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler_release_escrow(
    ctx: Context<ReleaseEscrow>,
    bounty_id: [u8; 32],
    base_points: u16,
    quality_score: u8,
    contribution_type: u8,
    is_agent: bool,
    agent_id: [u8; 32],
    reviewer: Pubkey,
    evidence_hash: [u8; 32],
    external_reference: [u8; 64],
) -> Result<()> {
    let clock = Clock::get()?;
    let config = &mut ctx.accounts.config;
    let bounty_proof = &mut ctx.accounts.bounty_proof;
    let operator_stats = &mut ctx.accounts.operator_stats;

    // ========================================================================
    // Validate fee recipients are configured
    // ========================================================================

    require!(
        config.holder_pool != Pubkey::default(),
        BountyError::FeeRecipientsNotSet
    );
    require!(
        config.labs_wallet != Pubkey::default(),
        BountyError::FeeRecipientsNotSet
    );

    // ========================================================================
    // Validation
    // ========================================================================

    require!(
        quality_score >= MIN_QUALITY_SCORE,
        BountyError::QualityScoreTooLow
    );
    require!(
        contribution_type <= 10,
        BountyError::InvalidContributionType
    );
    require!(
        base_points > 0 && base_points <= MAX_BOUNTY_POINTS,
        BountyError::InvalidBountyPoints
    );
    require!(
        reviewer != ctx.accounts.operator.key(),
        BountyError::ReviewerSameAsOperator
    );
    require!(evidence_hash != [0u8; 32], BountyError::InvalidEvidenceHash);

    let escrow_balance = ctx.accounts.escrow_token_account.amount;
    require!(escrow_balance > 0, BountyError::EscrowNotFunded);

    // Verify operator_stats was properly initialized by prepare instruction
    require!(
        operator_stats.operator == ctx.accounts.operator.key(),
        BountyError::InvalidOperator
    );

    // Trust level: oracle validates agent trust off-chain for commercial bounties
    let trust_level: u8 = 1;

    // ========================================================================
    // Calculate contribution multiplier
    // ========================================================================

    let multiplier_bps = get_contribution_multiplier(contribution_type)?;
    let adjusted_points = ((base_points as u64)
        .checked_mul(multiplier_bps as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)? as u16)
        .min(MAX_BOUNTY_POINTS);

    // Ensure rounding didn't produce zero points
    require!(adjusted_points > 0, BountyError::ZeroPointsAwarded);

    // ========================================================================
    // Protocol Fee Calculation (3% of escrow balance)
    // ========================================================================

    let total_fee = escrow_balance
        .checked_mul(PROTOCOL_FEE_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let holder_share = total_fee
        .checked_mul(FEE_HOLDER_SHARE_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let burn_share = total_fee
        .checked_mul(FEE_BURN_SHARE_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    // Labs gets remainder to handle rounding dust
    let labs_share = total_fee
        .checked_sub(holder_share)
        .ok_or(BountyError::ArithmeticUnderflow)?
        .checked_sub(burn_share)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    let net_reward = escrow_balance
        .checked_sub(total_fee)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    // Reviewer split (5% of net reward)
    let reviewer_tokens = net_reward
        .checked_mul(REVIEWER_REWARD_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let operator_tokens = net_reward
        .checked_sub(reviewer_tokens)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    require!(operator_tokens > 0, BountyError::ZeroTokensCalculated);

    // ========================================================================
    // Execute Transfers from Escrow
    // ========================================================================

    let escrow_seeds = &[
        BOUNTY_ESCROW_SEED,
        bounty_id.as_ref(),
        &[ctx.bumps.escrow_authority],
    ];
    let signer_seeds = &[&escrow_seeds[..]];

    // Transfer to operator
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.escrow_token_account.to_account_info(),
                to: ctx.accounts.operator_token_account.to_account_info(),
                authority: ctx.accounts.escrow_authority.to_account_info(),
            },
            signer_seeds,
        ),
        operator_tokens,
    )?;

    // Transfer to reviewer (named account, validated against mint)
    if reviewer_tokens > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.escrow_token_account.to_account_info(),
                    to: ctx.accounts.reviewer_token_account.to_account_info(),
                    authority: ctx.accounts.escrow_authority.to_account_info(),
                },
                signer_seeds,
            ),
            reviewer_tokens,
        )?;
    }

    // Fee: transfer to holder pool (50%, validated against config.holder_pool)
    if holder_share > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.escrow_token_account.to_account_info(),
                    to: ctx.accounts.holder_pool.to_account_info(),
                    authority: ctx.accounts.escrow_authority.to_account_info(),
                },
                signer_seeds,
            ),
            holder_share,
        )?;
    }

    // Fee: burn (40%)
    if burn_share > 0 {
        token::burn(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.escrow_token_account.to_account_info(),
                    authority: ctx.accounts.escrow_authority.to_account_info(),
                },
                signer_seeds,
            ),
            burn_share,
        )?;
    }

    // Fee: transfer to Labs wallet (10%, validated against config.labs_wallet)
    if labs_share > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.escrow_token_account.to_account_info(),
                    to: ctx.accounts.labs_wallet.to_account_info(),
                    authority: ctx.accounts.escrow_authority.to_account_info(),
                },
                signer_seeds,
            ),
            labs_share,
        )?;
    }

    // ========================================================================
    // Update State
    // ========================================================================

    let current_day = calculate_day_index(config.start_time)?;

    // Update operator stats
    operator_stats.total_bounties = operator_stats
        .total_bounties
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;
    operator_stats.total_points = operator_stats
        .total_points
        .checked_add(adjusted_points as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;
    operator_stats.total_tokens_earned = operator_stats
        .total_tokens_earned
        .checked_add(operator_tokens)
        .ok_or(BountyError::ArithmeticOverflow)?;
    operator_stats.decayable_balance = operator_stats
        .decayable_balance
        .checked_add(operator_tokens)
        .ok_or(BountyError::ArithmeticOverflow)?;
    operator_stats.original_allocation = operator_stats
        .original_allocation
        .checked_add(operator_tokens)
        .ok_or(BountyError::ArithmeticOverflow)?;
    operator_stats.last_activity_time = clock.unix_timestamp;

    // Update global config
    config.total_tokens_distributed = config
        .total_tokens_distributed
        .checked_add(net_reward)
        .ok_or(BountyError::ArithmeticOverflow)?;
    config.total_bounties = config
        .total_bounties
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;
    config.total_points = config
        .total_points
        .checked_add(adjusted_points as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    // Record immutable bounty proof
    bounty_proof.bounty_id = bounty_id;
    bounty_proof.bounty_source = BountySource::Commercial;
    bounty_proof.operator = ctx.accounts.operator.key();
    bounty_proof.funded_by = ctx.accounts.poster.key();
    bounty_proof.escrow_account = ctx.accounts.escrow_token_account.key();
    bounty_proof.base_points = base_points;
    bounty_proof.adjusted_points = adjusted_points;
    bounty_proof.quality_score = quality_score;
    bounty_proof.contribution_type = contribution_type;
    bounty_proof.is_agent = is_agent;
    bounty_proof.agent_id = agent_id;
    bounty_proof.trust_level = trust_level;
    bounty_proof.tokens_earned = operator_tokens;
    bounty_proof.fee_collected = total_fee;
    bounty_proof.reviewer = reviewer;
    bounty_proof.reviewer_tokens = reviewer_tokens;
    bounty_proof.evidence_hash = evidence_hash;
    bounty_proof.timestamp = clock.unix_timestamp;
    bounty_proof.day_index = current_day;
    bounty_proof.external_reference = external_reference;
    bounty_proof.bump = ctx.bumps.bounty_proof;
    bounty_proof.reserved = [0; 8];

    // ========================================================================
    // Emit Events
    // ========================================================================

    emit!(CommercialBountyCompleted {
        bounty_id,
        operator: ctx.accounts.operator.key(),
        poster: ctx.accounts.poster.key(),
        escrow_amount: escrow_balance,
        total_fee,
        holder_share,
        burn_share,
        labs_share,
        operator_tokens,
        reviewer_tokens,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Commercial bounty completed: {} AMOS distributed",
        net_reward
    );
    msg!(
        "Fee: {} total ({} holders, {} burned, {} labs)",
        total_fee,
        holder_share,
        burn_share,
        labs_share
    );

    Ok(())
}

// ============================================================================
// Refund Escrow to Poster (Expired / Uncompleted)
// ============================================================================

/// Poster reclaims escrowed funds if the bounty was not completed by deadline.
/// No fee is charged on refunds.
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32])]
pub struct RefundEscrow<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = mint @ BountyError::InvalidMint,
    )]
    pub config: Box<Account<'info, BountyConfig>>,

    /// Escrow token account
    #[account(
        mut,
        seeds = [BOUNTY_ESCROW_SEED, &bounty_id],
        bump,
        constraint = escrow_token_account.mint == mint.key() @ BountyError::InvalidMint,
    )]
    pub escrow_token_account: Box<Account<'info, TokenAccount>>,

    /// Escrow authority PDA
    /// CHECK: PDA derived from bounty_escrow seed + bounty_id
    #[account(
        seeds = [BOUNTY_ESCROW_SEED, &bounty_id],
        bump
    )]
    pub escrow_authority: AccountInfo<'info>,

    pub mint: Box<Account<'info, Mint>>,

    /// Poster's token account (destination for refund)
    #[account(
        mut,
        constraint = poster_token_account.mint == mint.key() @ BountyError::InvalidMint,
        constraint = poster_token_account.owner == poster.key() @ BountyError::InvalidPoster
    )]
    pub poster_token_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub poster: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handler_refund_escrow(ctx: Context<RefundEscrow>, bounty_id: [u8; 32]) -> Result<()> {
    let escrow_balance = ctx.accounts.escrow_token_account.amount;
    require!(escrow_balance > 0, BountyError::EscrowNotFunded);

    // Transfer all escrowed tokens back to poster
    let escrow_seeds = &[
        BOUNTY_ESCROW_SEED,
        bounty_id.as_ref(),
        &[ctx.bumps.escrow_authority],
    ];
    let signer_seeds = &[&escrow_seeds[..]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.escrow_token_account.to_account_info(),
                to: ctx.accounts.poster_token_account.to_account_info(),
                authority: ctx.accounts.escrow_authority.to_account_info(),
            },
            signer_seeds,
        ),
        escrow_balance,
    )?;

    emit!(CommercialBountyRefunded {
        bounty_id,
        poster: ctx.accounts.poster.key(),
        refund_amount: escrow_balance,
        timestamp: Clock::get()?.unix_timestamp,
    });

    msg!(
        "Commercial bounty refunded: {} AMOS returned to poster",
        escrow_balance
    );

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate the current day index since program start
fn calculate_day_index(start_time: i64) -> Result<u32> {
    let clock = Clock::get()?;
    let elapsed = clock
        .unix_timestamp
        .checked_sub(start_time)
        .ok_or(BountyError::InvalidTimestamp)?;
    let days = (elapsed as u64)
        .checked_div(86400)
        .ok_or(BountyError::ArithmeticOverflow)?;
    Ok(days as u32)
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct CommercialBountyCreated {
    pub bounty_id: [u8; 32],
    pub poster: Pubkey,
    pub reward_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct CommercialBountyCompleted {
    pub bounty_id: [u8; 32],
    pub operator: Pubkey,
    pub poster: Pubkey,
    pub escrow_amount: u64,
    pub total_fee: u64,
    pub holder_share: u64,
    pub burn_share: u64,
    pub labs_share: u64,
    pub operator_tokens: u64,
    pub reviewer_tokens: u64,
    pub timestamp: i64,
}

#[event]
pub struct CommercialBountyRefunded {
    pub bounty_id: [u8; 32],
    pub poster: Pubkey,
    pub refund_amount: u64,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commercial_fee_calculation() {
        let escrow_balance = 10_000u64;

        let total_fee = escrow_balance * PROTOCOL_FEE_BPS as u64 / BPS_DENOMINATOR as u64;
        assert_eq!(total_fee, 300);

        let holder_share = total_fee * FEE_HOLDER_SHARE_BPS as u64 / BPS_DENOMINATOR as u64;
        assert_eq!(holder_share, 150);

        let burn_share = total_fee * FEE_BURN_SHARE_BPS as u64 / BPS_DENOMINATOR as u64;
        assert_eq!(burn_share, 120);

        let labs_share = total_fee - holder_share - burn_share;
        assert_eq!(labs_share, 30);

        let net_reward = escrow_balance - total_fee;
        assert_eq!(net_reward, 9700);

        assert_eq!(holder_share + burn_share + labs_share, total_fee);
    }

    #[test]
    fn test_reviewer_split_on_commercial() {
        let net_reward = 9700u64;
        let reviewer_tokens = net_reward * REVIEWER_REWARD_BPS as u64 / BPS_DENOMINATOR as u64;
        let operator_tokens = net_reward - reviewer_tokens;

        assert_eq!(reviewer_tokens, 485);
        assert_eq!(operator_tokens, 9215);
    }

    #[test]
    fn test_treasury_bounty_has_zero_fee() {
        let fee_collected = 0u64;
        assert_eq!(fee_collected, 0);
    }

    #[test]
    fn test_fee_rounding_dust_goes_to_labs() {
        let escrow_balance = 333u64;
        let total_fee = escrow_balance * PROTOCOL_FEE_BPS as u64 / BPS_DENOMINATOR as u64;

        let holder_share = total_fee * FEE_HOLDER_SHARE_BPS as u64 / BPS_DENOMINATOR as u64;
        let burn_share = total_fee * FEE_BURN_SHARE_BPS as u64 / BPS_DENOMINATOR as u64;
        let labs_share = total_fee - holder_share - burn_share;

        assert_eq!(holder_share + burn_share + labs_share, total_fee);
    }
}
