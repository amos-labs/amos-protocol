//! V2 custody and cumulative reward accounting. No legacy accounting is imported.
use crate::{constants::*, errors::TreasuryError, state::TreasuryConfig};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

pub const POOL_V2: &[u8] = b"revenue_pool_v2";
pub const STAKE_V2: &[u8] = b"stake_v2";
pub const CUSTODY_V2: &[u8] = b"stake_custody_v2";
pub const REWARDS_V2: &[u8] = b"holder_rewards_v2";
pub const REWARD_SCALE: u128 = 1_000_000_000_000_000_000;

#[account]
#[derive(Default)]
pub struct RevenuePoolV2 {
    pub version: u8,
    pub mint: Pubkey,
    pub labs_wallet: Pubkey,
    pub total_staked: u64,
    pub reward_index: u128,
    /// Actual vault balance already accounted for; claims reduce this atomically.
    pub accounted_balance: u64,
    /// Receipts with no stakers never become a windfall for a future first stake.
    pub unallocated: u64,
    pub total_received: u64,
    pub total_claimed: u64,
    pub bump: u8,
}
impl RevenuePoolV2 {
    pub const LEN: usize = 8 + 1 + 32 + 32 + 8 + 16 + 8 * 4 + 1;
    pub fn sync(&mut self, actual_balance: u64) -> Result<()> {
        require!(self.version == 2, TreasuryError::UnsupportedVersion);
        let received = actual_balance
            .checked_sub(self.accounted_balance)
            .ok_or(TreasuryError::InvalidHolderPoolState)?;
        self.total_received = self
            .total_received
            .checked_add(received)
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        if self.total_staked == 0 {
            self.unallocated = self
                .unallocated
                .checked_add(received)
                .ok_or(TreasuryError::ArithmeticOverflow)?;
        } else {
            let delta = (received as u128)
                .checked_mul(REWARD_SCALE)
                .ok_or(TreasuryError::ArithmeticOverflow)?
                / self.total_staked as u128;
            self.reward_index = self
                .reward_index
                .checked_add(delta)
                .ok_or(TreasuryError::ArithmeticOverflow)?;
        }
        self.accounted_balance = actual_balance;
        Ok(())
    }
}

#[account]
#[derive(Default)]
pub struct StakePositionV2 {
    pub version: u8,
    pub owner: Pubkey,
    pub amount: u64,
    pub index_checkpoint: u128,
    pub fractional_credit: u128,
    pub pending: u64,
    pub eligible_at: i64,
    pub total_claimed: u64,
    pub bump: u8,
}
impl StakePositionV2 {
    pub const LEN: usize = 8 + 1 + 32 + 8 + 16 + 16 + 8 + 8 + 8 + 1;
    pub fn settle(&mut self, index: u128) -> Result<()> {
        require!(self.version == 2, TreasuryError::UnsupportedVersion);
        let delta = index
            .checked_sub(self.index_checkpoint)
            .ok_or(TreasuryError::InvalidStakeState)?;
        let scaled = delta
            .checked_mul(self.amount as u128)
            .and_then(|v| v.checked_add(self.fractional_credit))
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        let earned =
            u64::try_from(scaled / REWARD_SCALE).map_err(|_| TreasuryError::ArithmeticOverflow)?;
        self.pending = self
            .pending
            .checked_add(earned)
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        self.fractional_credit = scaled % REWARD_SCALE;
        self.index_checkpoint = index;
        Ok(())
    }
    pub fn take_claim(&mut self, pool: &mut RevenuePoolV2, now: i64) -> Result<u64> {
        self.settle(pool.reward_index)?;
        require!(
            now >= self.eligible_at,
            TreasuryError::MinimumStakePeriodNotMet
        );
        let amount = self.pending;
        require!(amount > 0, TreasuryError::NoClaimableRevenue);
        pool.accounted_balance = pool
            .accounted_balance
            .checked_sub(amount)
            .ok_or(TreasuryError::InsufficientHolderPoolFunds)?;
        pool.total_claimed = pool
            .total_claimed
            .checked_add(amount)
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        self.total_claimed = self
            .total_claimed
            .checked_add(amount)
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        self.pending = 0;
        Ok(amount)
    }
}

#[derive(Accounts)]
pub struct InitializeRevenueV2<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(seeds = [seeds::TREASURY_CONFIG], bump = treasury_config.bump,
        has_one = authority @ TreasuryError::Unauthorized,
        constraint = treasury_config.amos_mint == mint.key() @ TreasuryError::InvalidMint)]
    pub treasury_config: Box<Account<'info, TreasuryConfig>>,
    #[account(constraint = mint.decimals == AMOS_DECIMALS @ TreasuryError::InvalidMint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(init, payer = authority, space = RevenuePoolV2::LEN, seeds = [POOL_V2], bump)]
    pub pool: Box<Account<'info, RevenuePoolV2>>,
    #[account(init, payer = authority, seeds = [CUSTODY_V2], bump, token::mint = mint, token::authority = pool)]
    pub custody: Box<Account<'info, TokenAccount>>,
    #[account(init, payer = authority, seeds = [REWARDS_V2], bump, token::mint = mint, token::authority = pool)]
    pub rewards: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
pub fn initialize_revenue_v2(ctx: Context<InitializeRevenueV2>) -> Result<()> {
    require!(
        ctx.accounts.treasury_config.labs_wallet != Pubkey::default(),
        TreasuryError::InvalidLabsWallet
    );
    let pool = &mut ctx.accounts.pool;
    pool.version = 2;
    pool.mint = ctx.accounts.mint.key();
    pool.labs_wallet = ctx.accounts.treasury_config.labs_wallet;
    pool.bump = ctx.bumps.pool;
    Ok(())
}

#[derive(Accounts)]
pub struct OpenStakeV2<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut, seeds = [POOL_V2], bump = pool.bump, constraint = pool.version == 2 @ TreasuryError::UnsupportedVersion)]
    pub pool: Box<Account<'info, RevenuePoolV2>>,
    #[account(init, payer = owner, space = StakePositionV2::LEN, seeds = [STAKE_V2, owner.key().as_ref()], bump)]
    pub position: Box<Account<'info, StakePositionV2>>,
    #[account(mut, token::mint = pool.mint, token::authority = owner)]
    pub owner_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [CUSTODY_V2], bump, token::mint = pool.mint, token::authority = pool)]
    pub custody: Box<Account<'info, TokenAccount>>,
    #[account(seeds = [REWARDS_V2], bump, token::mint = pool.mint, token::authority = pool)]
    pub rewards: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
pub fn open_stake_v2(ctx: Context<OpenStakeV2>, amount: u64) -> Result<()> {
    require!(amount >= MIN_STAKE_AMOUNT, TreasuryError::StakeAmountTooLow);
    let now = Clock::get()?.unix_timestamp;
    let pool = &mut ctx.accounts.pool;
    pool.sync(ctx.accounts.rewards.amount)?;
    let position = &mut ctx.accounts.position;
    position.version = 2;
    position.owner = ctx.accounts.owner.key();
    position.index_checkpoint = pool.reward_index;
    position.amount = amount;
    position.eligible_at = now
        .checked_add((MIN_STAKE_DAYS * 86400) as i64)
        .ok_or(TreasuryError::ArithmeticOverflow)?;
    position.bump = ctx.bumps.position;
    pool.total_staked = pool
        .total_staked
        .checked_add(amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.owner_tokens.to_account_info(),
                to: ctx.accounts.custody.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        amount,
    )
}

#[derive(Accounts)]
pub struct ManageStakeV2<'info> {
    pub owner: Signer<'info>,
    #[account(mut, seeds = [POOL_V2], bump = pool.bump, constraint = pool.version == 2 @ TreasuryError::UnsupportedVersion)]
    pub pool: Box<Account<'info, RevenuePoolV2>>,
    #[account(mut, seeds = [STAKE_V2, owner.key().as_ref()], bump = position.bump,
        has_one = owner @ TreasuryError::NotStakeOwner, constraint = position.version == 2 @ TreasuryError::UnsupportedVersion)]
    pub position: Box<Account<'info, StakePositionV2>>,
    #[account(mut, token::mint = pool.mint, token::authority = owner)]
    pub owner_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [CUSTODY_V2], bump, token::mint = pool.mint, token::authority = pool)]
    pub custody: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [REWARDS_V2], bump, token::mint = pool.mint, token::authority = pool)]
    pub rewards: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
pub fn set_stake_v2(ctx: Context<ManageStakeV2>, amount: u64) -> Result<()> {
    require!(
        amount == 0 || amount >= MIN_STAKE_AMOUNT,
        TreasuryError::StakeBelowMinimum
    );
    let now = Clock::get()?.unix_timestamp;
    ctx.accounts.pool.sync(ctx.accounts.rewards.amount)?;
    ctx.accounts
        .position
        .settle(ctx.accounts.pool.reward_index)?;
    let old = ctx.accounts.position.amount;
    ctx.accounts.pool.total_staked = ctx
        .accounts
        .pool
        .total_staked
        .checked_sub(old)
        .and_then(|v| v.checked_add(amount))
        .ok_or(TreasuryError::ArithmeticOverflow)?;
    ctx.accounts.position.amount = amount;
    if amount > old {
        ctx.accounts.position.eligible_at = now
            .checked_add((MIN_STAKE_DAYS * 86400) as i64)
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.owner_tokens.to_account_info(),
                    to: ctx.accounts.custody.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            amount - old,
        )?;
    } else if amount < old {
        let seeds: &[&[u8]] = &[POOL_V2, &[ctx.accounts.pool.bump]];
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.custody.to_account_info(),
                    to: ctx.accounts.owner_tokens.to_account_info(),
                    authority: ctx.accounts.pool.to_account_info(),
                },
                &[seeds],
            ),
            old - amount,
        )?;
    }
    Ok(())
}
pub fn claim_revenue_v2(ctx: Context<ManageStakeV2>) -> Result<()> {
    ctx.accounts.pool.sync(ctx.accounts.rewards.amount)?;
    let amount = ctx
        .accounts
        .position
        .take_claim(&mut ctx.accounts.pool, Clock::get()?.unix_timestamp)?;
    let seeds: &[&[u8]] = &[POOL_V2, &[ctx.accounts.pool.bump]];
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.rewards.to_account_info(),
                to: ctx.accounts.owner_tokens.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[seeds],
        ),
        amount,
    )
}

#[derive(Accounts)]
pub struct DistributeFeeV2<'info> {
    pub payer: Signer<'info>,
    #[account(mut, seeds = [POOL_V2], bump = pool.bump, has_one = mint @ TreasuryError::InvalidMint,
        constraint = pool.version == 2 @ TreasuryError::UnsupportedVersion)]
    pub pool: Box<Account<'info, RevenuePoolV2>>,
    #[account(mut, constraint = mint.decimals == AMOS_DECIMALS @ TreasuryError::InvalidMint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, token::mint = mint, token::authority = payer)]
    pub payer_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [REWARDS_V2], bump, token::mint = mint, token::authority = pool)]
    pub rewards: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, token::authority = pool.labs_wallet)]
    pub labs_tokens: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
/// Amount is the fee already collected, not the commercial gross; fresh payer funds only.
pub fn distribute_fee_v2(ctx: Context<DistributeFeeV2>, amount: u64) -> Result<()> {
    require!(amount > 0, TreasuryError::ZeroRevenueAmount);
    let holders =
        ((amount as u128) * FEE_HOLDER_SHARE_BPS as u128 / BPS_DENOMINATOR as u128) as u64;
    let burn = ((amount as u128) * FEE_BURN_SHARE_BPS as u128 / BPS_DENOMINATOR as u128) as u64;
    let labs = amount - holders - burn;
    for (destination, value) in [
        (ctx.accounts.rewards.to_account_info(), holders),
        (ctx.accounts.labs_tokens.to_account_info(), labs),
    ] {
        if value > 0 {
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.payer_tokens.to_account_info(),
                        to: destination,
                        authority: ctx.accounts.payer.to_account_info(),
                    },
                ),
                value,
            )?;
        }
    }
    if burn > 0 {
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.payer_tokens.to_account_info(),
                    authority: ctx.accounts.payer.to_account_info(),
                },
            ),
            burn,
        )?;
    }
    ctx.accounts.rewards.reload()?;
    ctx.accounts.pool.sync(ctx.accounts.rewards.amount)?;
    emit!(FeeDistributedV2 {
        payer: ctx.accounts.payer.key(),
        amount,
        holders,
        burn,
        labs
    });
    Ok(())
}
#[derive(Accounts)]
pub struct SyncRewardsV2<'info> {
    #[account(mut, seeds = [POOL_V2], bump = pool.bump)]
    pub pool: Box<Account<'info, RevenuePoolV2>>,
    #[account(seeds = [REWARDS_V2], bump, token::mint = pool.mint, token::authority = pool)]
    pub rewards: Box<Account<'info, TokenAccount>>,
}
/// Recognize direct holder-share transfers (including commercial escrow). No further fee.
pub fn sync_rewards_v2(ctx: Context<SyncRewardsV2>) -> Result<()> {
    ctx.accounts.pool.sync(ctx.accounts.rewards.amount)
}
#[event]
pub struct FeeDistributedV2 {
    pub payer: Pubkey,
    pub amount: u64,
    pub holders: u64,
    pub burn: u64,
    pub labs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::{program_option::COption, program_pack::Pack};
    use std::collections::BTreeSet;

    fn pool(staked: u64) -> RevenuePoolV2 {
        RevenuePoolV2 {
            version: 2,
            total_staked: staked,
            ..Default::default()
        }
    }
    fn position(amount: u64) -> StakePositionV2 {
        StakePositionV2 {
            version: 2,
            amount,
            eligible_at: 30 * 86400,
            ..Default::default()
        }
    }
    #[test]
    fn repeated_claim_cannot_drain_another_holders_share() {
        let mut p = pool(4 * MIN_STAKE_AMOUNT);
        let mut a = position(MIN_STAKE_AMOUNT);
        let mut b = position(3 * MIN_STAKE_AMOUNT);
        p.sync(1000).unwrap();
        assert_eq!(a.take_claim(&mut p, 30 * 86400).unwrap(), 250);
        for _ in 0..20 {
            assert!(a.take_claim(&mut p, 30 * 86400).is_err());
        }
        assert_eq!(b.take_claim(&mut p, 30 * 86400).unwrap(), 750);
        assert_eq!(p.accounted_balance, 0);
    }
    #[test]
    fn claim_order_does_not_change_entitlements() {
        for reverse in [false, true] {
            let mut p = pool(4 * MIN_STAKE_AMOUNT);
            let mut a = position(MIN_STAKE_AMOUNT);
            let mut b = position(3 * MIN_STAKE_AMOUNT);
            p.sync(1000).unwrap();
            if reverse {
                assert_eq!(b.take_claim(&mut p, i64::MAX).unwrap(), 750);
                assert_eq!(a.take_claim(&mut p, i64::MAX).unwrap(), 250);
            } else {
                assert_eq!(a.take_claim(&mut p, i64::MAX).unwrap(), 250);
                assert_eq!(b.take_claim(&mut p, i64::MAX).unwrap(), 750);
            }
            assert_eq!(p.total_claimed, p.total_received);
        }
    }
    #[test]
    fn new_stake_cannot_claim_past_fees_and_withdrawal_keeps_only_earned_credit() {
        let mut p = pool(MIN_STAKE_AMOUNT);
        let mut a = position(MIN_STAKE_AMOUNT);
        p.sync(100).unwrap();
        let mut b = position(MIN_STAKE_AMOUNT);
        b.index_checkpoint = p.reward_index;
        p.total_staked += b.amount;
        p.sync(200).unwrap();
        a.settle(p.reward_index).unwrap();
        p.total_staked -= a.amount;
        a.amount = 0;
        p.sync(300).unwrap();
        assert_eq!(a.take_claim(&mut p, i64::MAX).unwrap(), 150);
        assert_eq!(b.take_claim(&mut p, i64::MAX).unwrap(), 150);
    }
    #[test]
    fn pending_external_deposit_synced_before_first_stake_is_not_claimable() {
        let mut p = pool(0);
        p.sync(100).unwrap();
        p.total_staked = MIN_STAKE_AMOUNT;
        let mut a = position(MIN_STAKE_AMOUNT);
        assert!(a.take_claim(&mut p, i64::MAX).is_err());
        p.sync(125).unwrap();
        assert_eq!(a.take_claim(&mut p, i64::MAX).unwrap(), 25);
        assert_eq!(p.accounted_balance, 100);
        assert_eq!(p.unallocated, 100);
    }
    #[test]
    fn frequent_settlement_preserves_fraction_and_cannot_create_rewards() {
        let mut p = pool(3 * MIN_STAKE_AMOUNT);
        let mut a = position(MIN_STAKE_AMOUNT);
        let mut b = position(2 * MIN_STAKE_AMOUNT);
        for balance in 1..=1000 {
            p.sync(balance).unwrap();
            a.settle(p.reward_index).unwrap();
        }
        b.settle(p.reward_index).unwrap();
        assert_eq!(a.pending, 333);
        assert_eq!(b.pending, 666);
        assert!(a.pending + b.pending <= p.total_received);
        assert!(a.fractional_credit < REWARD_SCALE);
    }
    #[test]
    fn minimum_uses_nine_decimal_atomic_units_and_claim_delay_is_enforced() {
        assert_eq!(MIN_STAKE_AMOUNT, 100_000_000_000);
        let mut p = pool(MIN_STAKE_AMOUNT);
        let mut a = position(MIN_STAKE_AMOUNT);
        p.sync(10).unwrap();
        assert!(a.take_claim(&mut p, 30 * 86400 - 1).is_err());
        assert_eq!(a.take_claim(&mut p, 30 * 86400).unwrap(), 10);
    }
    #[test]
    fn legacy_account_bytes_and_versions_are_not_accepted_as_v2() {
        let legacy = crate::state::HolderPool {
            amos_balance: 100,
            total_amos_deposited: 100,
            total_amos_claimed: 0,
            claim_count: 0,
            last_deposit_at: 0,
            last_claim_at: 0,
            bump: 0,
        };
        let mut bytes = Vec::new();
        legacy.try_serialize(&mut bytes).unwrap();
        assert!(RevenuePoolV2::try_deserialize(&mut bytes.as_slice()).is_err());
        let mut p = pool(1);
        p.version = 1;
        assert!(p.sync(1).is_err());
        let mut a = position(1);
        a.version = 1;
        assert!(a.settle(0).is_err());
    }
    fn info(
        key: Pubkey,
        owner: Pubkey,
        data: Vec<u8>,
        signer: bool,
        writable: bool,
        executable: bool,
    ) -> AccountInfo<'static> {
        AccountInfo::new(
            Box::leak(Box::new(key)),
            signer,
            writable,
            Box::leak(Box::new(10_000_000_000)),
            Box::leak(data.into_boxed_slice()),
            Box::leak(Box::new(owner)),
            executable,
            0,
        )
    }
    fn data<T: AccountSerialize>(value: &T) -> Vec<u8> {
        let mut b = vec![];
        value.try_serialize(&mut b).unwrap();
        b
    }
    fn token_info(key: Pubkey, mint: Pubkey, owner: Pubkey) -> AccountInfo<'static> {
        let mut bytes = vec![0; anchor_spl::token::spl_token::state::Account::LEN];
        anchor_spl::token::spl_token::state::Account::pack(
            anchor_spl::token::spl_token::state::Account {
                mint,
                owner,
                amount: 100,
                delegate: COption::None,
                state: anchor_spl::token::spl_token::state::AccountState::Initialized,
                is_native: COption::None,
                delegated_amount: 0,
                close_authority: COption::None,
            },
            &mut bytes,
        )
        .unwrap();
        info(key, token::ID, bytes, false, true, false)
    }
    fn accounts(mutation: &str) -> Vec<AccountInfo<'static>> {
        let user = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let (pool_key, bump) = Pubkey::find_program_address(&[POOL_V2], &crate::ID);
        let (position_key, stake_bump) =
            Pubkey::find_program_address(&[STAKE_V2, user.as_ref()], &crate::ID);
        let (custody_key, _) = Pubkey::find_program_address(&[CUSTODY_V2], &crate::ID);
        let (reward_key, _) = Pubkey::find_program_address(&[REWARDS_V2], &crate::ID);
        let mut p = pool(MIN_STAKE_AMOUNT);
        p.mint = mint;
        p.bump = bump;
        let mut pos = position(MIN_STAKE_AMOUNT);
        pos.owner = user;
        pos.bump = stake_bump;
        vec![
            info(
                user,
                Pubkey::default(),
                vec![],
                mutation != "unsigned",
                true,
                false,
            ),
            info(pool_key, crate::ID, data(&p), false, true, false),
            info(position_key, crate::ID, data(&pos), false, true, false),
            token_info(Pubkey::new_unique(), mint, user),
            token_info(
                if mutation == "arbitrary_vault" {
                    Pubkey::new_unique()
                } else {
                    custody_key
                },
                if mutation == "wrong_mint" {
                    Pubkey::new_unique()
                } else {
                    mint
                },
                if mutation == "wrong_authority" {
                    user
                } else {
                    pool_key
                },
            ),
            token_info(reward_key, mint, pool_key),
            info(token::ID, Pubkey::default(), vec![], false, false, true),
        ]
    }
    #[test]
    fn anchor_account_validation_rejects_fake_custody_mint_authority_and_unsigned_owner() {
        for mutation in [
            "valid",
            "arbitrary_vault",
            "wrong_mint",
            "wrong_authority",
            "unsigned",
        ] {
            let values = Box::leak(accounts(mutation).into_boxed_slice());
            let mut remaining = &values[..];
            let result = ManageStakeV2::try_accounts(
                &crate::ID,
                &mut remaining,
                &[],
                &mut ManageStakeV2Bumps::default(),
                &mut BTreeSet::new(),
            );
            assert_eq!(result.is_ok(), mutation == "valid", "{mutation}");
        }
    }
}
