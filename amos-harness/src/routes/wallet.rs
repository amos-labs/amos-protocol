//! Wallet connection routes — link Solana wallets to harness tenants.
//!
//! Endpoints:
//!   - `POST /api/v1/wallet/connect`    — Verify signature and store wallet
//!   - `GET  /api/v1/wallet/balance`    — Get AMOS token balance
//!   - `GET  /api/v1/wallet/info`       — Get wallet details for current tenant
//!   - `POST /api/v1/wallet/disconnect` — Unlink wallet
//!   - `GET  /api/v1/config/solana`     — Public: return Solana network config

use crate::middleware::Claims;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// Routes
// ═══════════════════════════════════════════════════════════════════════════

/// Protected wallet routes (require auth).
pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/connect", post(connect_wallet))
        .route("/balance", get(get_balance))
        .route("/info", get(get_info))
        .route("/disconnect", post(disconnect_wallet))
}

/// Public Solana config route (no auth).
pub fn public_routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new().route("/solana", get(get_solana_config))
}

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ConnectWalletRequest {
    public_key: String,
    signature: Vec<u8>,
    message: String,
    nonce: String,
}

#[derive(Debug, Serialize)]
struct ConnectWalletResponse {
    public_key: String,
    verified: bool,
}

#[derive(Debug, Serialize)]
struct WalletInfoResponse {
    id: String,
    tenant_id: String,
    wallet_address: String,
    wallet_type: String,
    is_primary: bool,
    verified_at: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct BalanceQuery {
    address: String,
}

#[derive(Debug, Serialize)]
struct BalanceResponse {
    address: String,
    balance: f64,
    /// Raw lamports / token amount
    raw_balance: u64,
}

#[derive(Debug, Serialize)]
struct SolanaConfigResponse {
    network: String,
    rpc_url: String,
    amos_mint: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════════════════

/// `POST /api/v1/wallet/connect` — Verify ed25519 signature and store wallet.
async fn connect_wallet(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ConnectWalletRequest>,
) -> Result<Json<ConnectWalletResponse>, (StatusCode, Json<serde_json::Value>)> {
    // 1. Validate the nonce is recent (within 5 minutes)
    let nonce_ts: i64 = req.nonce.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid nonce: expected Unix timestamp" })),
        )
    })?;
    let now = chrono::Utc::now().timestamp();
    if (now - nonce_ts).unsigned_abs() > 300 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Nonce expired — must be within 5 minutes" })),
        ));
    }

    // 2. Decode the public key from base58
    let pubkey_bytes: [u8; 32] = bs58::decode(&req.public_key)
        .into_vec()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid base58 public key" })),
            )
        })?
        .try_into()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Public key must be 32 bytes" })),
            )
        })?;

    // 3. Verify the ed25519 signature
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_bytes).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid ed25519 public key" })),
        )
    })?;

    let sig_bytes: [u8; 64] = req.signature.as_slice().try_into().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Signature must be 64 bytes" })),
        )
    })?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    use ed25519_dalek::Verifier;
    verifying_key
        .verify(req.message.as_bytes(), &signature)
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Signature verification failed" })),
            )
        })?;

    // 4. Parse tenant_id from claims
    let tenant_id: uuid::Uuid = claims.tenant_id.parse().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Invalid tenant_id in token" })),
        )
    })?;

    // 5. Upsert into wallet_connections
    sqlx::query(
        r#"
        INSERT INTO wallet_connections (tenant_id, wallet_address, wallet_type, verified_at, is_primary, updated_at)
        VALUES ($1, $2, 'solana', NOW(), true, NOW())
        ON CONFLICT (wallet_address)
        DO UPDATE SET tenant_id = $1, verified_at = NOW(), updated_at = NOW()
        "#,
    )
    .bind(tenant_id)
    .bind(&req.public_key)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to upsert wallet connection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to store wallet connection" })),
        )
    })?;

    tracing::info!(
        tenant_id = %tenant_id,
        wallet = %req.public_key,
        "Wallet connected"
    );

    Ok(Json(ConnectWalletResponse {
        public_key: req.public_key,
        verified: true,
    }))
}

/// `GET /api/v1/wallet/balance` — Get AMOS token balance for an address.
///
/// Queries the Solana JSON-RPC directly for SPL token balance. Falls back to
/// zero if the AMOS mint is not configured or the account doesn't exist.
async fn get_balance(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BalanceQuery>,
) -> Result<Json<BalanceResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Validate the address is plausible base58
    let _ = bs58::decode(&params.address).into_vec().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid base58 address" })),
        )
    })?;

    let rpc_url = &state.config.solana.rpc_url;
    let mint = match &state.config.solana.mint_address {
        Some(m) if !m.is_empty() => m.clone(),
        _ => {
            // No AMOS mint configured — return zero balance
            return Ok(Json(BalanceResponse {
                address: params.address,
                balance: 0.0,
                raw_balance: 0,
            }));
        }
    };

    // Query Solana JSON-RPC for SPL token accounts by owner filtered by mint
    let rpc_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTokenAccountsByOwner",
        "params": [
            params.address,
            { "mint": mint },
            { "encoding": "jsonParsed" }
        ]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(rpc_url)
        .json(&rpc_body)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("Solana RPC request failed: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Solana RPC request failed" })),
            )
        })?;

    let rpc_result: serde_json::Value = resp.json().await.map_err(|e| {
        tracing::warn!("Failed to parse Solana RPC response: {}", e);
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": "Invalid Solana RPC response" })),
        )
    })?;

    // Parse the token balance from the RPC response
    let (raw_balance, decimals) = rpc_result
        .pointer("/result/value")
        .and_then(|v| v.as_array())
        .and_then(|accounts| accounts.first())
        .and_then(|account| account.pointer("/account/data/parsed/info/tokenAmount"))
        .map(|amount| {
            let raw = amount
                .get("amount")
                .and_then(|a| a.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let dec = amount.get("decimals").and_then(|d| d.as_u64()).unwrap_or(9) as u32;
            (raw, dec)
        })
        .unwrap_or((0, 9));

    let balance = raw_balance as f64 / 10f64.powi(decimals as i32);

    Ok(Json(BalanceResponse {
        address: params.address,
        balance,
        raw_balance,
    }))
}

/// `GET /api/v1/wallet/info` — Get wallet details for the current tenant.
async fn get_info(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id: uuid::Uuid = claims.tenant_id.parse().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Invalid tenant_id in token" })),
        )
    })?;

    let row = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, String, String, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, tenant_id, wallet_address, wallet_type, is_primary, verified_at, created_at FROM wallet_connections WHERE tenant_id = $1 AND is_primary = true"
    )
    .bind(tenant_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to query wallet connection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Database query failed" })),
        )
    })?;

    match row {
        Some((id, tid, address, wtype, primary, verified, created)) => {
            Ok(Json(serde_json::json!({
                "id": id.to_string(),
                "tenant_id": tid.to_string(),
                "wallet_address": address,
                "wallet_type": wtype,
                "is_primary": primary,
                "verified_at": verified.to_rfc3339(),
                "created_at": created.to_rfc3339(),
            })))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "No wallet connected" })),
        )),
    }
}

/// `POST /api/v1/wallet/disconnect` — Unlink wallet for the current tenant.
async fn disconnect_wallet(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id: uuid::Uuid = claims.tenant_id.parse().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Invalid tenant_id in token" })),
        )
    })?;

    let result = sqlx::query("DELETE FROM wallet_connections WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to delete wallet connection: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to delete wallet connection" })),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "No wallet connected" })),
        ));
    }

    tracing::info!(tenant_id = %tenant_id, "Wallet disconnected");

    Ok(Json(
        serde_json::json!({ "disconnected": true, "tenant_id": tenant_id.to_string() }),
    ))
}

/// `GET /api/v1/config/solana` — Public endpoint returning Solana network config.
async fn get_solana_config(State(state): State<Arc<AppState>>) -> Json<SolanaConfigResponse> {
    // Infer network name from RPC URL
    let rpc_url = &state.config.solana.rpc_url;
    let network = if rpc_url.contains("mainnet") {
        "mainnet-beta"
    } else if rpc_url.contains("devnet") {
        "devnet"
    } else if rpc_url.contains("testnet") {
        "testnet"
    } else {
        "custom"
    }
    .to_string();

    Json(SolanaConfigResponse {
        network,
        rpc_url: rpc_url.clone(),
        amos_mint: state.config.solana.mint_address.clone(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routes_build() {
        // Verify routes compile and build without panic
        let _ = Router::<Arc<AppState>>::new()
            .route("/connect", post(connect_wallet))
            .route("/balance", get(get_balance))
            .route("/info", get(get_info))
            .route("/disconnect", post(disconnect_wallet));
    }

    #[test]
    fn test_public_routes_build() {
        let _ = Router::<Arc<AppState>>::new().route("/solana", get(get_solana_config));
    }

    #[test]
    fn test_nonce_validation_expired() {
        // A nonce from 10 minutes ago should be expired
        let ten_min_ago = chrono::Utc::now().timestamp() - 600;
        let now = chrono::Utc::now().timestamp();
        assert!((now - ten_min_ago).unsigned_abs() > 300);
    }

    #[test]
    fn test_nonce_validation_recent() {
        // A nonce from 1 minute ago should be valid
        let one_min_ago = chrono::Utc::now().timestamp() - 60;
        let now = chrono::Utc::now().timestamp();
        assert!((now - one_min_ago).unsigned_abs() <= 300);
    }

    #[test]
    fn test_network_inference_devnet() {
        let url = "https://api.devnet.solana.com";
        let network = if url.contains("mainnet") {
            "mainnet-beta"
        } else if url.contains("devnet") {
            "devnet"
        } else if url.contains("testnet") {
            "testnet"
        } else {
            "custom"
        };
        assert_eq!(network, "devnet");
    }

    #[test]
    fn test_network_inference_mainnet() {
        let url = "https://api.mainnet-beta.solana.com";
        let network = if url.contains("mainnet") {
            "mainnet-beta"
        } else if url.contains("devnet") {
            "devnet"
        } else {
            "custom"
        };
        assert_eq!(network, "mainnet-beta");
    }

    #[test]
    fn test_ed25519_signature_roundtrip() {
        use ed25519_dalek::{Signer, SigningKey, Verifier};
        use rand::RngCore;

        // Generate a random 32-byte secret key
        let mut secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = signing_key.verifying_key();

        // Sign a message
        let message = b"AMOS wallet connect: 1713100000";
        let signature = signing_key.sign(message);

        // Verify
        assert!(verifying_key.verify(message, &signature).is_ok());

        // Wrong message should fail
        assert!(verifying_key.verify(b"wrong message", &signature).is_err());
    }

    #[test]
    fn test_bs58_pubkey_decode() {
        // A valid Solana address is 32 bytes in base58
        let pubkey = "11111111111111111111111111111111"; // system program
        let bytes = bs58::decode(pubkey).into_vec().unwrap();
        assert_eq!(bytes.len(), 32);
    }
}
