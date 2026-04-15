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
}

impl SolanaClient {
    /// Create a new Solana client connected to the given RPC endpoint.
    pub fn new(rpc_url: &str, bounty_program_id: &str) -> Result<Self> {
        let rpc =
            RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

        let bounty_program_id = Pubkey::from_str(bounty_program_id)
            .map_err(|e| AmosError::SolanaRpc(format!("Invalid bounty program ID: {}", e)))?;

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
    pub fn load_oracle_keypair(&mut self, keypair_path: &str) -> Result<()> {
        let keypair_bytes = std::fs::read_to_string(keypair_path).map_err(|e| {
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
            AccountMeta::new_readonly(agent_trust_account, false),
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

/// Build the instruction data for `register_agent_trust`.
/// Layout: 8-byte discriminator + agent_id ([u8; 32]).
fn build_register_agent_trust_data(agent_id: &[u8; 32]) -> Vec<u8> {
    let disc = anchor_discriminator("register_agent_trust");
    let mut data = Vec::with_capacity(8 + 32);
    data.extend_from_slice(&disc);
    data.extend_from_slice(agent_id);
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
    reviewer: &Pubkey,
    evidence_hash: &[u8; 32],
) -> Vec<u8> {
    let disc = anchor_discriminator("submit_bounty_proof");
    let external_reference = [0u8; 64]; // Reserved, zeroed

    let mut data = Vec::with_capacity(8 + 32 + 2 + 1 + 1 + 1 + 32 + 4 + 32 + 32 + 64);
    data.extend_from_slice(&disc);
    data.extend_from_slice(bounty_id);
    data.extend_from_slice(&base_points.to_le_bytes());
    data.push(quality_score);
    data.push(contribution_type);
    data.push(is_agent as u8);
    data.extend_from_slice(agent_id);
    data.extend_from_slice(&day_index.to_le_bytes());
    data.extend_from_slice(reviewer.as_ref());
    data.extend_from_slice(evidence_hash);
    data.extend_from_slice(&external_reference);
    data
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
            &reviewer,
            &evidence_hash,
        );

        // 8 (disc) + 32 (bounty_id) + 2 (points) + 1 (quality) + 1 (type)
        // + 1 (is_agent) + 32 (agent_id) + 4 (day_index) + 32 (reviewer) + 32 (evidence)
        // + 64 (external_ref) = 209
        assert_eq!(data.len(), 209);
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
            &reviewer,
            &evidence_hash,
        );

        // Last 64 bytes should be zeroed (external_reference)
        assert_eq!(&data[145..209], &[0u8; 64]);
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
}
