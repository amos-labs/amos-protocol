//! V2 token custody. Legacy liquid-balance votes cannot enter this path.
use crate::{constants::*, errors::GovernanceError, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Mint, Token, TokenAccount, TransferChecked};

#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct VoteForFeatureV2<'info> {
    #[account(mut, seeds = [GOVERNANCE_SEED], bump = governance_config.bump)]
    pub governance_config: Box<Account<'info, GovernanceConfig>>,
    #[account(
        mut, seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.has_custodied_voting() @ GovernanceError::LegacyProposalRequiresResubmission,
        constraint = feature_proposal.status == ProposalStatus::Active @ GovernanceError::InvalidProposalStatus
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,
    #[account(address = governance_config.mint @ GovernanceError::InvalidMint)]
    pub mint: Account<'info, Mint>,
    #[account(
        init, payer = voter, space = VOTE_RECORD_V2_SIZE,
        seeds = [VOTE_RECORD_V2_SEED, proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()], bump
    )]
    pub vote_record: Account<'info, VoteRecordV2>,
    #[account(
        init, payer = voter,
        seeds = [VOTE_VAULT_V2_SEED, proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()], bump,
        token::mint = mint, token::authority = vote_record
    )]
    pub vote_vault: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint, token::authority = voter)]
    pub voter_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub voter: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn require_open_vote_window(proposal: &FeatureProposal, now: i64) -> Result<()> {
    require!(
        proposal.status == ProposalStatus::Active,
        GovernanceError::InvalidProposalStatus
    );
    require!(
        now >= proposal.created_at,
        GovernanceError::InvalidTimestamp
    );
    let expires_at = proposal
        .created_at
        .checked_add(PROPOSAL_EXPIRATION_SECONDS)
        .ok_or(GovernanceError::ArithmeticOverflow)?;
    require!(now < expires_at, GovernanceError::ProposalExpired);
    Ok(())
}

pub fn vote_for_feature_v2(
    ctx: Context<VoteForFeatureV2>,
    proposal_id: u64,
    vote_amount: u64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let proposal = &mut ctx.accounts.feature_proposal;
    require_open_vote_window(proposal, now)?;
    require!(
        ctx.accounts.voter.key() != proposal.proposer,
        GovernanceError::CannotVoteOnOwnProposal
    );
    require!(
        vote_amount >= MIN_VOTE_AMOUNT,
        GovernanceError::VoteAmountTooLow
    );
    require!(
        vote_amount <= ctx.accounts.voter_token_account.amount,
        GovernanceError::InsufficientBalance
    );
    let total = proposal
        .total_votes
        .checked_add(vote_amount)
        .ok_or(GovernanceError::ArithmeticOverflow)?;
    let count = ctx
        .accounts
        .governance_config
        .total_votes
        .checked_add(1)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    // The SPL transfer must succeed before any amount becomes counted influence.
    transfer_vote_tokens(
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.voter_token_account.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.vote_vault.to_account_info(),
        ctx.accounts.voter.to_account_info(),
        vote_amount,
        ctx.accounts.mint.decimals,
        &[],
    )?;
    let record = &mut ctx.accounts.vote_record;
    record.version = 2;
    record.voter = ctx.accounts.voter.key();
    record.proposal = proposal.key();
    record.proposal_id = proposal_id;
    record.mint = ctx.accounts.mint.key();
    record.amount = vote_amount;
    record.voted_at = now;
    record.withdrawn_at = None;
    record.bump = ctx.bumps.vote_record;
    record.vault_bump = ctx.bumps.vote_vault;
    proposal.total_votes = total;
    proposal.updated_at = now;
    ctx.accounts.governance_config.total_votes = count;
    Ok(())
}

#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct WithdrawVoteV2<'info> {
    #[account(seeds = [GOVERNANCE_SEED], bump = governance_config.bump)]
    pub governance_config: Box<Account<'info, GovernanceConfig>>,
    #[account(
        mut, seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.has_custodied_voting() @ GovernanceError::LegacyProposalRequiresResubmission
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,
    #[account(address = governance_config.mint @ GovernanceError::InvalidMint)]
    pub mint: Account<'info, Mint>,
    #[account(
        mut, seeds = [VOTE_RECORD_V2_SEED, proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump = vote_record.bump,
        constraint = vote_record.version == 2 @ GovernanceError::CustodiedVotingRequired,
        constraint = vote_record.voter == voter.key() @ GovernanceError::InvalidAccount,
        constraint = vote_record.proposal == feature_proposal.key() @ GovernanceError::InvalidAccount,
        constraint = vote_record.proposal_id == proposal_id @ GovernanceError::InvalidAccount,
        constraint = vote_record.mint == mint.key() @ GovernanceError::InvalidMint,
        constraint = vote_record.withdrawn_at.is_none() @ GovernanceError::VoteAlreadyWithdrawn
    )]
    pub vote_record: Account<'info, VoteRecordV2>,
    #[account(
        mut, seeds = [VOTE_VAULT_V2_SEED, proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump = vote_record.vault_bump, token::mint = mint, token::authority = vote_record
    )]
    pub vote_vault: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint, token::authority = voter)]
    pub voter_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub voter: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

/// Returns the new tally. A terminal tally is historical and never reopens.
fn withdrawal_tally(proposal: &FeatureProposal, vote: &VoteRecordV2, now: i64) -> Result<u64> {
    require!(
        vote.withdrawn_at.is_none(),
        GovernanceError::VoteAlreadyWithdrawn
    );
    require!(now >= vote.voted_at, GovernanceError::InvalidTimestamp);
    match proposal.status {
        ProposalStatus::Finalized | ProposalStatus::Cancelled => Ok(proposal.total_votes),
        ProposalStatus::Active => {
            let unlocked = vote
                .voted_at
                .checked_add(VOTE_LOCK_SECONDS)
                .ok_or(GovernanceError::ArithmeticOverflow)?;
            let expired = proposal
                .created_at
                .checked_add(PROPOSAL_EXPIRATION_SECONDS)
                .ok_or(GovernanceError::ArithmeticOverflow)?;
            require!(
                now >= unlocked || now >= expired,
                GovernanceError::VoteLocked
            );
            proposal
                .total_votes
                .checked_sub(vote.amount)
                .ok_or_else(|| GovernanceError::ArithmeticUnderflow.into())
        }
        _ => Err(GovernanceError::InvalidProposalStatus.into()),
    }
}

pub fn withdraw_vote_v2(ctx: Context<WithdrawVoteV2>, _proposal_id: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let next_tally = withdrawal_tally(
        &ctx.accounts.feature_proposal,
        &ctx.accounts.vote_record,
        now,
    )?;
    let refund = ctx.accounts.vote_vault.amount;
    require!(
        refund >= ctx.accounts.vote_record.amount,
        GovernanceError::VoteVaultUnderfunded
    );
    let id = ctx.accounts.vote_record.proposal_id.to_le_bytes();
    let voter = ctx.accounts.voter.key();
    let bump = [ctx.accounts.vote_record.bump];
    let seeds: &[&[u8]] = &[VOTE_RECORD_V2_SEED, &id, voter.as_ref(), &bump];
    // Return all tokens (including unsolicited deposits) to the one bound voter,
    // so dust cannot prevent vault closure. Extra deposits never create votes.
    transfer_vote_tokens(
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.vote_vault.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.voter_token_account.to_account_info(),
        ctx.accounts.vote_record.to_account_info(),
        refund,
        ctx.accounts.mint.decimals,
        &[seeds],
    )?;
    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        CloseAccount {
            account: ctx.accounts.vote_vault.to_account_info(),
            destination: ctx.accounts.voter.to_account_info(),
            authority: ctx.accounts.vote_record.to_account_info(),
        },
        &[seeds],
    ))?;
    ctx.accounts.vote_record.withdrawn_at = Some(now);
    // Keep the vote record allocated: init cannot re-create this vote after refund.
    if ctx.accounts.feature_proposal.status == ProposalStatus::Active {
        ctx.accounts.feature_proposal.total_votes = next_tally;
        ctx.accounts.feature_proposal.updated_at = now;
    }
    Ok(())
}

fn transfer_vote_tokens<'info>(
    program: AccountInfo<'info>,
    from: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    to: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    amount: u64,
    decimals: u8,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    token::transfer_checked(
        CpiContext::new_with_signer(
            program,
            TransferChecked {
                from,
                mint,
                to,
                authority,
            },
            signer_seeds,
        ),
        amount,
        decimals,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(status: ProposalStatus) -> FeatureProposal {
        let mut reserved = [0; 128];
        reserved[..8].copy_from_slice(CUSTODIED_VOTING_MARKER);
        FeatureProposal {
            id: 42,
            proposer: Pubkey::new_unique(),
            title: String::new(),
            description: String::new(),
            estimated_bounty: 0,
            total_votes: 100,
            status,
            customer_request_ids: vec![],
            created_at: 1000,
            updated_at: 1000,
            completed_at: None,
            benchmark_result: None,
            ab_test_result: None,
            feedback_result: None,
            steward_approval_result: None,
            bump: 1,
            reserved,
        }
    }
    fn vote() -> VoteRecordV2 {
        VoteRecordV2 {
            version: 2,
            voter: Pubkey::new_unique(),
            proposal: Pubkey::new_unique(),
            proposal_id: 42,
            mint: Pubkey::new_unique(),
            amount: 60,
            voted_at: 1001,
            withdrawn_at: None,
            bump: 1,
            vault_bump: 2,
        }
    }

    #[test]
    fn active_withdrawal_removes_influence_and_respects_lock() {
        let p = proposal(ProposalStatus::Active);
        let v = vote();
        assert!(withdrawal_tally(&p, &v, v.voted_at + VOTE_LOCK_SECONDS - 1).is_err());
        assert_eq!(
            withdrawal_tally(&p, &v, v.voted_at + VOTE_LOCK_SECONDS).unwrap(),
            40
        );
        let mut withdrawn = vote();
        withdrawn.withdrawn_at = Some(1002);
        assert!(withdrawal_tally(&p, &withdrawn, 9999999).is_err());
    }
    #[test]
    fn terminal_refunds_preserve_history_and_in_flight_work_keeps_custody() {
        for status in [ProposalStatus::Finalized, ProposalStatus::Cancelled] {
            let p = proposal(status);
            let v = vote();
            assert_eq!(withdrawal_tally(&p, &v, v.voted_at + 1).unwrap(), 100);
            assert!(require_open_vote_window(&p, v.voted_at + 1).is_err());
        }
        for status in [
            ProposalStatus::Draft,
            ProposalStatus::InDevelopment,
            ProposalStatus::AwaitingGates,
            ProposalStatus::RewardsDistribution,
        ] {
            assert!(withdrawal_tally(&proposal(status), &vote(), 9999999).is_err());
        }
    }
    #[test]
    fn status_authority_cannot_reopen_a_refunded_terminal_tally() {
        use crate::instructions::proposals::validate_status_transition;
        let statuses = [
            ProposalStatus::Draft,
            ProposalStatus::Active,
            ProposalStatus::InDevelopment,
            ProposalStatus::AwaitingGates,
            ProposalStatus::RewardsDistribution,
            ProposalStatus::Finalized,
            ProposalStatus::Cancelled,
        ];
        for terminal in [ProposalStatus::Finalized, ProposalStatus::Cancelled] {
            for target in statuses {
                assert!(validate_status_transition(&proposal(terminal), target, 1002).is_err());
            }
        }
        let mut p = proposal(ProposalStatus::Active);
        assert!(validate_status_transition(&p, ProposalStatus::InDevelopment, 1002).is_ok());
        assert!(validate_status_transition(
            &p,
            ProposalStatus::InDevelopment,
            p.created_at + PROPOSAL_EXPIRATION_SECONDS
        )
        .is_err());
        p.reserved = [0; 128];
        assert!(validate_status_transition(&p, ProposalStatus::InDevelopment, 1002).is_err());
        assert!(validate_status_transition(&p, ProposalStatus::Cancelled, 1002).is_ok());
    }

    #[test]
    fn expiry_allows_exit_but_prevents_new_influence() {
        let p = proposal(ProposalStatus::Active);
        let mut v = vote();
        let expiry = p.created_at + PROPOSAL_EXPIRATION_SECONDS;
        v.voted_at = expiry - 1;
        assert!(require_open_vote_window(&p, expiry - 1).is_ok());
        assert!(require_open_vote_window(&p, expiry).is_err());
        assert_eq!(withdrawal_tally(&p, &v, expiry).unwrap(), 40);
        assert!(require_open_vote_window(&p, p.created_at - 1).is_err());
    }
    #[test]
    fn legacy_counts_and_layout_cannot_be_reinterpreted_as_custody() {
        let mut p = proposal(ProposalStatus::Active);
        assert_eq!(p.custodied_vote_total(), 100);
        p.reserved = [0; 128];
        assert_eq!(p.custodied_vote_total(), 0);
        assert_ne!(VoteRecord::DISCRIMINATOR, VoteRecordV2::DISCRIMINATOR);
        let mut v = vote();
        v.withdrawn_at = Some(42);
        let mut bytes = Vec::new();
        v.try_serialize(&mut bytes).unwrap();
        assert_eq!(bytes.len(), VOTE_RECORD_V2_SIZE);
    }
    #[test]
    fn canonical_vaults_are_bound_to_both_proposal_and_voter() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        let derive = |id: u64, voter: Pubkey| {
            Pubkey::find_program_address(
                &[VOTE_VAULT_V2_SEED, &id.to_le_bytes(), voter.as_ref()],
                &crate::ID,
            )
            .0
        };
        assert_ne!(derive(1, a), derive(1, b));
        assert_ne!(derive(1, a), derive(2, a));
        let record = Pubkey::find_program_address(
            &[VOTE_RECORD_V2_SEED, &1u64.to_le_bytes(), a.as_ref()],
            &crate::ID,
        )
        .0;
        assert_ne!(derive(1, a), record);
        assert_ne!(
            record,
            Pubkey::find_program_address(
                &[VOTE_RECORD_SEED, &1u64.to_le_bytes(), a.as_ref()],
                &crate::ID
            )
            .0
        );
    }

    // These tests execute the actual production transfer helper against SPL
    // Token's native processor. PDA signer privileges are supplied by a narrow
    // CPI stub; they are not a substitute for an SBF/local-validator test.
    mod native_custody {
        use super::*;
        use anchor_lang::solana_program::{
            instruction::Instruction,
            program_error::ProgramError,
            program_option::COption,
            program_pack::Pack,
            program_stubs::{self, SyscallStubs},
        };
        use anchor_spl::token::spl_token::{
            self,
            processor::Processor,
            state::{Account as SplAccount, AccountState, Mint as SplMint},
        };
        use std::{collections::BTreeSet, sync::Mutex};

        static CPI_LOCK: Mutex<()> = Mutex::new(());
        struct NativeTokenCpi;
        impl SyscallStubs for NativeTokenCpi {
            fn sol_invoke_signed(
                &self,
                ix: &Instruction,
                infos: &[AccountInfo],
                seeds: &[&[&[u8]]],
            ) -> std::result::Result<(), ProgramError> {
                if ix.program_id != spl_token::ID {
                    return Err(ProgramError::IncorrectProgramId);
                }
                let signers = seeds
                    .iter()
                    .map(|s| Pubkey::create_program_address(s, &crate::ID))
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|_| ProgramError::InvalidSeeds)?;
                let mut accounts = infos.to_vec();
                for info in &mut accounts {
                    if signers.contains(info.key) {
                        info.is_signer = true;
                    }
                }
                Processor::process(&spl_token::ID, &accounts, &ix.data)
            }
        }
        struct RestoreCpi(Option<Box<dyn SyscallStubs>>);
        impl Drop for RestoreCpi {
            fn drop(&mut self) {
                program_stubs::set_syscall_stubs(self.0.take().unwrap());
            }
        }

        fn info(
            key: Pubkey,
            owner: Pubkey,
            data: Vec<u8>,
            signer: bool,
            executable: bool,
        ) -> AccountInfo<'static> {
            AccountInfo::new(
                Box::leak(Box::new(key)),
                signer,
                true,
                Box::leak(Box::new(10_000_000)),
                Box::leak(data.into_boxed_slice()),
                Box::leak(Box::new(owner)),
                executable,
                0,
            )
        }
        fn token_account(
            key: Pubkey,
            mint: Pubkey,
            owner: Pubkey,
            amount: u64,
        ) -> AccountInfo<'static> {
            let account = SplAccount {
                mint,
                owner,
                amount,
                delegate: COption::None,
                state: AccountState::Initialized,
                is_native: COption::None,
                delegated_amount: 0,
                close_authority: COption::None,
            };
            let mut data = vec![0; SplAccount::LEN];
            SplAccount::pack(account, &mut data).unwrap();
            info(key, spl_token::ID, data, false, false)
        }
        fn mint_account(key: Pubkey) -> AccountInfo<'static> {
            let mint = SplMint {
                mint_authority: COption::None,
                supply: 1000,
                decimals: 9,
                is_initialized: true,
                freeze_authority: COption::None,
            };
            let mut data = vec![0; SplMint::LEN];
            SplMint::pack(mint, &mut data).unwrap();
            info(key, spl_token::ID, data, false, false)
        }
        fn program_account() -> AccountInfo<'static> {
            info(spl_token::ID, Pubkey::new_unique(), vec![], false, true)
        }
        fn amount(account: &AccountInfo) -> u64 {
            SplAccount::unpack(&account.try_borrow_data().unwrap())
                .unwrap()
                .amount
        }
        fn anchor_account<T: AccountSerialize>(
            key: Pubkey,
            value: &T,
            size: usize,
        ) -> AccountInfo<'static> {
            let mut data = vec![0; size];
            value.try_serialize(&mut &mut data[..]).unwrap();
            info(key, crate::ID, data, false, false)
        }

        #[test]
        fn transferred_votes_cannot_be_reused_through_a_second_wallet() {
            let _lock = CPI_LOCK.lock().unwrap();
            let _restore = RestoreCpi(Some(program_stubs::set_syscall_stubs(Box::new(
                NativeTokenCpi,
            ))));
            let voter = Pubkey::new_unique();
            let second = Pubkey::new_unique();
            let mint = mint_account(Pubkey::new_unique());
            let id = 42u64.to_le_bytes();
            let (record, bump) = Pubkey::find_program_address(
                &[VOTE_RECORD_V2_SEED, &id, voter.as_ref()],
                &crate::ID,
            );
            let authority = info(voter, Pubkey::default(), vec![], true, false);
            let record_authority = info(record, crate::ID, vec![], false, false);
            let source = token_account(Pubkey::new_unique(), *mint.key, voter, 100);
            let vault = token_account(Pubkey::new_unique(), *mint.key, record, 0);
            let second_account = token_account(Pubkey::new_unique(), *mint.key, second, 0);
            transfer_vote_tokens(
                program_account(),
                source.clone(),
                mint.clone(),
                vault.clone(),
                authority.clone(),
                60,
                9,
                &[],
            )
            .unwrap();
            assert_eq!((amount(&source), amount(&vault)), (40, 60));
            // The source wallet no longer has the60 tokens with which it voted.
            assert!(transfer_vote_tokens(
                program_account(),
                source.clone(),
                mint.clone(),
                second_account.clone(),
                authority.clone(),
                60,
                9,
                &[]
            )
            .is_err());
            assert_eq!(amount(&second_account), 0);
            // Neither the original wallet nor an unsigned PDA can take custody.
            assert!(transfer_vote_tokens(
                program_account(),
                vault.clone(),
                mint.clone(),
                source.clone(),
                authority,
                60,
                9,
                &[]
            )
            .is_err());
            assert!(transfer_vote_tokens(
                program_account(),
                vault.clone(),
                mint.clone(),
                source.clone(),
                record_authority.clone(),
                60,
                9,
                &[]
            )
            .is_err());
            let bump_seed = [bump];
            let seeds: &[&[u8]] = &[VOTE_RECORD_V2_SEED, &id, voter.as_ref(), &bump_seed];
            transfer_vote_tokens(
                program_account(),
                vault.clone(),
                mint,
                source.clone(),
                record_authority,
                60,
                9,
                &[seeds],
            )
            .unwrap();
            assert_eq!((amount(&source), amount(&vault)), (100, 0));
        }

        fn withdrawal_fixture() -> Vec<AccountInfo<'static>> {
            let mut p = proposal(ProposalStatus::Active);
            let mut v = vote();
            let (governance_key, governance_bump) =
                Pubkey::find_program_address(&[GOVERNANCE_SEED], &crate::ID);
            let (proposal_key, proposal_bump) = Pubkey::find_program_address(
                &[FEATURE_PROPOSAL_SEED, &p.id.to_le_bytes()],
                &crate::ID,
            );
            let (record_key, record_bump) = Pubkey::find_program_address(
                &[VOTE_RECORD_V2_SEED, &p.id.to_le_bytes(), v.voter.as_ref()],
                &crate::ID,
            );
            let (vault_key, vault_bump) = Pubkey::find_program_address(
                &[VOTE_VAULT_V2_SEED, &p.id.to_le_bytes(), v.voter.as_ref()],
                &crate::ID,
            );
            p.bump = proposal_bump;
            v.proposal = proposal_key;
            v.bump = record_bump;
            v.vault_bump = vault_bump;
            let gov = GovernanceConfig {
                authority: Pubkey::new_unique(),
                oracle: Pubkey::new_unique(),
                mint: v.mint,
                treasury: Pubkey::new_unique(),
                params: Pubkey::new_unique(),
                total_proposals: 1,
                total_votes: 1,
                total_bounties_paid: 0,
                bump: governance_bump,
                reserved: [0; 128],
            };
            vec![
                anchor_account(governance_key, &gov, GOVERNANCE_CONFIG_SIZE),
                anchor_account(proposal_key, &p, FEATURE_PROPOSAL_SIZE),
                mint_account(v.mint),
                anchor_account(record_key, &v, VOTE_RECORD_V2_SIZE),
                token_account(vault_key, v.mint, record_key, v.amount),
                token_account(Pubkey::new_unique(), v.mint, v.voter, 0),
                info(v.voter, Pubkey::default(), vec![], true, false),
                program_account(),
            ]
        }
        fn validates(infos: Vec<AccountInfo<'static>>) -> bool {
            let leaked = Box::leak(infos.into_boxed_slice());
            let mut remaining: &[AccountInfo] = leaked;
            WithdrawVoteV2::try_accounts(
                &crate::ID,
                &mut remaining,
                &42u64.to_le_bytes(),
                &mut WithdrawVoteV2Bumps::default(),
                &mut BTreeSet::new(),
            )
            .is_ok()
        }
        #[test]
        fn actual_anchor_refund_constraints_reject_foreign_destinations_and_vaults() {
            assert!(validates(withdrawal_fixture()));
            let mut wrong_owner = withdrawal_fixture();
            wrong_owner[5] = token_account(
                Pubkey::new_unique(),
                *wrong_owner[2].key,
                Pubkey::new_unique(),
                0,
            );
            assert!(!validates(wrong_owner));
            let mut wrong_vault = withdrawal_fixture();
            wrong_vault[4].key = Box::leak(Box::new(Pubkey::new_unique()));
            assert!(!validates(wrong_vault));
            let mut wrong_mint = withdrawal_fixture();
            wrong_mint[5] = token_account(
                Pubkey::new_unique(),
                Pubkey::new_unique(),
                *wrong_mint[6].key,
                0,
            );
            assert!(!validates(wrong_mint));
            let mut no_signature = withdrawal_fixture();
            no_signature[6].is_signer = false;
            assert!(!validates(no_signature));
        }
    }
}
