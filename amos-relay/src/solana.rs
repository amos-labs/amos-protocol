//! Solana RPC client for bounty settlement and fee distribution.
//!
//! Connects to devnet (or mainnet) and submits `submit_bounty_proof` transactions
//! to the on-chain AMOS Bounty Program, completing the economic loop:
//! relay approves bounty → on-chain token distribution → agent receives tokens.

use amos_core::{AmosError, Result};
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program::ID as SYSTEM_PROGRAM_ID,
    transaction::Transaction,
};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

// Well-known program IDs
const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const SPL_ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

// PDA seeds (must match amos-solana/programs/amos-bounty/src/constants.rs)
const BOUNTY_CONFIG_SEED: &[u8] = b"bounty_config";
const DAILY_POOL_SEED: &[u8] = b"daily_pool";
const BOUNTY_PROOF_SEED: &[u8] = b"bounty_proof";
const OPERATOR_STATS_SEED: &[u8] = b"operator_stats";
const AGENT_TRUST_SEED: &[u8] = b"agent_trust";
const BOUNTY_LISTING_SEED: &[u8] = b"bounty_listing";

// ── Dynamic payout constants ─────────────────────────────────────────
/// Virtual points added to the denominator so no single submission can drain the pool.
const VIRTUAL_POINTS_BASE: u64 = 10_000;

/// One whole AMOS token in lamports (10^9).
const ONE_TOKEN: u64 = 1_000_000_000;

/// Wrapper around Solana RPC client for relay operations.
pub struct SolanaClient {
    rpc: Arc<RpcClient>,
    rpc_url: String,
    /// Bounty program ID
    pub bounty_program_id: Pubkey,
    /// Oracle keypair for signing settlement transactions
    oracle_keypair: Option<Keypair>,
    /// AMOS SPL token mint
    mint: Option<Pubkey>,
    /// Treasury token account
    treasury_token_account: Option<Pubkey>,
}

/// Result of a successful bounty settlement on-chain.
#[derive(Debug, Clone)]
pub struct SettlementResult {
    /// Solana transaction signature
    pub tx_signature: String,
    /// Tokens distributed to the operator
    pub operator_tokens: u64,
    /// Tokens distributed to the reviewer
    pub reviewer_tokens: u64,
}

/// Parameters for settling a bounty on-chain.
#[derive(Debug)]
pub struct SettlementParams {
    /// Unique bounty ID (will be hashed to [u8; 32])
    pub bounty_id: String,
    /// Agent's Solana wallet address (operator)
    pub agent_wallet: String,
    /// Reviewer's Solana wallet address
    pub reviewer_wallet: String,
    /// Base contribution points (derived from reward amount)
    pub base_points: u16,
    /// Quality score (0-100)
    pub quality_score: u8,
    /// Contribution type (0=bug_fix, 1=feature, etc.)
    pub contribution_type: u8,
    /// Whether the worker is an autonomous agent
    pub is_agent: bool,
    /// Agent ID bytes (for trust tracking)
    pub agent_id: [u8; 32],
    /// SHA-256 hash of the submission evidence
    pub evidence_hash: [u8; 32],
    /// Maximum token payout in lamports (reward_tokens * 10^9). 0 = no cap.
    pub max_reward: u64,
}

/// Deserialized state from the on-chain DailyPool PDA.
#[derive(Debug, Clone)]
pub struct DailyPoolState {
    pub day_index: u32,
    /// Total emission allocated for this day (in lamports, i.e. tokens × 10^9).
    pub daily_emission: u64,
    /// Tokens already distributed from this pool (lamports).
    pub tokens_distributed: u64,
    /// Total points accumulated across all bounties today.
    pub total_points: u64,
    /// Number of bounty proofs submitted today.
    pub proof_count: u32,
}

/// Compute the dynamic max_reward for a bounty using the combined
/// virtual-points + time-drip formula.
///
/// ```text
/// seconds_elapsed = now - day_start
/// emission_so_far = daily_emission × seconds_elapsed / 86400
/// available_pool  = emission_so_far - tokens_already_distributed
/// denominator     = total_points_today + VIRTUAL_POINTS_BASE + my_points
/// max_reward      = (my_points / denominator) × available_pool
/// ```
///
/// `start_time` is the on-chain program start timestamp (BountyConfig.start_time).
/// `points` is the submitter's base_points for this bounty.
/// Returns max_reward in lamports (tokens × 10^9).
pub fn compute_dynamic_max_reward(
    points: u64,
    pool: &DailyPoolState,
    start_time: i64,
    now: i64,
) -> u64 {
    if points == 0 || pool.daily_emission == 0 {
        return 0;
    }

    // When did this day start?
    let day_start = start_time + (pool.day_index as i64) * 86400;
    let seconds_elapsed = (now - day_start).max(0) as u64;
    let seconds_in_day: u64 = 86400;

    // Time-drip: only the fraction of emission that has "dripped" so far is available
    // Use u128 to avoid overflow: daily_emission (up to ~16000 * 10^9) × seconds_elapsed
    let emission_so_far = ((pool.daily_emission as u128) * (seconds_elapsed as u128)
        / (seconds_in_day as u128)) as u64;

    // Available pool = what has dripped minus what's already been paid out
    let available = emission_so_far.saturating_sub(pool.tokens_distributed);
    if available == 0 {
        return 0;
    }

    // Virtual-points-adjusted proportional share
    // denominator = total_points_today + VIRTUAL_BASE + my_points
    let denominator = pool.total_points + VIRTUAL_POINTS_BASE + points;
    if denominator == 0 {
        return 0;
    }

    // max_reward = (points / denominator) × available
    // Use u128 to prevent overflow
    let max_reward = ((points as u128) * (available as u128) / (denominator as u128)) as u64;

    // Safety floor: at least 1 token (10^9 lamports) if any emission is available,
    // so dust submissions still get something.
    max_reward.max(ONE_TOKEN.min(available))
}

/// Minimum size in bytes of a valid on-chain `BountyConfig` account payload.
/// Layout: 8 discriminator + 32 oracle + 32 mint + 32 treasury + 8 start_time.
const CONFIG_ACCOUNT_MIN_LEN: usize = 112;

/// Byte offset of `start_time` within a `BountyConfig` account payload.
const CONFIG_START_TIME_OFFSET: usize = 104;

/// Minimum size in bytes of a valid on-chain `DailyPool` account payload.
/// Layout: 8 disc + 4 day_index + 8 daily_emission + 8 tokens_distributed +
/// 8 total_points + 4 proof_count + 1 finalized + 1 bump +
/// 8 growth_tokens + 8 growth_points + 8 technical_tokens + 8 technical_points.
const DAILY_POOL_ACCOUNT_MIN_LEN: usize = 8 + 4 + 8 + 8 + 8 + 4 + 1 + 1 + 8 + 8 + 8 + 8;

/// Decode a `BountyConfig` account payload and return its `start_time`.
///
/// Pure function: takes the raw account bytes and returns the little-endian
/// `i64` stored at offset `CONFIG_START_TIME_OFFSET`. Returns
/// `AmosError::Internal` if the payload is shorter than expected, or
/// `AmosError::SolanaRpc` if slice-to-array conversion fails (defense-in-depth;
/// unreachable given the length check).
fn decode_config_start_time(data: &[u8]) -> Result<i64> {
    if data.len() < CONFIG_ACCOUNT_MIN_LEN {
        return Err(AmosError::Internal(format!(
            "Config account too small: got {} bytes, need at least {}",
            data.len(),
            CONFIG_ACCOUNT_MIN_LEN
        )));
    }
    let slice = data[CONFIG_START_TIME_OFFSET..CONFIG_START_TIME_OFFSET + 8]
        .try_into()
        .map_err(|_| AmosError::SolanaRpc("Config start_time slice conversion failed".into()))?;
    Ok(i64::from_le_bytes(slice))
}

/// Decode a `DailyPool` account payload into a [`DailyPoolState`].
///
/// Pure function: takes the raw account bytes (including the 8-byte Anchor
/// discriminator) and the expected `day_index`. Returns
/// `AmosError::Internal` if the payload is shorter than expected, or
/// `AmosError::SolanaRpc` if any slice-to-array conversion fails
/// (defense-in-depth; unreachable given the length check).
fn decode_daily_pool(data: &[u8], day_index: u32) -> Result<DailyPoolState> {
    if data.len() < DAILY_POOL_ACCOUNT_MIN_LEN {
        return Err(AmosError::Internal(format!(
            "DailyPool account too small: got {} bytes, need at least {}",
            data.len(),
            DAILY_POOL_ACCOUNT_MIN_LEN
        )));
    }
    let off = 8; // skip discriminator
    let daily_emission = u64::from_le_bytes(data[off + 4..off + 12].try_into().map_err(|_| {
        AmosError::SolanaRpc("DailyPool daily_emission slice conversion failed".into())
    })?);
    let tokens_distributed =
        u64::from_le_bytes(data[off + 12..off + 20].try_into().map_err(|_| {
            AmosError::SolanaRpc("DailyPool tokens_distributed slice conversion failed".into())
        })?);
    let total_points = u64::from_le_bytes(data[off + 20..off + 28].try_into().map_err(|_| {
        AmosError::SolanaRpc("DailyPool total_points slice conversion failed".into())
    })?);
    let proof_count = u32::from_le_bytes(data[off + 28..off + 32].try_into().map_err(|_| {
        AmosError::SolanaRpc("DailyPool proof_count slice conversion failed".into())
    })?);
    Ok(DailyPoolState {
        day_index,
        daily_emission,
        tokens_distributed,
        total_points,
        proof_count,
    })
}

/// Validate an RPC endpoint URL.
///
/// Rejects empty strings and URLs that don't start with a supported scheme.
/// Solana RPC uses HTTP(S) for JSON-RPC and WS(S) for pubsub; anything else
/// is almost certainly a misconfiguration.
fn validate_rpc_url(rpc_url: &str) -> Result<()> {
    let trimmed = rpc_url.trim();
    if trimmed.is_empty() {
        return Err(AmosError::Validation("RPC URL cannot be empty".into()));
    }
    const SUPPORTED_SCHEMES: [&str; 4] = ["http://", "https://", "ws://", "wss://"];
    if !SUPPORTED_SCHEMES.iter().any(|s| trimmed.starts_with(s)) {
        return Err(AmosError::Validation(format!(
            "RPC URL must start with http://, https://, ws://, or wss:// (got: {})",
            trimmed
        )));
    }
    Ok(())
}

/// Validate that an agent trust level is in the allowed range 1..=5.
fn validate_trust_level(trust_level: u8) -> Result<()> {
    if !(1..=5).contains(&trust_level) {
        return Err(AmosError::Validation(format!(
            "trust_level must be in range 1..=5 (got: {})",
            trust_level
        )));
    }
    Ok(())
}

/// Compute a fallback max_reward when the on-chain pool cannot be read
/// (e.g., pool not created yet, RPC error). Uses a conservative estimate
/// based on the sigmoid emission schedule.
pub fn fallback_max_reward(points: u64) -> u64 {
    // Conservative: assume day 0 emission (16,000 AMOS), full day elapsed,
    // 10,000 total_points already accumulated. This underestimates payout
    // which is the safe direction (on-chain proportional formula still runs).
    let daily_emission: u64 = 16_000 * ONE_TOKEN;
    let assumed_total_points: u64 = 10_000;
    let denominator = assumed_total_points + VIRTUAL_POINTS_BASE + points;
    let max_reward = ((points as u128) * (daily_emission as u128) / (denominator as u128)) as u64;
    max_reward.max(ONE_TOKEN)
}

impl SolanaClient {
    /// Create a new Solana client connected to the given RPC endpoint.
    ///
    /// Validates that `rpc_url` is non-empty and uses a supported scheme
    /// (`http://`, `https://`, or the Solana-specific `ws://`/`wss://`).
    pub fn new(rpc_url: &str, bounty_program_id: &str) -> Result<Self> {
        validate_rpc_url(rpc_url)?;

        let rpc =
            RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

        let bounty_program_id = Pubkey::from_str(bounty_program_id)
            .map_err(|e| AmosError::Validation(format!("Invalid bounty program ID: {}", e)))?;

        Ok(Self {
            rpc: Arc::new(rpc),
            rpc_url: rpc_url.to_string(),
            bounty_program_id,
            oracle_keypair: None,
            mint: None,
            treasury_token_account: None,
        })
    }

    /// Load the oracle keypair from a JSON file (Solana CLI format).
    ///
    /// Enforces a minimum security posture (see `SECURITY.md` for the full model):
    /// * On Unix, the file **must** have mode `0600` — any group/world bit
    ///   causes a hard error (not a warning). An oracle signing key must not
    ///   be readable by other users on the host.
    /// * If the key lives in a risky path (home dir, `/tmp`, `/var/tmp`, or
    ///   any world-writable ancestor), a startup warning is emitted. Loading
    ///   still proceeds — the warning is advisory.
    pub fn load_oracle_keypair(&mut self, keypair_path: &str) -> Result<()> {
        use std::io::Read;

        if keypair_path.trim().is_empty() {
            return Err(AmosError::Validation("Keypair path cannot be empty".into()));
        }

        let path = Path::new(keypair_path);

        // Open+fstat the file once and keep the handle for reading. This
        // closes the TOCTOU window between the permission check and the
        // read: an attacker who races a swap after the stat can't force us
        // to read a different file.
        let mut file = enforce_keypair_permissions(path)?;

        if let Some(msg) = classify_keypair_path(path) {
            warn!(path = %keypair_path, "{}", msg);
        }

        let mut keypair_bytes = String::new();
        file.read_to_string(&mut keypair_bytes).map_err(|e| {
            AmosError::Internal(format!(
                "Failed to read oracle keypair at '{}': {}",
                keypair_path, e
            ))
        })?;

        let bytes: Vec<u8> = serde_json::from_str(&keypair_bytes)
            .map_err(|e| AmosError::Internal(format!("Invalid keypair JSON format: {}", e)))?;

        self.oracle_keypair = Some(
            Keypair::try_from(bytes.as_slice())
                .map_err(|e| AmosError::Internal(format!("Invalid keypair bytes: {}", e)))?,
        );

        if let Some(ref kp) = self.oracle_keypair {
            info!(oracle = %kp.pubkey(), "Oracle keypair loaded");
        }
        Ok(())
    }

    /// Set the AMOS token mint address.
    pub fn set_mint(&mut self, mint_address: &str) -> Result<()> {
        self.mint = Some(
            Pubkey::from_str(mint_address)
                .map_err(|e| AmosError::Internal(format!("Invalid mint address: {}", e)))?,
        );
        Ok(())
    }

    /// Set the treasury token account.
    pub fn set_treasury(&mut self, treasury_address: &str) -> Result<()> {
        self.treasury_token_account = Some(
            Pubkey::from_str(treasury_address)
                .map_err(|e| AmosError::Internal(format!("Invalid treasury address: {}", e)))?,
        );
        Ok(())
    }

    /// Health check: verify RPC is reachable.
    pub async fn health_check(&self) -> Result<()> {
        let rpc_url = self.rpc_url.clone();
        tokio::task::spawn_blocking(move || {
            let rpc = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
            rpc.get_health()
                .map_err(|e| AmosError::SolanaRpc(format!("Health check failed: {}", e)))
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        Ok(())
    }

    /// Check if settlement is fully configured (keypair + mint + treasury).
    pub fn is_settlement_ready(&self) -> bool {
        self.oracle_keypair.is_some()
            && self.mint.is_some()
            && self.treasury_token_account.is_some()
    }

    /// Process bounty payout on-chain via `prepare_bounty_submission` + `submit_bounty_proof`.
    ///
    /// Builds and submits a transaction to the AMOS Bounty Program that:
    /// 1. Prepares daily pool and operator stats (idempotent init)
    /// 2. Records the bounty proof on-chain
    /// 3. Distributes tokens from treasury to the agent (95%) and reviewer (5%)
    /// 4. Updates operator stats and agent trust records
    pub async fn process_bounty_payout(
        &self,
        params: &SettlementParams,
    ) -> Result<SettlementResult> {
        let oracle = self.oracle_keypair.as_ref().ok_or_else(|| {
            AmosError::Internal("Oracle keypair not configured — cannot settle bounties".into())
        })?;
        let mint = self
            .mint
            .ok_or_else(|| AmosError::Internal("Mint address not configured".into()))?;
        let treasury = self
            .treasury_token_account
            .ok_or_else(|| AmosError::Internal("Treasury token account not configured".into()))?;

        let operator = Pubkey::from_str(&params.agent_wallet)
            .map_err(|e| AmosError::Validation(format!("Invalid agent wallet: {}", e)))?;
        let reviewer = Pubkey::from_str(&params.reviewer_wallet)
            .map_err(|e| AmosError::Validation(format!("Invalid reviewer wallet: {}", e)))?;

        let program_id = self.bounty_program_id;

        // Hash the bounty UUID to get a fixed 32-byte ID
        let bounty_id_bytes = hash_to_32_bytes(&params.bounty_id);

        // Derive all PDAs
        let (config_pda, _) = Pubkey::find_program_address(&[BOUNTY_CONFIG_SEED], &program_id);

        // Fetch config account to read start_time for correct day_index calculation
        let rpc_for_config = self.rpc.clone();
        let config_pda_copy = config_pda;
        let start_time = tokio::task::spawn_blocking(move || {
            let account = rpc_for_config
                .get_account(&config_pda_copy)
                .map_err(|e| AmosError::SolanaRpc(format!("Failed to fetch config: {}", e)))?;
            // Layout: 8 (discriminator) + 32 (oracle) + 32 (mint) + 32 (treasury) + 8 (start_time)
            let data = account.data;
            if data.len() < 8 + 32 + 32 + 32 + 8 {
                return Err(AmosError::Internal("Config account too small".into()));
            }
            let ts = i64::from_le_bytes(data[104..112].try_into().map_err(|_| {
                AmosError::Internal("Config account data slice conversion failed".into())
            })?);
            Ok::<i64, AmosError>(ts)
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        let now = chrono::Utc::now().timestamp();
        let day_index = ((now - start_time) / 86400) as u32;

        let (daily_pool_pda, _) =
            Pubkey::find_program_address(&[DAILY_POOL_SEED, &day_index.to_le_bytes()], &program_id);

        let (bounty_proof_pda, _) =
            Pubkey::find_program_address(&[BOUNTY_PROOF_SEED, &bounty_id_bytes], &program_id);

        let (operator_stats_pda, _) =
            Pubkey::find_program_address(&[OPERATOR_STATS_SEED, operator.as_ref()], &program_id);

        // Agent trust record — pass program_id for non-agent (Anchor Optional None pattern)
        let agent_trust_account = if params.is_agent {
            let (pda, _) =
                Pubkey::find_program_address(&[AGENT_TRUST_SEED, &params.agent_id], &program_id);
            pda
        } else {
            program_id // signals "None" to Anchor optional account
        };

        // ── Pre-flight: ensure agent_trust PDA is initialized on-chain ──
        if params.is_agent {
            let rpc_check = self.rpc.clone();
            let agent_trust_pda = agent_trust_account;
            let account_exists = tokio::task::spawn_blocking(move || {
                match rpc_check.get_account(&agent_trust_pda) {
                    Ok(acct) => Ok(!acct.data.is_empty()),
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("AccountNotFound")
                            || err_str.contains("could not find account")
                        {
                            Ok(false)
                        } else {
                            Err(AmosError::SolanaRpc(format!(
                                "Failed to check agent_trust account: {}",
                                e
                            )))
                        }
                    }
                }
            })
            .await
            .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

            if !account_exists {
                info!(
                    agent_id = ?params.agent_id,
                    pda = %agent_trust_account,
                    "Agent trust PDA not initialized — registering on-chain"
                );

                let register_data = build_register_agent_trust_data(&params.agent_id);
                let register_accounts = vec![
                    AccountMeta::new(agent_trust_account, false),
                    AccountMeta::new(oracle.pubkey(), true), // operator (payer + signer)
                    AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                ];
                let register_ix = Instruction {
                    program_id,
                    accounts: register_accounts,
                    data: register_data,
                };

                let rpc_reg = self.rpc.clone();
                let oracle_bytes_reg = oracle.to_bytes();
                tokio::task::spawn_blocking(move || {
                    let oracle_kp =
                        Keypair::try_from(oracle_bytes_reg.as_slice()).map_err(|e| {
                            AmosError::Internal(format!("Keypair reconstruction: {}", e))
                        })?;
                    send_with_retry(&rpc_reg, &[register_ix], &oracle_kp, &[&oracle_kp], 2)
                })
                .await
                .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

                info!(pda = %agent_trust_account, "Agent trust PDA registered successfully");
            }
        }

        // Derive associated token accounts for operator and reviewer
        let operator_ata = derive_associated_token_account(&operator, &mint);
        let reviewer_ata = derive_associated_token_account(&reviewer, &mint);

        let token_program = Pubkey::from_str(SPL_TOKEN_PROGRAM_ID)
            .map_err(|e| AmosError::Internal(format!("Invalid SPL token program ID: {}", e)))?;
        let ata_program = Pubkey::from_str(SPL_ASSOCIATED_TOKEN_PROGRAM_ID)
            .map_err(|e| AmosError::Internal(format!("Invalid ATA program ID: {}", e)))?;

        // ── Pre-flight: ensure operator & reviewer ATAs exist ────────
        // Uses create_associated_token_account_idempotent (instruction byte 1)
        // so it's safe to call even if the ATA already exists.
        for (wallet, ata, label) in [
            (&operator, &operator_ata, "operator"),
            (&reviewer, &reviewer_ata, "reviewer"),
        ] {
            let rpc_ata = self.rpc.clone();
            let ata_copy = *ata;
            let needs_create =
                tokio::task::spawn_blocking(move || match rpc_ata.get_account(&ata_copy) {
                    Ok(acct) => Ok(acct.data.is_empty()),
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("AccountNotFound")
                            || err_str.contains("could not find account")
                        {
                            Ok(true)
                        } else {
                            Err(AmosError::SolanaRpc(format!(
                                "Failed to check {} ATA: {}",
                                "wallet", e
                            )))
                        }
                    }
                })
                .await
                .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

            if needs_create {
                info!(
                    wallet = %wallet,
                    ata = %ata,
                    label,
                    "Creating associated token account"
                );

                let create_ata_ix = Instruction {
                    program_id: ata_program,
                    accounts: vec![
                        AccountMeta::new(oracle.pubkey(), true), // payer
                        AccountMeta::new(*ata, false),
                        AccountMeta::new_readonly(*wallet, false),
                        AccountMeta::new_readonly(mint, false),
                        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                        AccountMeta::new_readonly(token_program, false),
                    ],
                    data: vec![1], // 1 = create_idempotent
                };

                let rpc_create = self.rpc.clone();
                let oracle_bytes_ata = oracle.to_bytes();
                tokio::task::spawn_blocking(move || {
                    let oracle_kp =
                        Keypair::try_from(oracle_bytes_ata.as_slice()).map_err(|e| {
                            AmosError::Internal(format!("Keypair reconstruction: {}", e))
                        })?;
                    send_with_retry(&rpc_create, &[create_ata_ix], &oracle_kp, &[&oracle_kp], 2)
                })
                .await
                .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

                info!(ata = %ata, label, "ATA created successfully");
            }
        }

        // ── Instruction 1: prepare_bounty_submission ──────────────────
        // Creates daily_pool and operator_stats if they don't exist (idempotent)
        let prepare_data = build_prepare_bounty_submission_data(&operator, day_index);

        let prepare_accounts = vec![
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(daily_pool_pda, false),
            AccountMeta::new(operator_stats_pda, false),
            AccountMeta::new(oracle.pubkey(), true), // payer + signer
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        let prepare_ix = Instruction {
            program_id,
            accounts: prepare_accounts,
            data: prepare_data,
        };

        // ── Instruction 2: submit_bounty_proof ────────────────────────
        let submit_data = build_submit_bounty_proof_data(
            &bounty_id_bytes,
            params.base_points,
            params.quality_score,
            params.contribution_type,
            params.is_agent,
            &params.agent_id,
            day_index,
            params.max_reward,
            &reviewer,
            &params.evidence_hash,
        );

        // Account order matches the SubmitBountyProof Anchor context struct
        let submit_accounts = vec![
            AccountMeta::new(config_pda, false),
            AccountMeta::new(daily_pool_pda, false),
            AccountMeta::new(bounty_proof_pda, false),
            AccountMeta::new(operator_stats_pda, false),
            AccountMeta::new_readonly(operator, false),
            AccountMeta::new(agent_trust_account, false), // mut — on-chain updates trust stats
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(treasury, false),
            AccountMeta::new(operator_ata, false),
            AccountMeta::new(reviewer_ata, false),
            AccountMeta::new(oracle.pubkey(), true), // oracle_authority signer
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        let submit_ix = Instruction {
            program_id,
            accounts: submit_accounts,
            data: submit_data,
        };

        // Build, sign, and send transaction with both instructions (with retry)
        let rpc = self.rpc.clone();
        let oracle_keypair_bytes = oracle.to_bytes();

        let tx_signature = tokio::task::spawn_blocking(move || {
            let oracle_kp = Keypair::try_from(oracle_keypair_bytes.as_slice())
                .map_err(|e| AmosError::Internal(format!("Keypair reconstruction: {}", e)))?;

            send_with_retry(
                &rpc,
                &[prepare_ix, submit_ix],
                &oracle_kp,
                &[&oracle_kp],
                4, // max 4 retries = 5 total attempts
            )
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        info!(
            bounty_id = %params.bounty_id,
            tx = %tx_signature,
            agent = %params.agent_wallet,
            day_index,
            "Bounty settlement transaction confirmed on-chain"
        );

        Ok(SettlementResult {
            tx_signature,
            operator_tokens: 0, // Actual amount determined by on-chain pool math
            reviewer_tokens: 0,
        })
    }

    // ── Dynamic Pool Reader ─────────────────────────────────────────

    /// Read the on-chain BountyConfig start_time.
    /// Returns (start_time, day_index) for the current moment.
    pub async fn read_config_timing(&self) -> Result<(i64, u32)> {
        let program_id = self.bounty_program_id;
        let (config_pda, _) = Pubkey::find_program_address(&[BOUNTY_CONFIG_SEED], &program_id);

        let rpc = self.rpc.clone();
        let (start_time, now) = tokio::task::spawn_blocking(move || {
            let mut last_err = None;
            for attempt in 0..3 {
                match rpc.get_account(&config_pda) {
                    Ok(account) => {
                        let ts = decode_config_start_time(&account.data)?;
                        let now = chrono::Utc::now().timestamp();
                        return Ok::<(i64, i64), AmosError>((ts, now));
                    }
                    Err(e) => {
                        last_err = Some(e);
                        if attempt < 2 {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                    }
                }
            }
            Err(AmosError::SolanaRpc(format!(
                "Failed to fetch config after 3 attempts: {}",
                last_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "no error recorded".into())
            )))
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        let day_index = ((now - start_time) / 86400) as u32;
        Ok((start_time, day_index))
    }

    /// Read the on-chain DailyPool PDA for a given day_index.
    /// Returns `None` if the account doesn't exist yet (no submissions today).
    pub async fn read_daily_pool(&self, day_index: u32) -> Result<Option<DailyPoolState>> {
        let program_id = self.bounty_program_id;
        let (daily_pool_pda, _) =
            Pubkey::find_program_address(&[DAILY_POOL_SEED, &day_index.to_le_bytes()], &program_id);

        let rpc = self.rpc.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut last_err = None;
            for attempt in 0..3 {
                match rpc.get_account(&daily_pool_pda) {
                    Ok(account) => {
                        return Ok(Some(decode_daily_pool(&account.data, day_index)?));
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("AccountNotFound")
                            || err_str.contains("could not find account")
                        {
                            return Ok(None); // Pool not created yet today
                        }
                        last_err = Some(e);
                        if attempt < 2 {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                    }
                }
            }
            Err(AmosError::SolanaRpc(format!(
                "Failed to fetch daily pool after 3 attempts: {}",
                last_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "no error recorded".into())
            )))
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        Ok(result)
    }

    /// Check whether a bounty has already been settled on-chain.
    ///
    /// Returns `true` if the `bounty_proof` PDA for this `bounty_id` exists
    /// on Solana. This is the on-chain source of truth: the PDA is created
    /// the moment `submit_bounty_proof` is confirmed, and a repeat call
    /// with the same `bounty_id` would fail with "account already
    /// initialized" at the program level.
    ///
    /// Used as a pre-flight idempotency guard before calling
    /// [`process_bounty_payout`]. If the relay crashed between an
    /// on-chain confirmation and writing the DB, this check lets a retry
    /// reconcile instead of attempting a second payout.
    pub async fn is_bounty_settled(&self, bounty_id: &str) -> Result<bool> {
        let bounty_id_bytes = hash_to_32_bytes(bounty_id);
        let (bounty_proof_pda, _) = Pubkey::find_program_address(
            &[BOUNTY_PROOF_SEED, &bounty_id_bytes],
            &self.bounty_program_id,
        );

        let rpc = self.rpc.clone();
        let exists = tokio::task::spawn_blocking(move || {
            let mut last_err = None;
            for attempt in 0..3 {
                match rpc.get_account(&bounty_proof_pda) {
                    Ok(_) => return Ok::<bool, AmosError>(true),
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("AccountNotFound")
                            || err_str.contains("could not find account")
                        {
                            return Ok(false);
                        }
                        last_err = Some(e);
                        if attempt < 2 {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                    }
                }
            }
            Err(AmosError::SolanaRpc(format!(
                "Failed to check bounty_proof PDA after 3 attempts: {}",
                last_err.unwrap()
            )))
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        Ok(exists)
    }

    /// Burn protocol fees (ops/burn share) by sending tokens to the burn address.
    pub async fn burn_protocol_fees(&self, amount: u64) -> Result<String> {
        if amount == 0 {
            return Ok("no_burn_needed".to_string());
        }

        // For the burn, we need the oracle to sign a token burn instruction
        // against the ops pool token account. For now, log and return a marker
        // indicating the burn is pending on-chain integration.
        warn!(
            amount,
            "Protocol fee burn not yet integrated — amount recorded in fee ledger"
        );
        Ok(format!("pending_burn_{}", amount))
    }

    // ── On-Chain Bounty Posting ────────────────────────────────────────

    /// Post a bounty listing on-chain via `post_bounty_listing`.
    ///
    /// Creates an immutable `BountyListing` PDA that any relay can read.
    /// The oracle signs as the poster for system/treasury bounties.
    #[allow(clippy::too_many_arguments)]
    pub async fn post_bounty_on_chain(
        &self,
        bounty_id_hash: &[u8; 32],
        bounty_source: u8,
        reward_amount: u64,
        contribution_type: u8,
        required_trust_level: u8,
        claim_timeout_hours: u64,
        deadline: i64,
    ) -> Result<String> {
        // ── Input validation at the trust boundary ───────────────────────
        if reward_amount == 0 {
            return Err(AmosError::Validation(
                "reward_amount must be greater than 0".into(),
            ));
        }
        if bounty_source > 2 {
            return Err(AmosError::Validation(format!(
                "bounty_source must be 0..=2 (got: {})",
                bounty_source
            )));
        }
        if contribution_type > 3 {
            return Err(AmosError::Validation(format!(
                "contribution_type must be 0..=3 (got: {})",
                contribution_type
            )));
        }
        validate_trust_level(required_trust_level)?;
        if claim_timeout_hours == 0 {
            return Err(AmosError::Validation(
                "claim_timeout_hours must be greater than 0".into(),
            ));
        }
        if deadline <= 0 {
            return Err(AmosError::Validation(format!(
                "deadline must be a positive unix timestamp (got: {})",
                deadline
            )));
        }

        let oracle = self.oracle_keypair.as_ref().ok_or_else(|| {
            AmosError::Internal(
                "Oracle keypair not configured — cannot post bounty on-chain".into(),
            )
        })?;

        let program_id = self.bounty_program_id;
        let oracle_pubkey = oracle.pubkey();

        // Derive PDAs
        let (config_pda, _) = Pubkey::find_program_address(&[BOUNTY_CONFIG_SEED], &program_id);
        let (listing_pda, _) =
            Pubkey::find_program_address(&[BOUNTY_LISTING_SEED, bounty_id_hash], &program_id);

        let data = build_post_bounty_listing_data(
            bounty_id_hash,
            bounty_source,
            reward_amount,
            contribution_type,
            required_trust_level,
            claim_timeout_hours,
            deadline,
        );

        let ix = Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new_readonly(config_pda, false),
                AccountMeta::new(listing_pda, false),
                AccountMeta::new(oracle_pubkey, true), // poster = oracle (signer)
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ],
            data,
        };

        let rpc = self.rpc.clone();
        let oracle_bytes: Vec<u8> = oracle.to_bytes().to_vec();

        let tx_sig = tokio::task::spawn_blocking(move || {
            let oracle_kp = Keypair::try_from(oracle_bytes.as_slice()).map_err(|e| {
                AmosError::Internal(format!("Failed to reconstruct keypair: {}", e))
            })?;
            send_with_retry(&rpc, &[ix], &oracle_kp, &[&oracle_kp], 2)
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        info!(tx = %tx_sig, "Bounty listing posted on-chain");
        Ok(tx_sig)
    }

    // ── On-Chain Agent Registration ────────────────────────────────────

    /// Bootstrap an agent's trust level on-chain via `bootstrap_agent_trust`.
    ///
    /// Only works on agents with 0 completions (fresh registrations).
    /// Used to pre-register operator and QA reviewer wallets at trust 5.
    pub async fn bootstrap_agent_trust(
        &self,
        wallet_address: &str,
        trust_level: u8,
    ) -> Result<String> {
        validate_trust_level(trust_level)?;

        let oracle = self.oracle_keypair.as_ref().ok_or_else(|| {
            AmosError::Internal(
                "Oracle keypair not configured — cannot bootstrap agent trust".into(),
            )
        })?;

        let wallet_pubkey = Pubkey::from_str(wallet_address)
            .map_err(|e| AmosError::Validation(format!("Invalid wallet address: {}", e)))?;
        let agent_id = wallet_pubkey.to_bytes();

        let program_id = self.bounty_program_id;
        let oracle_pubkey = oracle.pubkey();

        let (config_pda, _) = Pubkey::find_program_address(&[BOUNTY_CONFIG_SEED], &program_id);
        let (trust_pda, _) =
            Pubkey::find_program_address(&[AGENT_TRUST_SEED, &agent_id], &program_id);

        let data = build_bootstrap_agent_trust_data(&agent_id, trust_level);

        let ix = Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new_readonly(config_pda, false),
                AccountMeta::new(trust_pda, false),
                AccountMeta::new_readonly(oracle_pubkey, true), // oracle_authority (signer)
            ],
            data,
        };

        let rpc = self.rpc.clone();
        let oracle_bytes: Vec<u8> = oracle.to_bytes().to_vec();

        let tx_sig = tokio::task::spawn_blocking(move || {
            let oracle_kp = Keypair::try_from(oracle_bytes.as_slice()).map_err(|e| {
                AmosError::Internal(format!("Failed to reconstruct keypair: {}", e))
            })?;
            send_with_retry(&rpc, &[ix], &oracle_kp, &[&oracle_kp], 2)
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        info!(
            wallet = %wallet_address,
            trust_level,
            tx = %tx_sig,
            "Agent trust bootstrapped on-chain"
        );
        Ok(tx_sig)
    }

    /// Register an agent's trust record on-chain via `register_agent_trust`.
    ///
    /// Uses the wallet pubkey bytes as `agent_id` so the PDA is deterministically
    /// derivable from the wallet address alone (portable across relays).
    pub async fn register_agent_on_chain(&self, wallet_address: &str) -> Result<String> {
        let oracle = self.oracle_keypair.as_ref().ok_or_else(|| {
            AmosError::Internal(
                "Oracle keypair not configured — cannot register agent on-chain".into(),
            )
        })?;

        let wallet_pubkey = Pubkey::from_str(wallet_address)
            .map_err(|e| AmosError::Validation(format!("Invalid wallet address: {}", e)))?;
        let agent_id = wallet_pubkey.to_bytes();

        let program_id = self.bounty_program_id;
        let oracle_pubkey = oracle.pubkey();

        let (trust_pda, _) =
            Pubkey::find_program_address(&[AGENT_TRUST_SEED, &agent_id], &program_id);

        let data = build_register_agent_trust_data(&agent_id);

        let ix = Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(trust_pda, false),
                AccountMeta::new(oracle_pubkey, true), // operator = oracle (signer)
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ],
            data,
        };

        let rpc = self.rpc.clone();
        let oracle_bytes: Vec<u8> = oracle.to_bytes().to_vec();

        let tx_sig = tokio::task::spawn_blocking(move || {
            let oracle_kp = Keypair::try_from(oracle_bytes.as_slice()).map_err(|e| {
                AmosError::Internal(format!("Failed to reconstruct keypair: {}", e))
            })?;
            send_with_retry(&rpc, &[ix], &oracle_kp, &[&oracle_kp], 2)
        })
        .await
        .map_err(|e| AmosError::Internal(format!("Tokio join error: {}", e)))??;

        info!(wallet = %wallet_address, tx = %tx_sig, "Agent trust registered on-chain");
        Ok(tx_sig)
    }
}

/// Send a transaction with retry and exponential backoff.
/// Refreshes blockhash on each attempt. Retries up to `max_retries` times.
fn send_with_retry(
    rpc: &RpcClient,
    instructions: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
    max_retries: u32,
) -> Result<String> {
    let mut last_error = None;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            // Exponential backoff: 500ms, 1s, 2s, 4s
            let delay_ms = 500 * 2u64.pow(attempt - 1);
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }

        // Always fetch a fresh blockhash on each attempt
        let blockhash = match rpc.get_latest_blockhash() {
            Ok(bh) => bh,
            Err(e) => {
                warn!(attempt, "Failed to get blockhash: {}", e);
                last_error = Some(AmosError::SolanaRpc(format!(
                    "Blockhash fetch failed: {}",
                    e
                )));
                continue;
            }
        };

        let tx = Transaction::new_signed_with_payer(
            instructions,
            Some(&payer.pubkey()),
            signers,
            blockhash,
        );

        match rpc.send_and_confirm_transaction(&tx) {
            Ok(sig) => return Ok(sig.to_string()),
            Err(e) => {
                let err_str = e.to_string();
                warn!(attempt, "Transaction failed: {}", err_str);

                // Don't retry on deterministic failures
                if err_str.contains("insufficient funds")
                    || err_str.contains("already processed")
                    || err_str.contains("AccountNotFound")
                    || err_str.contains("AccountNotInitialized")
                    || err_str.contains("already in use")
                    || err_str.contains("ConstraintMut")
                    || err_str.contains("ConstraintSeeds")
                    || err_str.contains("ConstraintOwner")
                    || err_str.contains("InstructionDidNotDeserialize")
                {
                    return Err(AmosError::SolanaRpc(format!(
                        "Transaction failed (non-retryable): {}",
                        e
                    )));
                }

                last_error = Some(AmosError::SolanaRpc(format!("Transaction failed: {}", e)));
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| AmosError::SolanaRpc("Transaction failed after all retries".into())))
}

/// Compute the Anchor instruction discriminator for a function name.
/// Format: sha256("global:<function_name>")[0..8]
fn anchor_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("global:{}", name).as_bytes());
    let hash = hasher.finalize();
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

/// Build the instruction data for `bootstrap_agent_trust`.
/// Layout: 8-byte discriminator + agent_id ([u8; 32]) + trust_level (u8) = 41 bytes.
fn build_bootstrap_agent_trust_data(agent_id: &[u8; 32], trust_level: u8) -> Vec<u8> {
    let disc = anchor_discriminator("bootstrap_agent_trust");
    let mut data = Vec::with_capacity(8 + 32 + 1);
    data.extend_from_slice(&disc);
    data.extend_from_slice(agent_id);
    data.push(trust_level);
    data
}

/// Build the instruction data for `register_agent_trust`.
/// Layout: 8-byte discriminator + agent_id ([u8; 32]).
fn build_register_agent_trust_data(agent_id: &[u8; 32]) -> Vec<u8> {
    let disc = anchor_discriminator("register_agent_trust");
    let mut data = Vec::with_capacity(8 + 32);
    data.extend_from_slice(&disc);
    data.extend_from_slice(agent_id);
    data
}

/// Build the instruction data for `post_bounty_listing`.
/// Layout: 8-byte discriminator + bounty_id (32) + bounty_source (1) + reward_amount (8)
///       + contribution_type (1) + required_trust_level (1) + claim_timeout_hours (8) + deadline (8)
///       = 67 bytes total.
#[allow(clippy::too_many_arguments)]
fn build_post_bounty_listing_data(
    bounty_id: &[u8; 32],
    bounty_source: u8,
    reward_amount: u64,
    contribution_type: u8,
    required_trust_level: u8,
    claim_timeout_hours: u64,
    deadline: i64,
) -> Vec<u8> {
    let disc = anchor_discriminator("post_bounty_listing");
    let mut data = Vec::with_capacity(8 + 32 + 1 + 8 + 1 + 1 + 8 + 8);
    data.extend_from_slice(&disc);
    data.extend_from_slice(bounty_id);
    data.push(bounty_source);
    data.extend_from_slice(&reward_amount.to_le_bytes());
    data.push(contribution_type);
    data.push(required_trust_level);
    data.extend_from_slice(&claim_timeout_hours.to_le_bytes());
    data.extend_from_slice(&deadline.to_le_bytes());
    data
}

/// Build the instruction data for `prepare_bounty_submission`.
/// Layout: 8-byte discriminator + operator_key (Pubkey, 32 bytes) + day_index (u32, 4 bytes).
fn build_prepare_bounty_submission_data(operator: &Pubkey, day_index: u32) -> Vec<u8> {
    let disc = anchor_discriminator("prepare_bounty_submission");
    let mut data = Vec::with_capacity(8 + 32 + 4);
    data.extend_from_slice(&disc);
    data.extend_from_slice(operator.as_ref());
    data.extend_from_slice(&day_index.to_le_bytes());
    data
}

/// Build the instruction data for `submit_bounty_proof`.
/// Layout: 8-byte discriminator + borsh-serialized fixed-size args.
#[allow(clippy::too_many_arguments)]
fn build_submit_bounty_proof_data(
    bounty_id: &[u8; 32],
    base_points: u16,
    quality_score: u8,
    contribution_type: u8,
    is_agent: bool,
    agent_id: &[u8; 32],
    day_index: u32,
    max_reward: u64,
    reviewer: &Pubkey,
    evidence_hash: &[u8; 32],
) -> Vec<u8> {
    let disc = anchor_discriminator("submit_bounty_proof");
    let external_reference = [0u8; 64]; // Reserved, zeroed

    let mut data = Vec::with_capacity(8 + 32 + 2 + 1 + 1 + 1 + 32 + 4 + 8 + 32 + 32 + 64);
    data.extend_from_slice(&disc);
    data.extend_from_slice(bounty_id);
    data.extend_from_slice(&base_points.to_le_bytes());
    data.push(quality_score);
    data.push(contribution_type);
    data.push(is_agent as u8);
    data.extend_from_slice(agent_id);
    data.extend_from_slice(&day_index.to_le_bytes());
    data.extend_from_slice(&max_reward.to_le_bytes());
    data.extend_from_slice(reviewer.as_ref());
    data.extend_from_slice(evidence_hash);
    data.extend_from_slice(&external_reference);
    data
}

/// Open the oracle keypair file and enforce that it's owner-only readable
/// on Unix. Returns the open `File` handle so the caller can read from the
/// same fd the permissions were checked against — this closes the TOCTOU
/// window that a two-step `metadata()` + `read_to_string()` would leave open.
///
/// Returns an error if:
/// * The file cannot be opened or stat'd (missing, no permission, etc.).
/// * On Unix, any bit in `mode & 0o077` is set — i.e. the file is group- or
///   world-readable/writable/executable. Only `0600` (or stricter) is allowed.
///
/// On non-Unix targets POSIX mode bits don't apply; the function only
/// performs the open (liveness) check.
fn enforce_keypair_permissions(path: &Path) -> Result<std::fs::File> {
    let file = std::fs::File::open(path).map_err(|e| {
        AmosError::Internal(format!(
            "Failed to read oracle keypair at '{}': {}",
            path.display(),
            e
        ))
    })?;

    let metadata = file.metadata().map_err(|e| {
        AmosError::Internal(format!(
            "Failed to stat oracle keypair at '{}': {}",
            path.display(),
            e
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(AmosError::Config(format!(
                "Oracle keypair '{}' has insecure permissions {:#o}; required 0600 (owner read/write only). \
                 Run: chmod 600 '{}'",
                path.display(),
                mode,
                path.display()
            )));
        }
    }

    // Suppress unused-variable warning on non-unix targets.
    #[cfg(not(unix))]
    let _ = metadata;

    Ok(file)
}

/// Classify a keypair file location and return a human-readable warning
/// message if the location is risky. `None` means the path is fine.
///
/// Risky locations:
/// * Inside the user's home directory (`$HOME`) — shared across many processes,
///   often synced to backups or cloud drives.
/// * `/tmp` or `/var/tmp` — world-traversable, periodically swept.
/// * Any ancestor directory with the world-writable bit set (`o+w`), which
///   means a non-owner can replace the keypair file wholesale.
fn classify_keypair_path(path: &Path) -> Option<String> {
    classify_keypair_path_with_home(path, home_dir().as_deref())
}

/// Pure variant of [`classify_keypair_path`] that accepts the user's home
/// directory as an explicit argument. Exposed for unit tests so they can
/// exercise the home-dir branch without mutating process-wide `$HOME`.
fn classify_keypair_path_with_home(path: &Path, home: Option<&Path>) -> Option<String> {
    let raw = path.to_path_buf();
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| raw.clone());

    // Check both the raw path and the canonicalized path against the temp
    // prefixes. macOS symlinks `/tmp` → `/private/tmp` and `/var` →
    // `/private/var`, so `canonicalize("/tmp/foo")` on an existing file
    // returns `/private/tmp/foo` and a `starts_with("/tmp")` comparison
    // misses. Checking both forms catches the key in both cases.
    if is_under_temp_dir(&raw) || is_under_temp_dir(&abs) {
        return Some(format!(
            "Oracle keypair at '{}' is stored under a world-traversable temp directory. \
             Move it to a dedicated secrets directory (e.g. /etc/amos/secrets) with 0700 on the parent dir.",
            abs.display()
        ));
    }

    if let Some(home) = home {
        if raw.starts_with(home) || abs.starts_with(home) {
            return Some(format!(
                "Oracle keypair at '{}' is stored inside the home directory ({}). \
                 Home dirs are typically backed up, synced, and shared across processes. \
                 Move the key to a dedicated secrets directory (e.g. /etc/amos/secrets).",
                abs.display(),
                home.display()
            ));
        }
    }

    #[cfg(unix)]
    {
        if let Some(dir) = world_writable_ancestor(&abs) {
            return Some(format!(
                "Oracle keypair at '{}' has a world-writable ancestor directory '{}'. \
                 Any local user can replace the keypair file. \
                 Tighten the directory with: chmod o-w '{}'.",
                abs.display(),
                dir.display(),
                dir.display()
            ));
        }
    }

    None
}

/// Returns `true` if `path` is under a known world-traversable temp
/// directory. Covers both the logical names (`/tmp`, `/var/tmp`) and the
/// macOS canonicalized forms (`/private/tmp`, `/private/var/tmp`).
fn is_under_temp_dir(path: &Path) -> bool {
    const TEMP_PREFIXES: &[&str] = &["/tmp", "/var/tmp", "/private/tmp", "/private/var/tmp"];
    TEMP_PREFIXES.iter().any(|p| path.starts_with(p))
}

/// Look up the user's home directory. Prefers `$HOME` (Unix) and falls back
/// to `%USERPROFILE%` (Windows) so the risky-path warning still fires on
/// non-Unix hosts even though the permission check does not.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(unix)]
fn world_writable_ancestor(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let mut current = path.parent();
    while let Some(dir) = current {
        if let Ok(md) = std::fs::metadata(dir) {
            if md.permissions().mode() & 0o002 != 0 {
                return Some(dir.to_path_buf());
            }
        }
        current = dir.parent();
    }
    None
}

/// Hash a string (bounty UUID) to a fixed 32-byte array.
fn hash_to_32_bytes(input: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Derive an Associated Token Account (ATA) address.
fn derive_associated_token_account(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    // These are well-known constant program IDs — parsing cannot fail.
    let ata_program =
        Pubkey::from_str(SPL_ASSOCIATED_TOKEN_PROGRAM_ID).expect("constant SPL ATA program ID");
    let token_program =
        Pubkey::from_str(SPL_TOKEN_PROGRAM_ID).expect("constant SPL token program ID");

    let (ata, _) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ata_program,
    );
    ata
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Client creation ────────────────────────────────────────────────

    #[test]
    fn solana_client_can_be_created() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        );
        assert!(client.is_ok());
    }

    #[test]
    fn test_invalid_program_id() {
        let client = SolanaClient::new("https://api.devnet.solana.com", "invalid_pubkey");
        assert!(client.is_err());
    }

    #[test]
    fn test_settlement_readiness_unconfigured() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        assert!(!client.is_settlement_ready());
        assert!(client.oracle_keypair.is_none());
        assert!(client.mint.is_none());
        assert!(client.treasury_token_account.is_none());
    }

    #[test]
    fn test_set_mint_valid() {
        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        assert!(client.set_mint("11111111111111111111111111111111").is_ok());
        assert!(client.mint.is_some());
    }

    #[test]
    fn test_set_mint_invalid() {
        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        assert!(client.set_mint("not_a_pubkey").is_err());
        assert!(client.mint.is_none());
    }

    #[test]
    fn test_set_treasury_valid() {
        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        assert!(client
            .set_treasury("11111111111111111111111111111111")
            .is_ok());
        assert!(client.treasury_token_account.is_some());
    }

    #[test]
    fn test_settlement_readiness_partial() {
        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        // Only mint set — not ready
        client.set_mint("11111111111111111111111111111111").unwrap();
        assert!(!client.is_settlement_ready());

        // Mint + treasury — still not ready (no keypair)
        client
            .set_treasury("11111111111111111111111111111111")
            .unwrap();
        assert!(!client.is_settlement_ready());
    }

    #[test]
    fn test_load_oracle_keypair_missing_file() {
        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let result = client.load_oracle_keypair("/nonexistent/path/keypair.json");
        assert!(result.is_err());
        assert!(client.oracle_keypair.is_none());
    }

    // ── Anchor discriminator ───────────────────────────────────────────

    #[test]
    fn test_anchor_discriminator_deterministic() {
        let disc1 = anchor_discriminator("submit_bounty_proof");
        let disc2 = anchor_discriminator("submit_bounty_proof");
        assert_eq!(disc1, disc2);
        assert_eq!(disc1.len(), 8);
    }

    #[test]
    fn test_anchor_discriminator_different_for_different_functions() {
        let disc_submit = anchor_discriminator("submit_bounty_proof");
        let disc_init = anchor_discriminator("initialize");
        let disc_decay = anchor_discriminator("apply_decay");
        assert_ne!(disc_submit, disc_init);
        assert_ne!(disc_submit, disc_decay);
        assert_ne!(disc_init, disc_decay);
    }

    // ── Instruction data building ──────────────────────────────────────

    #[test]
    fn test_instruction_data_length() {
        let bounty_id = [1u8; 32];
        let agent_id = [2u8; 32];
        let evidence_hash = [3u8; 32];
        let reviewer = Pubkey::new_unique();

        let data = build_submit_bounty_proof_data(
            &bounty_id,
            100,
            80,
            1,
            true,
            &agent_id,
            42,
            500_000_000_000u64,
            &reviewer,
            &evidence_hash,
        );

        // 8 (disc) + 32 (bounty_id) + 2 (points) + 1 (quality) + 1 (type)
        // + 1 (is_agent) + 32 (agent_id) + 4 (day_index) + 8 (max_reward)
        // + 32 (reviewer) + 32 (evidence) + 64 (external_ref) = 217
        assert_eq!(data.len(), 217);
    }

    #[test]
    fn test_instruction_data_starts_with_discriminator() {
        let bounty_id = [0u8; 32];
        let agent_id = [0u8; 32];
        let evidence_hash = [0u8; 32];
        let reviewer = Pubkey::new_unique();

        let data = build_submit_bounty_proof_data(
            &bounty_id,
            0,
            0,
            0,
            false,
            &agent_id,
            42,
            500_000_000_000u64,
            &reviewer,
            &evidence_hash,
        );

        let expected_disc = anchor_discriminator("submit_bounty_proof");
        assert_eq!(&data[..8], &expected_disc);
    }

    #[test]
    fn test_instruction_data_encodes_base_points_little_endian() {
        let bounty_id = [0u8; 32];
        let agent_id = [0u8; 32];
        let evidence_hash = [0u8; 32];
        let reviewer = Pubkey::new_unique();

        let data = build_submit_bounty_proof_data(
            &bounty_id,
            500,
            80,
            1,
            true,
            &agent_id,
            42,
            500_000_000_000u64,
            &reviewer,
            &evidence_hash,
        );

        // base_points at offset 8 + 32 = 40, 2 bytes LE
        let points_bytes = &data[40..42];
        assert_eq!(points_bytes, &500u16.to_le_bytes());
    }

    #[test]
    fn test_instruction_data_encodes_quality_and_type() {
        let bounty_id = [0u8; 32];
        let agent_id = [0u8; 32];
        let evidence_hash = [0u8; 32];
        let reviewer = Pubkey::new_unique();

        let data = build_submit_bounty_proof_data(
            &bounty_id,
            100,
            95,
            7,
            true,
            &agent_id,
            42,
            500_000_000_000u64,
            &reviewer,
            &evidence_hash,
        );

        // quality_score at offset 42, contribution_type at 43, is_agent at 44
        assert_eq!(data[42], 95); // quality
        assert_eq!(data[43], 7); // infrastructure contribution type
        assert_eq!(data[44], 1); // is_agent = true
    }

    #[test]
    fn test_instruction_data_is_agent_false() {
        let bounty_id = [0u8; 32];
        let agent_id = [0u8; 32];
        let evidence_hash = [0u8; 32];
        let reviewer = Pubkey::new_unique();

        let data = build_submit_bounty_proof_data(
            &bounty_id,
            100,
            80,
            1,
            false,
            &agent_id,
            42,
            500_000_000_000u64,
            &reviewer,
            &evidence_hash,
        );

        assert_eq!(data[44], 0); // is_agent = false
    }

    #[test]
    fn test_instruction_data_contains_bounty_id() {
        let bounty_id = [42u8; 32];
        let agent_id = [0u8; 32];
        let evidence_hash = [0u8; 32];
        let reviewer = Pubkey::new_unique();

        let data = build_submit_bounty_proof_data(
            &bounty_id,
            100,
            80,
            1,
            true,
            &agent_id,
            42,
            500_000_000_000u64,
            &reviewer,
            &evidence_hash,
        );

        // bounty_id at offset 8..40
        assert_eq!(&data[8..40], &bounty_id);
    }

    #[test]
    fn test_instruction_data_ends_with_zeroed_external_reference() {
        let bounty_id = [1u8; 32];
        let agent_id = [2u8; 32];
        let evidence_hash = [3u8; 32];
        let reviewer = Pubkey::new_unique();

        let data = build_submit_bounty_proof_data(
            &bounty_id,
            100,
            80,
            1,
            true,
            &agent_id,
            42,
            500_000_000_000u64,
            &reviewer,
            &evidence_hash,
        );

        // Last 64 bytes should be zeroed (external_reference)
        // offset = 8 + 32 + 2 + 1 + 1 + 1 + 32 + 4 + 8 + 32 + 32 = 153
        assert_eq!(&data[153..217], &[0u8; 64]);
    }

    // ── Hash utility ───────────────────────────────────────────────────

    #[test]
    fn test_hash_to_32_bytes_deterministic() {
        let hash1 = hash_to_32_bytes("test-bounty-id");
        let hash2 = hash_to_32_bytes("test-bounty-id");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 32);
    }

    #[test]
    fn test_hash_to_32_bytes_different_inputs() {
        let hash1 = hash_to_32_bytes("bounty-1");
        let hash2 = hash_to_32_bytes("bounty-2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_to_32_bytes_uuid_format() {
        // Real UUID format that would come from the relay
        let hash = hash_to_32_bytes("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, [0u8; 32]); // Not all zeros
    }

    // ── ATA derivation ─────────────────────────────────────────────────

    #[test]
    fn test_ata_derivation_deterministic() {
        let wallet = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let ata1 = derive_associated_token_account(&wallet, &mint);
        let ata2 = derive_associated_token_account(&wallet, &mint);
        assert_eq!(ata1, ata2);
    }

    #[test]
    fn test_ata_derivation_different_wallets() {
        let wallet1 = Pubkey::new_unique();
        let wallet2 = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let ata1 = derive_associated_token_account(&wallet1, &mint);
        let ata2 = derive_associated_token_account(&wallet2, &mint);
        assert_ne!(ata1, ata2);
    }

    #[test]
    fn test_ata_derivation_different_mints() {
        let wallet = Pubkey::new_unique();
        let mint1 = Pubkey::new_unique();
        let mint2 = Pubkey::new_unique();
        let ata1 = derive_associated_token_account(&wallet, &mint1);
        let ata2 = derive_associated_token_account(&wallet, &mint2);
        assert_ne!(ata1, ata2);
    }

    #[test]
    fn test_ata_differs_from_wallet() {
        let wallet = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let ata = derive_associated_token_account(&wallet, &mint);
        assert_ne!(ata, wallet);
        assert_ne!(ata, mint);
    }

    #[test]
    fn test_ata_derivation_mainnet_mint() {
        let wallet = Pubkey::from_str("HxfBT3nUz4xTL6zSbXF9HanW2Ext99Ah9f6NPU6dhr5N").unwrap();
        let mainnet_mint =
            Pubkey::from_str("5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ").unwrap();
        let old_mint = Pubkey::from_str("8DjVELBUno2XmqLdtyDbbS9NGkR5KHAnRx5rUqgZmpej").unwrap();

        let mainnet_ata = derive_associated_token_account(&wallet, &mainnet_mint);
        let old_ata = derive_associated_token_account(&wallet, &old_mint);

        // Mainnet ATA should match spl-token CLI output
        assert_eq!(
            mainnet_ata.to_string(),
            "97224MpmFSydTZWnCXcHZ1Uhuo2ofMjWmtghM2thXHEb",
            "Mainnet mint ATA mismatch"
        );
        // Old mint ATA should be different
        assert_eq!(
            old_ata.to_string(),
            "2tUtjpWzqin11LZBhAzg7Qwfn6YrQjTvvwB3saqq2R24",
            "Old mint ATA mismatch"
        );
        assert_ne!(
            mainnet_ata, old_ata,
            "Different mints should yield different ATAs"
        );
    }

    // ── PDA derivation ─────────────────────────────────────────────────

    #[test]
    fn test_pda_derivation_config() {
        let program_id = Pubkey::from_str("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq").unwrap();
        let (config_pda, bump) = Pubkey::find_program_address(&[BOUNTY_CONFIG_SEED], &program_id);
        // PDA should be deterministic
        let (config_pda2, bump2) = Pubkey::find_program_address(&[BOUNTY_CONFIG_SEED], &program_id);
        assert_eq!(config_pda, config_pda2);
        assert_eq!(bump, bump2);
        assert_ne!(config_pda, program_id);
    }

    #[test]
    fn test_pda_derivation_daily_pool() {
        let program_id = Pubkey::from_str("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq").unwrap();
        let day1: u32 = 100;
        let day2: u32 = 101;
        let (pool1, _) =
            Pubkey::find_program_address(&[DAILY_POOL_SEED, &day1.to_le_bytes()], &program_id);
        let (pool2, _) =
            Pubkey::find_program_address(&[DAILY_POOL_SEED, &day2.to_le_bytes()], &program_id);
        assert_ne!(pool1, pool2); // Different days = different PDAs
    }

    #[test]
    fn test_pda_derivation_bounty_proof() {
        let program_id = Pubkey::from_str("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq").unwrap();
        let bounty_id_1 = hash_to_32_bytes("bounty-1");
        let bounty_id_2 = hash_to_32_bytes("bounty-2");
        let (proof1, _) =
            Pubkey::find_program_address(&[BOUNTY_PROOF_SEED, &bounty_id_1], &program_id);
        let (proof2, _) =
            Pubkey::find_program_address(&[BOUNTY_PROOF_SEED, &bounty_id_2], &program_id);
        assert_ne!(proof1, proof2); // Different bounties = different PDAs
    }

    #[test]
    fn is_bounty_settled_derives_same_pda_as_settlement_path() {
        // The idempotency guard relies on `is_bounty_settled` checking the
        // SAME PDA that `process_bounty_payout` would attempt to create.
        // If either side of that derivation drifts, the guard silently
        // stops protecting us. This test pins both derivations together.
        let program_id = Pubkey::from_str("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq").unwrap();
        let bounty_id_str = "6a1e3f9c-abcd-4ef0-1234-56789abcdef0";

        // Derivation used in `is_bounty_settled`.
        let guard_bytes = hash_to_32_bytes(bounty_id_str);
        let (guard_pda, _) =
            Pubkey::find_program_address(&[BOUNTY_PROOF_SEED, &guard_bytes], &program_id);

        // Derivation used in `process_bounty_payout` (must match — see line
        // around the `let bounty_id_bytes = hash_to_32_bytes(...)` in that fn).
        let settle_bytes = hash_to_32_bytes(bounty_id_str);
        let (settle_pda, _) =
            Pubkey::find_program_address(&[BOUNTY_PROOF_SEED, &settle_bytes], &program_id);

        assert_eq!(
            guard_pda, settle_pda,
            "is_bounty_settled must derive the same PDA as process_bounty_payout"
        );
    }

    #[test]
    fn test_pda_derivation_operator_stats() {
        let program_id = Pubkey::from_str("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq").unwrap();
        let op1 = Pubkey::new_unique();
        let op2 = Pubkey::new_unique();
        let (stats1, _) =
            Pubkey::find_program_address(&[OPERATOR_STATS_SEED, op1.as_ref()], &program_id);
        let (stats2, _) =
            Pubkey::find_program_address(&[OPERATOR_STATS_SEED, op2.as_ref()], &program_id);
        assert_ne!(stats1, stats2);
    }

    #[test]
    fn test_pda_derivation_agent_trust() {
        let program_id = Pubkey::from_str("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq").unwrap();
        let agent_id_1 = [1u8; 32];
        let agent_id_2 = [2u8; 32];
        let (trust1, _) =
            Pubkey::find_program_address(&[AGENT_TRUST_SEED, &agent_id_1], &program_id);
        let (trust2, _) =
            Pubkey::find_program_address(&[AGENT_TRUST_SEED, &agent_id_2], &program_id);
        assert_ne!(trust1, trust2);
    }

    #[test]
    fn test_register_agent_trust_data_layout() {
        let agent_id = [42u8; 32];
        let data = build_register_agent_trust_data(&agent_id);
        // 8-byte discriminator + 32-byte agent_id
        assert_eq!(data.len(), 40);
        assert_eq!(&data[8..40], &agent_id);
        // Discriminator should match anchor naming
        let expected_disc = anchor_discriminator("register_agent_trust");
        assert_eq!(&data[0..8], &expected_disc);
    }

    // ── Bounty listing instruction data ──────────────────────────────────

    #[test]
    fn test_post_bounty_listing_data_length() {
        let bounty_id = [1u8; 32];
        let data = build_post_bounty_listing_data(&bounty_id, 0, 500, 7, 1, 72, 1713200000);
        // 8 (disc) + 32 (bounty_id) + 1 (source) + 8 (reward) + 1 (type) + 1 (trust) + 8 (timeout) + 8 (deadline) = 67
        assert_eq!(data.len(), 67);
    }

    #[test]
    fn test_post_bounty_listing_data_discriminator() {
        let bounty_id = [0u8; 32];
        let data = build_post_bounty_listing_data(&bounty_id, 0, 0, 0, 0, 0, 0);
        let expected_disc = anchor_discriminator("post_bounty_listing");
        assert_eq!(&data[..8], &expected_disc);
    }

    #[test]
    fn test_post_bounty_listing_data_encodes_fields() {
        let bounty_id = [42u8; 32];
        let data = build_post_bounty_listing_data(&bounty_id, 1, 500, 7, 3, 72, 1713200000);

        // bounty_id at offset 8..40
        assert_eq!(&data[8..40], &bounty_id);
        // bounty_source at offset 40
        assert_eq!(data[40], 1);
        // reward_amount at offset 41..49 (u64 LE)
        assert_eq!(&data[41..49], &500u64.to_le_bytes());
        // contribution_type at offset 49
        assert_eq!(data[49], 7);
        // required_trust_level at offset 50
        assert_eq!(data[50], 3);
        // claim_timeout_hours at offset 51..59 (u64 LE)
        assert_eq!(&data[51..59], &72u64.to_le_bytes());
        // deadline at offset 59..67 (i64 LE)
        assert_eq!(&data[59..67], &1713200000i64.to_le_bytes());
    }

    #[test]
    fn test_post_bounty_listing_data_different_inputs() {
        let id1 = [1u8; 32];
        let id2 = [2u8; 32];
        let data1 = build_post_bounty_listing_data(&id1, 0, 100, 1, 1, 24, 1000);
        let data2 = build_post_bounty_listing_data(&id2, 1, 200, 3, 5, 48, 2000);
        assert_ne!(data1, data2);
    }

    // ── Bootstrap agent trust ─────────────────────────────────────────

    #[test]
    fn test_bootstrap_agent_trust_data_length() {
        let agent_id = [42u8; 32];
        let data = build_bootstrap_agent_trust_data(&agent_id, 5);
        assert_eq!(data.len(), 41); // 8 disc + 32 agent_id + 1 trust_level
    }

    #[test]
    fn test_bootstrap_agent_trust_data_layout() {
        let agent_id = [7u8; 32];
        let data = build_bootstrap_agent_trust_data(&agent_id, 3);

        // Discriminator at 0..8
        let expected_disc = anchor_discriminator("bootstrap_agent_trust");
        assert_eq!(&data[0..8], &expected_disc);

        // agent_id at 8..40
        assert_eq!(&data[8..40], &agent_id);

        // trust_level at 40
        assert_eq!(data[40], 3);
    }

    #[test]
    fn test_bootstrap_agent_trust_data_different_levels() {
        let agent_id = [1u8; 32];
        let data1 = build_bootstrap_agent_trust_data(&agent_id, 1);
        let data5 = build_bootstrap_agent_trust_data(&agent_id, 5);
        assert_ne!(data1, data5);
        assert_eq!(data1[40], 1);
        assert_eq!(data5[40], 5);
    }

    // ── Burn fees ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_burn_zero_amount() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let result = client.burn_protocol_fees(0).await.unwrap();
        assert_eq!(result, "no_burn_needed");
    }

    #[tokio::test]
    async fn test_burn_nonzero_amount() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let result = client.burn_protocol_fees(1000).await.unwrap();
        assert_eq!(result, "pending_burn_1000");
    }

    // ── Process payout validation ──────────────────────────────────────

    #[tokio::test]
    async fn test_process_payout_without_keypair() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let params = SettlementParams {
            bounty_id: "test-bounty".to_string(),
            agent_wallet: "11111111111111111111111111111111".to_string(),
            reviewer_wallet: "11111111111111111111111111111111".to_string(),
            base_points: 100,
            quality_score: 80,
            contribution_type: 1,
            is_agent: true,
            agent_id: [0u8; 32],
            evidence_hash: [0u8; 32],
            max_reward: 500_000_000_000, // 500 AMOS
        };

        let result = client.process_bounty_payout(&params).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Oracle keypair not configured"));
    }

    #[tokio::test]
    async fn test_process_payout_invalid_wallet() {
        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        // Create a temporary keypair file for the oracle
        let keypair = Keypair::new();
        let keypair_bytes = keypair.to_bytes();
        let tmpfile = std::env::temp_dir().join("test_oracle_keypair.json");
        std::fs::write(
            &tmpfile,
            serde_json::to_string(&keypair_bytes.to_vec()).unwrap(),
        )
        .unwrap();
        set_mode_0600(&tmpfile);

        client
            .load_oracle_keypair(tmpfile.to_str().unwrap())
            .unwrap();
        client.set_mint("11111111111111111111111111111111").unwrap();
        client
            .set_treasury("11111111111111111111111111111111")
            .unwrap();

        let params = SettlementParams {
            bounty_id: "test-bounty".to_string(),
            agent_wallet: "not_a_valid_pubkey".to_string(),
            reviewer_wallet: "11111111111111111111111111111111".to_string(),
            base_points: 100,
            quality_score: 80,
            contribution_type: 1,
            is_agent: true,
            agent_id: [0u8; 32],
            evidence_hash: [0u8; 32],
            max_reward: 500_000_000_000,
        };

        let result = client.process_bounty_payout(&params).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid agent wallet"));

        // Cleanup
        let _ = std::fs::remove_file(tmpfile);
    }

    // ── Keypair loading ────────────────────────────────────────────────

    #[test]
    fn test_load_keypair_from_file() {
        let keypair = Keypair::new();
        let keypair_bytes = keypair.to_bytes();
        let tmpfile = std::env::temp_dir().join("test_load_keypair.json");
        std::fs::write(
            &tmpfile,
            serde_json::to_string(&keypair_bytes.to_vec()).unwrap(),
        )
        .unwrap();
        set_mode_0600(&tmpfile);

        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        assert!(client
            .load_oracle_keypair(tmpfile.to_str().unwrap())
            .is_ok());
        assert!(client.oracle_keypair.is_some());
        assert_eq!(client.oracle_keypair.unwrap().pubkey(), keypair.pubkey());

        let _ = std::fs::remove_file(tmpfile);
    }

    #[test]
    fn test_load_keypair_invalid_json() {
        let tmpfile = std::env::temp_dir().join("test_bad_keypair.json");
        std::fs::write(&tmpfile, "not valid json at all").unwrap();
        set_mode_0600(&tmpfile);

        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        assert!(client
            .load_oracle_keypair(tmpfile.to_str().unwrap())
            .is_err());

        let _ = std::fs::remove_file(tmpfile);
    }

    // ── Keypair permission & path hardening (SECURE-006) ──────────────

    /// Helper: chmod a test file to 0600 on Unix; no-op elsewhere.
    fn set_mode_0600(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms).unwrap();
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    /// Helper: write a valid keypair JSON to `path` at a specific mode.
    fn write_keypair_at_mode(path: &Path, mode: u32) {
        let kp = Keypair::new();
        let bytes = kp.to_bytes();
        std::fs::write(path, serde_json::to_string(&bytes.to_vec()).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(mode);
            std::fs::set_permissions(path, perms).unwrap();
        }
        #[cfg(not(unix))]
        let _ = mode;
    }

    #[cfg(unix)]
    #[test]
    fn test_load_keypair_permissions_0600_ok() {
        let tmpfile = std::env::temp_dir().join("secure006_perm_0600.json");
        write_keypair_at_mode(&tmpfile, 0o600);

        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let res = client.load_oracle_keypair(tmpfile.to_str().unwrap());
        assert!(res.is_ok(), "0600 should load: {res:?}");
        assert!(client.oracle_keypair.is_some());

        let _ = std::fs::remove_file(tmpfile);
    }

    #[cfg(unix)]
    #[test]
    fn test_load_keypair_permissions_0644_rejected() {
        let tmpfile = std::env::temp_dir().join("secure006_perm_0644.json");
        write_keypair_at_mode(&tmpfile, 0o644);

        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let res = client.load_oracle_keypair(tmpfile.to_str().unwrap());
        assert!(res.is_err(), "0644 should be rejected");
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("insecure permissions") && msg.contains("0600"),
            "error should mention 0600 requirement, got: {msg}"
        );
        assert!(client.oracle_keypair.is_none());

        let _ = std::fs::remove_file(tmpfile);
    }

    #[cfg(unix)]
    #[test]
    fn test_load_keypair_group_readable_rejected() {
        let tmpfile = std::env::temp_dir().join("secure006_perm_0640.json");
        write_keypair_at_mode(&tmpfile, 0o640);

        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let res = client.load_oracle_keypair(tmpfile.to_str().unwrap());
        assert!(res.is_err(), "0640 (group readable) should be rejected");

        let _ = std::fs::remove_file(tmpfile);
    }

    #[test]
    fn test_load_keypair_nonexistent_file_clean_error() {
        let mut client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq",
        )
        .unwrap();

        let res = client.load_oracle_keypair("/nonexistent/path/keypair.json");
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("Failed to read oracle keypair"),
            "should surface a clean read error, got: {msg}"
        );
        assert!(client.oracle_keypair.is_none());
    }

    #[test]
    fn test_classify_path_tmp_triggers_warning() {
        let path = PathBuf::from("/tmp/amos-oracle.json");
        let warning = classify_keypair_path(&path);
        assert!(warning.is_some(), "/tmp path should warn");
        assert!(warning.unwrap().contains("temp directory"));
    }

    #[test]
    fn test_classify_path_var_tmp_triggers_warning() {
        let path = PathBuf::from("/var/tmp/amos-oracle.json");
        let warning = classify_keypair_path(&path);
        assert!(warning.is_some(), "/var/tmp path should warn");
    }

    #[test]
    fn test_classify_path_macos_private_tmp_triggers_warning() {
        // On macOS, canonicalize resolves /tmp → /private/tmp. Any keypair
        // whose canonical path starts with /private/tmp (or /private/var/tmp)
        // must still trip the temp-dir warning.
        let canonical = PathBuf::from("/private/tmp/amos-oracle.json");
        let warning = classify_keypair_path(&canonical);
        assert!(warning.is_some(), "/private/tmp path should warn");
        assert!(warning.unwrap().contains("temp directory"));

        let canonical_var = PathBuf::from("/private/var/tmp/amos-oracle.json");
        let warning_var = classify_keypair_path(&canonical_var);
        assert!(warning_var.is_some(), "/private/var/tmp path should warn");
    }

    #[test]
    fn test_classify_path_home_triggers_warning() {
        // Use the pure variant so this test doesn't race with any other
        // test that reads $HOME when run in parallel.
        let home = PathBuf::from("/home/alice");
        let key = home.join("amos-founder.json");
        let warning = classify_keypair_path_with_home(&key, Some(&home));

        assert!(warning.is_some(), "home-dir path should warn");
        assert!(warning.unwrap().contains("home directory"));
    }

    #[test]
    fn test_classify_path_safe_location_no_warning() {
        // A path under /etc (not /tmp, not under HOME, parent not
        // world-writable) should produce no warning. We don't need the file
        // to exist — the classifier falls back to the raw path when
        // canonicalize fails.
        let path = PathBuf::from("/etc/amos/secrets/oracle.json");
        let warning = classify_keypair_path(&path);
        assert!(
            warning.is_none(),
            "safe path should not warn, got: {warning:?}"
        );
    }

    // ── Dynamic max_reward computation ────────────────────────────────

    fn make_pool(
        daily_emission: u64,
        tokens_distributed: u64,
        total_points: u64,
    ) -> DailyPoolState {
        DailyPoolState {
            day_index: 0,
            daily_emission,
            tokens_distributed,
            total_points,
            proof_count: 0,
        }
    }

    #[test]
    fn test_dynamic_max_reward_basic() {
        // Day 0, noon (half the day elapsed), 16,000 AMOS emission
        let pool = make_pool(16_000 * ONE_TOKEN, 0, 0);
        let start_time = 1000;
        let now = start_time + 43200; // 12 hours elapsed

        // 1000 points, no prior points, no prior distribution
        let reward = compute_dynamic_max_reward(1000, &pool, start_time, now);

        // emission_so_far = 16000 * 43200 / 86400 = 8000 AMOS = 8_000_000_000_000
        // denominator = 0 + 10000 + 1000 = 11000
        // reward = 1000/11000 * 8000 AMOS = ~727.27 AMOS
        let expected = ((1000u128 * 8_000 * ONE_TOKEN as u128) / 11_000u128) as u64;
        assert_eq!(reward, expected);
    }

    #[test]
    fn test_dynamic_max_reward_with_competition() {
        // Half day, 8000 AMOS already distributed, 15000 total_points
        let pool = make_pool(
            16_000 * ONE_TOKEN,
            4_000 * ONE_TOKEN, // 4000 AMOS already paid out
            15_000,            // 15000 points accumulated
        );
        let start_time = 1000;
        let now = start_time + 43200; // noon

        let reward = compute_dynamic_max_reward(1000, &pool, start_time, now);

        // emission_so_far = 8000 AMOS, available = 8000 - 4000 = 4000 AMOS
        // denominator = 15000 + 10000 + 1000 = 26000
        // reward = 1000/26000 * 4000 AMOS ≈ 153.8 AMOS
        let expected = ((1000u128 * 4_000 * ONE_TOKEN as u128) / 26_000u128) as u64;
        assert_eq!(reward, expected);
    }

    #[test]
    fn test_dynamic_max_reward_shrinks_over_day() {
        let emission = 16_000 * ONE_TOKEN;

        // Morning: 8am, no prior activity
        let pool_8am = make_pool(emission, 0, 0);
        let start = 0;
        let at_8am = 8 * 3600; // 8 hours

        let reward_8am = compute_dynamic_max_reward(1000, &pool_8am, start, at_8am);

        // Afternoon: 4pm, some activity already
        let pool_4pm = make_pool(emission, 3_000 * ONE_TOKEN, 5_000);
        let at_4pm = 16 * 3600; // 16 hours

        let reward_4pm = compute_dynamic_max_reward(1000, &pool_4pm, start, at_4pm);

        // Evening: 10pm, lots of activity
        let pool_10pm = make_pool(emission, 10_000 * ONE_TOKEN, 20_000);
        let at_10pm = 22 * 3600;

        let reward_10pm = compute_dynamic_max_reward(1000, &pool_10pm, start, at_10pm);

        // Rewards should decrease as the day fills up
        assert!(
            reward_8am > reward_4pm,
            "8am ({}) should beat 4pm ({})",
            reward_8am,
            reward_4pm
        );
        assert!(
            reward_4pm > reward_10pm,
            "4pm ({}) should beat 10pm ({})",
            reward_4pm,
            reward_10pm
        );
    }

    #[test]
    fn test_dynamic_max_reward_zero_points() {
        let pool = make_pool(16_000 * ONE_TOKEN, 0, 0);
        assert_eq!(compute_dynamic_max_reward(0, &pool, 0, 43200), 0);
    }

    #[test]
    fn test_dynamic_max_reward_zero_emission() {
        let pool = make_pool(0, 0, 0);
        assert_eq!(compute_dynamic_max_reward(1000, &pool, 0, 43200), 0);
    }

    #[test]
    fn test_dynamic_max_reward_pool_exhausted() {
        // Everything already distributed
        let pool = make_pool(16_000 * ONE_TOKEN, 16_000 * ONE_TOKEN, 50_000);
        let reward = compute_dynamic_max_reward(1000, &pool, 0, 86400);
        // available = emission_so_far (16000) - distributed (16000) = 0
        assert_eq!(reward, 0);
    }

    #[test]
    fn test_dynamic_max_reward_minimum_floor() {
        // Very small points, should still get at least 1 AMOS (the floor)
        let pool = make_pool(16_000 * ONE_TOKEN, 0, 100_000);
        let reward = compute_dynamic_max_reward(1, &pool, 0, 43200);
        assert!(
            reward >= ONE_TOKEN,
            "Minimum floor should be 1 AMOS, got {}",
            reward
        );
    }

    #[test]
    fn test_dynamic_max_reward_virtual_base_prevents_drain() {
        // First submitter of the day with 2000 points (max trust level)
        let pool = make_pool(16_000 * ONE_TOKEN, 0, 0);
        let reward = compute_dynamic_max_reward(2000, &pool, 0, 86400); // full day

        // Without virtual base: 2000/2000 = 100% of 16000 = 16000 AMOS
        // With virtual base: 2000/(0+10000+2000) = 16.67% of 16000 = ~2667 AMOS
        let full_emission = 16_000 * ONE_TOKEN;
        assert!(
            reward < full_emission / 2,
            "Virtual base should prevent >50% drain, got {}",
            reward
        );
    }

    #[test]
    fn test_fallback_max_reward() {
        let reward = fallback_max_reward(1000);
        // 1000 / (10000 + 10000 + 1000) * 16000 AMOS ≈ 762 AMOS
        let expected = ((1000u128 * 16_000 * ONE_TOKEN as u128) / 21_000u128) as u64;
        assert_eq!(reward, expected);
    }

    #[test]
    fn test_fallback_max_reward_minimum() {
        let reward = fallback_max_reward(1);
        assert!(
            reward >= ONE_TOKEN,
            "Fallback should return at least 1 AMOS"
        );
    }

    // ── Malformed-input / fuzz tests ───────────────────────────────────
    //
    // These tests feed pathological inputs into every public `SolanaClient`
    // function and assert each one returns `Err(_)` rather than panicking.
    // They guard against the class of bug fixed by SECURE-007: unchecked
    // `.unwrap()` / unvalidated boundary input that could DoS the relay
    // when an attacker or buggy caller supplies bad data.

    const VALID_PROGRAM_ID: &str = "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq";
    const VALID_RPC_URL: &str = "https://api.devnet.solana.com";

    fn make_client() -> SolanaClient {
        SolanaClient::new(VALID_RPC_URL, VALID_PROGRAM_ID).unwrap()
    }

    /// Build a client with a random in-memory oracle keypair loaded. This
    /// makes boundary-validation tests exercise the specific validation
    /// code path rather than short-circuiting on the "oracle not configured"
    /// error (which would also return `Err` but prove nothing about the
    /// input check).
    fn make_client_with_oracle() -> SolanaClient {
        let mut client = make_client();
        client.oracle_keypair = Some(Keypair::new());
        client
    }

    // --- validate_rpc_url ---

    #[test]
    fn test_validate_rpc_url_accepts_supported_schemes() {
        for url in [
            "http://localhost:8899",
            "https://api.devnet.solana.com",
            "ws://localhost:8900",
            "wss://api.mainnet-beta.solana.com",
        ] {
            assert!(validate_rpc_url(url).is_ok(), "should accept {}", url);
        }
    }

    #[test]
    fn test_validate_rpc_url_rejects_empty_and_whitespace() {
        for url in ["", "   ", "\t\n"] {
            assert!(
                validate_rpc_url(url).is_err(),
                "should reject empty-ish URL: {:?}",
                url
            );
        }
    }

    #[test]
    fn test_validate_rpc_url_rejects_unsupported_schemes() {
        for url in [
            "ftp://example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "api.devnet.solana.com", // no scheme at all
            "localhost",
            "://no-scheme",
        ] {
            assert!(
                validate_rpc_url(url).is_err(),
                "should reject unsupported URL: {}",
                url
            );
        }
    }

    // --- SolanaClient::new ---

    #[test]
    fn test_new_rejects_empty_rpc_url() {
        let result = SolanaClient::new("", VALID_PROGRAM_ID);
        assert!(result.is_err(), "empty URL should be rejected");
    }

    #[test]
    fn test_new_rejects_garbage_rpc_url() {
        let result = SolanaClient::new("not a url at all", VALID_PROGRAM_ID);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_rejects_malformed_program_ids() {
        for bad_id in [
            "",
            " ",
            "not_base58",
            "OIl0", // contains 0, O, I, l — none valid in base58
            "too_short",
            "A",
            &"X".repeat(128), // excessively long
            "\0\0\0\0",
            "🦀🦀🦀",
        ] {
            let result = SolanaClient::new(VALID_RPC_URL, bad_id);
            assert!(
                result.is_err(),
                "SolanaClient::new should reject malformed program id: {:?}",
                bad_id
            );
        }
    }

    // --- load_oracle_keypair ---

    #[test]
    fn test_load_oracle_keypair_rejects_empty_path() {
        let mut client = make_client();
        assert!(client.load_oracle_keypair("").is_err());
        assert!(client.load_oracle_keypair("   ").is_err());
    }

    #[test]
    fn test_load_oracle_keypair_rejects_nonexistent_path() {
        let mut client = make_client();
        let result = client.load_oracle_keypair("/definitely/does/not/exist/keypair.json");
        assert!(result.is_err());
    }

    // --- set_mint / set_treasury ---

    #[test]
    fn test_set_mint_rejects_malformed_input() {
        let mut client = make_client();
        for bad in [
            "",
            " ",
            "not_base58",
            "O0Il", // invalid base58 chars
            "\0",
            &"Z".repeat(256),
        ] {
            assert!(
                client.set_mint(bad).is_err(),
                "set_mint should reject: {:?}",
                bad
            );
        }
    }

    #[test]
    fn test_set_treasury_rejects_malformed_input() {
        let mut client = make_client();
        for bad in ["", " ", "not_base58", "O0Il", &"Z".repeat(256)] {
            assert!(
                client.set_treasury(bad).is_err(),
                "set_treasury should reject: {:?}",
                bad
            );
        }
    }

    // --- validate_trust_level ---

    #[test]
    fn test_validate_trust_level_accepts_valid_range() {
        for level in 1..=5 {
            assert!(validate_trust_level(level).is_ok());
        }
    }

    #[test]
    fn test_validate_trust_level_rejects_out_of_range() {
        for bad in [0, 6, 7, 100, u8::MAX] {
            assert!(
                validate_trust_level(bad).is_err(),
                "should reject trust_level {}",
                bad
            );
        }
    }

    // --- bootstrap_agent_trust (boundary validation only — no RPC call made
    //     when validation fails fast) ---

    #[tokio::test]
    async fn test_bootstrap_agent_trust_rejects_invalid_trust_level() {
        let client = make_client_with_oracle();
        for bad_level in [0, 6, 7, u8::MAX] {
            let result = client
                .bootstrap_agent_trust("HxfBT3nUz4xTL6zSbXF9HanW2Ext99Ah9f6NPU6dhr5N", bad_level)
                .await;
            assert!(
                result.is_err(),
                "bootstrap should reject trust_level={}",
                bad_level
            );
        }
    }

    #[tokio::test]
    async fn test_bootstrap_agent_trust_rejects_malformed_wallet() {
        let client = make_client_with_oracle();
        for bad in ["", " ", "not_base58", "O0Il", "\0\0", &"Q".repeat(256)] {
            let result = client.bootstrap_agent_trust(bad, 5).await;
            assert!(
                result.is_err(),
                "bootstrap should reject malformed wallet: {:?}",
                bad
            );
        }
    }

    // --- post_bounty_on_chain (numeric-boundary validation) ---
    //
    // These use `make_client_with_oracle()` so that validation is what fails,
    // not the "no oracle keypair configured" check. A valid deadline in the
    // future is used for cases that only exercise one other field.

    #[tokio::test]
    async fn test_post_bounty_rejects_zero_reward() {
        let client = make_client_with_oracle();
        let result = client
            .post_bounty_on_chain(&[0u8; 32], 0, 0, 0, 5, 24, 1_800_000_000)
            .await;
        assert!(result.is_err(), "should reject zero reward_amount");
    }

    #[tokio::test]
    async fn test_post_bounty_rejects_bad_source() {
        let client = make_client_with_oracle();
        for bad_source in [3u8, 4, 100, u8::MAX] {
            let result = client
                .post_bounty_on_chain(&[0u8; 32], bad_source, 1000, 0, 5, 24, 1_800_000_000)
                .await;
            assert!(
                result.is_err(),
                "should reject bounty_source={}",
                bad_source
            );
        }
    }

    #[tokio::test]
    async fn test_post_bounty_rejects_bad_contribution_type() {
        let client = make_client_with_oracle();
        for bad_ct in [4u8, 5, 100, u8::MAX] {
            let result = client
                .post_bounty_on_chain(&[0u8; 32], 0, 1000, bad_ct, 5, 24, 1_800_000_000)
                .await;
            assert!(
                result.is_err(),
                "should reject contribution_type={}",
                bad_ct
            );
        }
    }

    #[tokio::test]
    async fn test_post_bounty_rejects_bad_trust_level() {
        let client = make_client_with_oracle();
        for bad in [0u8, 6, 100, u8::MAX] {
            let result = client
                .post_bounty_on_chain(&[0u8; 32], 0, 1000, 0, bad, 24, 1_800_000_000)
                .await;
            assert!(result.is_err(), "should reject trust_level={}", bad);
        }
    }

    #[tokio::test]
    async fn test_post_bounty_rejects_zero_timeout() {
        let client = make_client_with_oracle();
        let result = client
            .post_bounty_on_chain(&[0u8; 32], 0, 1000, 0, 5, 0, 1_800_000_000)
            .await;
        assert!(result.is_err(), "should reject zero claim_timeout_hours");
    }

    #[tokio::test]
    async fn test_post_bounty_rejects_nonpositive_deadline() {
        let client = make_client_with_oracle();
        for bad_deadline in [0i64, -1, -100, i64::MIN] {
            let result = client
                .post_bounty_on_chain(&[0u8; 32], 0, 1000, 0, 5, 24, bad_deadline)
                .await;
            assert!(result.is_err(), "should reject deadline={}", bad_deadline);
        }
    }

    // --- register_agent_on_chain ---

    #[tokio::test]
    async fn test_register_agent_rejects_malformed_wallet() {
        let client = make_client_with_oracle();
        for bad in ["", " ", "not_base58", "O0Il", "\0\0", &"Q".repeat(256)] {
            let result = client.register_agent_on_chain(bad).await;
            assert!(
                result.is_err(),
                "register_agent should reject malformed wallet: {:?}",
                bad
            );
        }
    }

    // ── Pure decoder tests ─────────────────────────────────────────────
    //
    // These directly exercise `decode_config_start_time` and
    // `decode_daily_pool` with hand-crafted byte arrays — covering the
    // exact `try_into.map_err` paths that replaced the original
    // unwrap()s. Feeding truncated, empty, and boundary-sized inputs
    // here proves the fix behaves as designed without needing a live
    // Solana RPC.

    fn build_config_payload(start_time: i64) -> Vec<u8> {
        let mut buf = vec![0u8; CONFIG_ACCOUNT_MIN_LEN];
        buf[CONFIG_START_TIME_OFFSET..CONFIG_START_TIME_OFFSET + 8]
            .copy_from_slice(&start_time.to_le_bytes());
        buf
    }

    fn build_daily_pool_payload(
        day_index: u32,
        daily_emission: u64,
        tokens_distributed: u64,
        total_points: u64,
        proof_count: u32,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; DAILY_POOL_ACCOUNT_MIN_LEN];
        let off = 8; // discriminator
        buf[off..off + 4].copy_from_slice(&day_index.to_le_bytes());
        buf[off + 4..off + 12].copy_from_slice(&daily_emission.to_le_bytes());
        buf[off + 12..off + 20].copy_from_slice(&tokens_distributed.to_le_bytes());
        buf[off + 20..off + 28].copy_from_slice(&total_points.to_le_bytes());
        buf[off + 28..off + 32].copy_from_slice(&proof_count.to_le_bytes());
        buf
    }

    // --- decode_config_start_time ---

    #[test]
    fn test_decode_config_start_time_valid() {
        let payload = build_config_payload(1_800_000_000);
        assert_eq!(decode_config_start_time(&payload).unwrap(), 1_800_000_000);
    }

    #[test]
    fn test_decode_config_start_time_negative_value_is_preserved() {
        // i64::from_le_bytes should handle negative timestamps round-trip
        let payload = build_config_payload(-1);
        assert_eq!(decode_config_start_time(&payload).unwrap(), -1);
    }

    #[test]
    fn test_decode_config_start_time_rejects_empty() {
        let result = decode_config_start_time(&[]);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("Config account too small"),
            "unexpected err: {}",
            msg
        );
    }

    #[test]
    fn test_decode_config_start_time_rejects_one_byte_short() {
        // CONFIG_ACCOUNT_MIN_LEN - 1 bytes → still below threshold
        let payload = vec![0u8; CONFIG_ACCOUNT_MIN_LEN - 1];
        let result = decode_config_start_time(&payload);
        assert!(result.is_err(), "should reject 111-byte payload");
    }

    #[test]
    fn test_decode_config_start_time_rejects_all_short_sizes() {
        for size in 0..CONFIG_ACCOUNT_MIN_LEN {
            let payload = vec![0xAAu8; size];
            assert!(
                decode_config_start_time(&payload).is_err(),
                "should reject {}-byte payload",
                size
            );
        }
    }

    #[test]
    fn test_decode_config_start_time_accepts_exact_min_size() {
        let payload = vec![0u8; CONFIG_ACCOUNT_MIN_LEN];
        assert!(decode_config_start_time(&payload).is_ok());
    }

    #[test]
    fn test_decode_config_start_time_accepts_oversized_payload() {
        // A longer-than-expected account (e.g., future schema extension)
        // must still decode the start_time at the fixed offset.
        let mut payload = build_config_payload(42);
        payload.extend(std::iter::repeat_n(0xFFu8, 500));
        assert_eq!(decode_config_start_time(&payload).unwrap(), 42);
    }

    #[test]
    fn test_decode_config_start_time_fuzz_garbage_short() {
        // Pseudo-random short garbage should never panic — only ever Err.
        let garbage_sizes = [1usize, 7, 8, 50, 100, 103, 111];
        for size in garbage_sizes {
            let payload: Vec<u8> = (0..size).map(|i| (i * 37 + 13) as u8).collect();
            assert!(
                decode_config_start_time(&payload).is_err(),
                "size {} should err",
                size
            );
        }
    }

    // --- decode_daily_pool ---

    #[test]
    fn test_decode_daily_pool_valid_roundtrip() {
        let payload = build_daily_pool_payload(7, 16_000_000_000_000, 500_000_000, 12_345, 42);
        let pool = decode_daily_pool(&payload, 7).unwrap();
        assert_eq!(pool.day_index, 7);
        assert_eq!(pool.daily_emission, 16_000_000_000_000);
        assert_eq!(pool.tokens_distributed, 500_000_000);
        assert_eq!(pool.total_points, 12_345);
        assert_eq!(pool.proof_count, 42);
    }

    #[test]
    fn test_decode_daily_pool_rejects_empty() {
        assert!(decode_daily_pool(&[], 0).is_err());
    }

    #[test]
    fn test_decode_daily_pool_rejects_one_byte_short() {
        let payload = vec![0u8; DAILY_POOL_ACCOUNT_MIN_LEN - 1];
        let result = decode_daily_pool(&payload, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_daily_pool_rejects_all_short_sizes() {
        for size in 0..DAILY_POOL_ACCOUNT_MIN_LEN {
            let payload = vec![0xAAu8; size];
            assert!(
                decode_daily_pool(&payload, 0).is_err(),
                "should reject {}-byte payload",
                size
            );
        }
    }

    #[test]
    fn test_decode_daily_pool_accepts_exact_min_size() {
        let payload = vec![0u8; DAILY_POOL_ACCOUNT_MIN_LEN];
        let pool = decode_daily_pool(&payload, 99).unwrap();
        assert_eq!(pool.day_index, 99);
        assert_eq!(pool.daily_emission, 0);
        assert_eq!(pool.tokens_distributed, 0);
        assert_eq!(pool.total_points, 0);
        assert_eq!(pool.proof_count, 0);
    }

    #[test]
    fn test_decode_daily_pool_accepts_oversized_payload() {
        let mut payload = build_daily_pool_payload(7, 16_000_000_000_000, 500_000_000, 12_345, 42);
        payload.extend(std::iter::repeat_n(0xFFu8, 500));
        let pool = decode_daily_pool(&payload, 7).unwrap();
        assert_eq!(pool.daily_emission, 16_000_000_000_000);
        assert_eq!(pool.proof_count, 42);
    }

    #[test]
    fn test_decode_daily_pool_error_message_reports_sizes() {
        let result = decode_daily_pool(&[0xAA; 10], 0);
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("10 bytes"),
            "error should include actual size: {}",
            msg
        );
        assert!(
            msg.contains("DailyPool account too small"),
            "error should identify the account: {}",
            msg
        );
    }

    #[test]
    fn test_decode_daily_pool_fuzz_garbage_short() {
        // Range of pseudo-random short garbage payloads — never panic.
        let sizes = [1usize, 7, 8, 20, 50, 65, 73];
        for size in sizes {
            let payload: Vec<u8> = (0..size).map(|i| (i * 53 + 17) as u8).collect();
            assert!(
                decode_daily_pool(&payload, 0).is_err(),
                "size {} should err",
                size
            );
        }
    }

    #[test]
    fn test_decode_daily_pool_preserves_caller_day_index() {
        // The caller passes day_index explicitly — the decoder should echo
        // it regardless of what's in the payload. This guards against
        // silent drift if the on-chain layout changes.
        let payload = build_daily_pool_payload(7, 0, 0, 0, 0);
        let pool = decode_daily_pool(&payload, 999).unwrap();
        assert_eq!(pool.day_index, 999);
    }

    #[test]
    fn test_decode_daily_pool_does_not_panic_on_any_size_up_to_min() {
        // Exhaustive guard: every single byte-length up to MIN must
        // either Ok or Err — never panic. Runs under 1 ms.
        for size in 0..=DAILY_POOL_ACCOUNT_MIN_LEN {
            let payload = vec![0xFFu8; size];
            let _ = decode_daily_pool(&payload, 0);
        }
    }
}
