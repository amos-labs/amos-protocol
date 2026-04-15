//! Background task that retries failed on-chain settlements.
//!
//! Runs on a fixed interval, finds approved bounties with settlement_status='failed',
//! and retries the Solana transaction. Uses exponential backoff per bounty via a
//! retry_count column, giving up after MAX_RETRIES.

use crate::{
    solana::{compute_dynamic_max_reward, fallback_max_reward, SettlementParams},
    state::RelayState,
};
use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;
use sqlx::Row;
use std::str::FromStr;
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

/// How often to scan for failed settlements.
const RETRY_INTERVAL: Duration = Duration::from_secs(120);

/// Maximum number of retry attempts per bounty before giving up.
const MAX_RETRIES: i32 = 5;

/// Run the settlement retry loop. Intended to be spawned as a background task.
pub async fn run_settlement_retry_loop(state: RelayState) {
    // Wait a bit before first run to let the server fully start
    tokio::time::sleep(Duration::from_secs(10)).await;

    info!(
        "Settlement retry background task started (interval={}s, max_retries={})",
        RETRY_INTERVAL.as_secs(),
        MAX_RETRIES
    );

    loop {
        if let Err(e) = retry_failed_settlements(&state).await {
            warn!("Settlement retry cycle error: {}", e);
        }
        tokio::time::sleep(RETRY_INTERVAL).await;
    }
}

async fn retry_failed_settlements(state: &RelayState) -> Result<(), String> {
    let solana = match state.solana.as_ref() {
        Some(s) if s.is_settlement_ready() => s,
        _ => return Ok(()), // No Solana configured or not ready — nothing to retry
    };

    // Find failed bounties that haven't exceeded max retries
    let rows = sqlx::query(
        r#"
        SELECT id, reward_tokens, claimed_by_wallet, claimed_by_agent_id,
               result, quality_score,
               COALESCE(settlement_retry_count, 0) as retry_count
        FROM relay_bounties
        WHERE status = 'approved'
          AND settlement_status = 'failed'
          AND COALESCE(settlement_retry_count, 0) < $1
        ORDER BY approved_at ASC
        LIMIT 5
        "#,
    )
    .bind(MAX_RETRIES)
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("Failed to query failed settlements: {}", e))?;

    if rows.is_empty() {
        return Ok(());
    }

    info!(
        "Found {} bounties with failed settlements to retry",
        rows.len()
    );

    for row in rows {
        let id: Uuid = row.get("id");
        let reward_tokens: i64 = row.get("reward_tokens");
        let reward_tokens = reward_tokens as u64;
        let retry_count: i32 = row.get("retry_count");

        let claimed_by_wallet: Option<String> = row.get("claimed_by_wallet");
        let claimed_by_agent_id: Option<Uuid> = row.get("claimed_by_agent_id");

        let wallet = match claimed_by_wallet {
            Some(ref w) if !w.is_empty() => w.clone(),
            _ => {
                if let Some(aid) = claimed_by_agent_id {
                    match sqlx::query_scalar::<_, Option<String>>(
                        "SELECT wallet_address FROM relay_agents WHERE id = $1",
                    )
                    .bind(aid)
                    .fetch_optional(&state.db)
                    .await
                    {
                        Ok(Some(Some(w))) => w,
                        _ => {
                            warn!(bounty_id = %id, "No wallet found — skipping retry");
                            continue;
                        }
                    }
                } else {
                    warn!(bounty_id = %id, "No agent or wallet — skipping retry");
                    continue;
                }
            }
        };

        // Build settlement params — use wallet pubkey bytes as agent_id (portable across relays)
        let bounty_id_str = id.to_string();
        let agent_id_bytes: [u8; 32] = Pubkey::from_str(&wallet)
            .map(|pk| pk.to_bytes())
            .unwrap_or([0u8; 32]);

        let result_json: Option<serde_json::Value> = row.get("result");
        let evidence_hash = {
            let mut hasher = Sha256::new();
            hasher.update(
                serde_json::to_string(&result_json)
                    .unwrap_or_default()
                    .as_bytes(),
            );
            let result = hasher.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&result);
            out
        };

        let agent_trust_level: i16 = if let Some(aid) = claimed_by_agent_id {
            sqlx::query_scalar::<_, i16>("SELECT trust_level FROM relay_agents WHERE id = $1")
                .bind(aid)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .unwrap_or(1)
        } else {
            1
        };

        let max_for_trust = match agent_trust_level {
            1 => 100u64,
            2 => 200,
            3 => 500,
            4 => 1000,
            _ => 2000,
        };
        let base_points = (reward_tokens.min(max_for_trust)) as u16;
        let quality_score: Option<i16> = row.get("quality_score");

        // Dynamic max_reward from on-chain pool state
        let max_reward = match solana.read_config_timing().await {
            Ok((start_time, day_index)) => {
                let now = chrono::Utc::now().timestamp();
                match solana.read_daily_pool(day_index).await {
                    Ok(Some(pool)) => {
                        let mr =
                            compute_dynamic_max_reward(base_points as u64, &pool, start_time, now);
                        info!(bounty_id = %id, max_reward = mr, "Dynamic max_reward for retry");
                        mr
                    }
                    _ => fallback_max_reward(base_points as u64),
                }
            }
            Err(_) => fallback_max_reward(base_points as u64),
        };

        // Use a fallback reviewer wallet
        let reviewer_wallet: String = sqlx::query_scalar::<_, Option<String>>(
            "SELECT reviewer_wallet FROM relay_bounties WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or_else(|| "kekPK242otEGHrNmZA7v2jLYdkg3BPYiTPMJvrDhNuj".to_string());

        let params = SettlementParams {
            bounty_id: bounty_id_str,
            agent_wallet: wallet,
            reviewer_wallet,
            base_points,
            quality_score: quality_score.unwrap_or(70) as u8,
            contribution_type: 1,
            is_agent: true,
            agent_id: agent_id_bytes,
            evidence_hash,
            max_reward,
        };

        info!(bounty_id = %id, retry = retry_count + 1, "Retrying settlement");

        // Increment retry count first
        let _ = sqlx::query(
            "UPDATE relay_bounties SET settlement_retry_count = COALESCE(settlement_retry_count, 0) + 1 WHERE id = $1",
        )
        .bind(id)
        .execute(&state.db)
        .await;

        match solana.process_bounty_payout(&params).await {
            Ok(result) => {
                let _ = sqlx::query(
                    "UPDATE relay_bounties SET settlement_tx = $1, settlement_status = 'settled' WHERE id = $2",
                )
                .bind(&result.tx_signature)
                .bind(id)
                .execute(&state.db)
                .await;

                let _ = sqlx::query(
                    "UPDATE protocol_fee_ledger SET settled_on_chain = true, settlement_tx = $1 WHERE bounty_id = $2",
                )
                .bind(&result.tx_signature)
                .bind(id)
                .execute(&state.db)
                .await;

                info!(bounty_id = %id, tx = %result.tx_signature, "Settlement retry succeeded");
            }
            Err(e) => {
                warn!(bounty_id = %id, retry = retry_count + 1, error = %e, "Settlement retry failed");
                if retry_count + 1 >= MAX_RETRIES {
                    warn!(bounty_id = %id, "Max retries reached — marking as permanently failed");
                    let _ = sqlx::query(
                        "UPDATE relay_bounties SET settlement_status = 'permanently_failed' WHERE id = $1",
                    )
                    .bind(id)
                    .execute(&state.db)
                    .await;
                }
            }
        }

        // Small delay between retries to avoid hammering the RPC
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    Ok(())
}
