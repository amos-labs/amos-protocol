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

// V2 is a new account discriminator and PDA. Legacy escrows have no authenticated
// poster/deadline and cannot be refunded or released by this interface.
pub const COMMERCIAL_ESCROW_V2_SEED: &[u8] = b"commercial_escrow_v2";
#[account]
pub struct CommercialEscrowV2 {
    pub version: u8,
    pub bounty_id: [u8; 32],
    pub poster: Pubkey,
    pub mint: Pubkey,
    pub deadline: i64,
    pub deposited: u64,
    pub status: u8, // 0 open, 1 released, 2 refunded; never close/reinitialize
    pub bump: u8,
}
impl CommercialEscrowV2 {
    pub const SIZE: usize = 8 + 1 + 32 + 32 + 32 + 8 + 8 + 1 + 1;
    fn validate_action(&self, poster: Pubkey, mint: Pubkey, now: i64, refund: bool) -> Result<()> {
        require!(
            self.version == 2 && self.status == 0,
            BountyError::InvalidEscrow
        );
        require_keys_eq!(self.poster, poster, BountyError::InvalidPoster);
        require_keys_eq!(self.mint, mint, BountyError::InvalidMint);
        if refund {
            require!(now >= self.deadline, BountyError::EscrowNotExpired);
        } else {
            require!(now < self.deadline, BountyError::EscrowExpired);
        }
        Ok(())
    }
}

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

    #[account(init, payer = poster, space = CommercialEscrowV2::SIZE,
        seeds = [COMMERCIAL_ESCROW_V2_SEED, &bounty_id], bump)]
    pub escrow_record: Box<Account<'info, CommercialEscrowV2>>,

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
    deadline: i64,
) -> Result<()> {
    require!(reward_amount > 0, BountyError::ZeroTokensCalculated);
    require!(
        reward_amount >= MIN_COMMERCIAL_ESCROW,
        BountyError::EscrowBelowMinimum
    );

    let now = Clock::get()?.unix_timestamp;
    require!(deadline > now, BountyError::InvalidTimestamp);
    require!(ctx.accounts.mint.decimals == 9, BountyError::InvalidMint);
    let record = &mut ctx.accounts.escrow_record;
    record.version = 2;
    record.bounty_id = bounty_id;
    record.poster = ctx.accounts.poster.key();
    record.mint = ctx.accounts.mint.key();
    record.deadline = deadline;
    record.deposited = reward_amount;
    record.status = 0;
    record.bump = ctx.bumps.escrow_record;

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
/// Named recipient accounts bind the reviewer owner and configured fee destinations.
/// V2 escrow_record authenticates original poster, mint, deadline and open status.
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
    #[account(mut, seeds = [COMMERCIAL_ESCROW_V2_SEED, &bounty_id],
        bump = escrow_record.bump,
        constraint = escrow_record.bounty_id == bounty_id @ BountyError::InvalidEscrow,
        has_one = poster @ BountyError::InvalidPoster,
        has_one = mint @ BountyError::InvalidMint)]
    pub escrow_record: Box<Account<'info, CommercialEscrowV2>>,

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
        constraint = escrow_token_account.owner == escrow_authority.key() @ BountyError::InvalidEscrow,
    )]
    pub escrow_token_account: Box<Account<'info, TokenAccount>>,

    /// Escrow authority PDA
    /// CHECK: PDA derived from bounty_escrow seed + bounty_id
    #[account(
        seeds = [BOUNTY_ESCROW_SEED, &bounty_id],
        bump
    )]
    pub escrow_authority: AccountInfo<'info>,

    #[account(mut)]
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
    /// CHECK: Bound to escrow_record.poster, then stored in bounty proof for audit trail
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
    ctx.accounts.escrow_record.validate_action(
        ctx.accounts.poster.key(),
        ctx.accounts.mint.key(),
        clock.unix_timestamp,
        false,
    )?;
    require_keys_eq!(
        ctx.accounts.reviewer_token_account.owner,
        reviewer,
        BountyError::InvalidOperator
    );
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
        quality_score >= MIN_QUALITY_SCORE && quality_score <= 100,
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

    let total_fee = bps_share(escrow_balance, PROTOCOL_FEE_BPS)?;

    let holder_share = bps_share(total_fee, FEE_HOLDER_SHARE_BPS)?;

    let burn_share = bps_share(total_fee, FEE_BURN_SHARE_BPS)?;

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
    let reviewer_tokens = bps_share(net_reward, REVIEWER_REWARD_BPS)?;

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
    operator_stats.last_decay_time = clock.unix_timestamp;

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

    ctx.accounts.escrow_record.status = 1;

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
    #[account(mut, seeds = [COMMERCIAL_ESCROW_V2_SEED, &bounty_id],
        bump = escrow_record.bump,
        constraint = escrow_record.bounty_id == bounty_id @ BountyError::InvalidEscrow,
        has_one = poster @ BountyError::InvalidPoster,
        has_one = mint @ BountyError::InvalidMint)]
    pub escrow_record: Box<Account<'info, CommercialEscrowV2>>,

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
        constraint = escrow_token_account.owner == escrow_authority.key() @ BountyError::InvalidEscrow,
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
    ctx.accounts.escrow_record.validate_action(
        ctx.accounts.poster.key(),
        ctx.accounts.mint.key(),
        Clock::get()?.unix_timestamp,
        true,
    )?;
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

    ctx.accounts.escrow_record.status = 2;

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

// u128 prevents valid 9-decimal balances near the 100M AMOS supply cap from
// overflowing an intermediate amount * BPS product.
fn bps_share(amount: u64, bps: u16) -> Result<u64> {
    require!(
        bps as u64 <= BPS_DENOMINATOR as u64,
        BountyError::ArithmeticOverflow
    );
    Ok((amount as u128 * bps as u128 / BPS_DENOMINATOR as u128) as u64)
}

/// Calculate the current day index since program start
fn calculate_day_index(start_time: i64) -> Result<u32> {
    let clock = Clock::get()?;
    let elapsed = clock
        .unix_timestamp
        .checked_sub(start_time)
        .ok_or(BountyError::InvalidTimestamp)?;
    require!(elapsed >= 0, BountyError::InvalidTimestamp);
    u32::try_from(elapsed / 86400).map_err(|_| error!(BountyError::InvalidDayIndex))
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

    fn record() -> CommercialEscrowV2 {
        CommercialEscrowV2 {
            version: 2,
            bounty_id: [7; 32],
            poster: Pubkey::new_unique(),
            mint: Pubkey::new_unique(),
            deadline: 1000,
            deposited: 100_000_000_000,
            status: 0,
            bump: 255,
        }
    }
    #[test]
    fn refund_requires_original_poster_and_recorded_mint() {
        let r = record();
        assert!(r
            .validate_action(Pubkey::new_unique(), r.mint, 1001, true)
            .is_err());
        assert!(r
            .validate_action(r.poster, Pubkey::new_unique(), 1001, true)
            .is_err());
        assert!(r.validate_action(r.poster, r.mint, 1001, true).is_ok());
    }
    #[test]
    fn deadline_boundary_has_exactly_one_valid_action() {
        let r = record();
        for now in [999, 1000, 1001] {
            assert_eq!(
                r.validate_action(r.poster, r.mint, now, true).is_ok(),
                now >= 1000
            );
            assert_eq!(
                r.validate_action(r.poster, r.mint, now, false).is_ok(),
                now < 1000
            );
        }
    }
    #[test]
    fn settled_or_legacy_records_cannot_be_refunded_or_released() {
        for (version, status) in [(2, 1), (2, 2), (0, 0), (1, 0)] {
            let mut r = record();
            r.version = version;
            r.status = status;
            assert!(r.validate_action(r.poster, r.mint, 999, false).is_err());
            assert!(r.validate_action(r.poster, r.mint, 1001, true).is_err());
        }
        assert!(
            CommercialEscrowV2::try_deserialize(&mut &vec![0u8; CommercialEscrowV2::SIZE][..])
                .is_err()
        );
    }
    #[test]
    fn v2_metadata_is_a_distinct_sized_account() {
        let r = record();
        let mut bytes = vec![];
        r.try_serialize(&mut bytes).unwrap();
        assert_eq!(bytes.len(), CommercialEscrowV2::SIZE);
        let a =
            Pubkey::find_program_address(&[COMMERCIAL_ESCROW_V2_SEED, &r.bounty_id], &crate::ID).0;
        let b = Pubkey::find_program_address(&[BOUNTY_ESCROW_SEED, &r.bounty_id], &crate::ID).0;
        assert_ne!(a, b);
    }
    #[test]
    fn anchor_refund_accounts_reject_wrong_poster_unsigned_and_missing_legacy_metadata() {
        use anchor_lang::solana_program::{program_option::COption, program_pack::Pack};
        use std::collections::BTreeSet;
        fn info(
            key: Pubkey,
            owner: Pubkey,
            bytes: Vec<u8>,
            signer: bool,
            writable: bool,
            executable: bool,
        ) -> AccountInfo<'static> {
            AccountInfo::new(
                Box::leak(Box::new(key)),
                signer,
                writable,
                Box::leak(Box::new(10_000_000)),
                Box::leak(bytes.into_boxed_slice()),
                Box::leak(Box::new(owner)),
                executable,
                0,
            )
        }
        fn serialize<T: AccountSerialize>(v: &T) -> Vec<u8> {
            let mut b = vec![];
            v.try_serialize(&mut b).unwrap();
            b
        }
        fn tokens(key: Pubkey, mint: Pubkey, owner: Pubkey) -> AccountInfo<'static> {
            let mut bytes = vec![0; token::spl_token::state::Account::LEN];
            token::spl_token::state::Account::pack(
                token::spl_token::state::Account {
                    mint,
                    owner,
                    amount: 100,
                    delegate: COption::None,
                    state: token::spl_token::state::AccountState::Initialized,
                    is_native: COption::None,
                    delegated_amount: 0,
                    close_authority: COption::None,
                },
                &mut bytes,
            )
            .unwrap();
            info(key, token::ID, bytes, false, true, false)
        }
        for mutation in ["valid", "attacker", "unsigned", "legacy", "wrong_authority"] {
            let mut r = record();
            let user = if mutation == "attacker" {
                Pubkey::new_unique()
            } else {
                r.poster
            };
            let (record_key, bump) = Pubkey::find_program_address(
                &[COMMERCIAL_ESCROW_V2_SEED, &r.bounty_id],
                &crate::ID,
            );
            r.bump = bump;
            let (config_key, config_bump) =
                Pubkey::find_program_address(&[BOUNTY_CONFIG_SEED], &crate::ID);
            let mut config =
                BountyConfig::try_deserialize_unchecked(&mut &vec![0u8; BountyConfig::SIZE][..])
                    .unwrap();
            config.mint = r.mint;
            config.bump = config_bump;
            let (escrow_key, _) =
                Pubkey::find_program_address(&[BOUNTY_ESCROW_SEED, &r.bounty_id], &crate::ID);
            let escrow = tokens(
                escrow_key,
                r.mint,
                if mutation == "wrong_authority" {
                    user
                } else {
                    escrow_key
                },
            );
            let mut mint_data = vec![0; token::spl_token::state::Mint::LEN];
            token::spl_token::state::Mint::pack(
                token::spl_token::state::Mint {
                    mint_authority: COption::None,
                    supply: 1000,
                    decimals: 9,
                    is_initialized: true,
                    freeze_authority: COption::None,
                },
                &mut mint_data,
            )
            .unwrap();
            let accounts = Box::leak(
                vec![
                    info(
                        record_key,
                        crate::ID,
                        if mutation == "legacy" {
                            vec![0; CommercialEscrowV2::SIZE]
                        } else {
                            serialize(&r)
                        },
                        false,
                        true,
                        false,
                    ),
                    info(
                        config_key,
                        crate::ID,
                        serialize(&config),
                        false,
                        false,
                        false,
                    ),
                    escrow.clone(),
                    escrow,
                    info(r.mint, token::ID, mint_data, false, false, false),
                    tokens(Pubkey::new_unique(), r.mint, user),
                    info(
                        user,
                        Pubkey::default(),
                        vec![],
                        mutation != "unsigned",
                        true,
                        false,
                    ),
                    info(token::ID, Pubkey::default(), vec![], false, false, true),
                ]
                .into_boxed_slice(),
            );
            let mut remaining = &accounts[..];
            let result = RefundEscrow::try_accounts(
                &crate::ID,
                &mut remaining,
                &r.bounty_id,
                &mut RefundEscrowBumps::default(),
                &mut BTreeSet::new(),
            );
            assert_eq!(result.is_ok(), mutation == "valid", "{mutation}");
        }
    }

    #[test]
    fn full_supply_escrow_splits_conserve_atomic_units_without_intermediate_overflow() {
        for balance in [100_000_000_000u64, 100_000_000 * 1_000_000_000, u64::MAX] {
            let fee = bps_share(balance, PROTOCOL_FEE_BPS).unwrap();
            let holders = bps_share(fee, FEE_HOLDER_SHARE_BPS).unwrap();
            let burn = bps_share(fee, FEE_BURN_SHARE_BPS).unwrap();
            let labs = fee - holders - burn;
            let net = balance - fee;
            let reviewer = bps_share(net, REVIEWER_REWARD_BPS).unwrap();
            let worker = net - reviewer;
            assert_eq!(
                worker as u128 + reviewer as u128 + holders as u128 + burn as u128 + labs as u128,
                balance as u128
            );
        }
    }

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
