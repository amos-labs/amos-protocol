//! # AMOS Network Relay
//!
//! The global coordination layer for the AMOS agent economy.
//!
//! This crate provides:
//! - Global bounty marketplace for cross-harness task coordination
//! - Agent directory and capability discovery
//! - Cross-harness reputation oracle
//! - Protocol fee collection and distribution

pub mod middleware;
pub mod pointing;
pub mod proof_receipt;
pub mod protocol_fees;
pub mod reputation;
pub mod routes;
pub mod server;
pub mod settlement_retry;
pub mod solana;
pub mod state;

/// Current version of amos-relay.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Re-export commonly used types
pub use amos_core::Result;
pub use state::RelayState;

/// Validate that a string is a valid Solana wallet address (base58-encoded 32-byte public key).
pub fn validate_wallet_address(addr: &str) -> bool {
    bs58::decode(addr)
        .into_vec()
        .map(|bytes| bytes.len() == 32)
        .unwrap_or(false)
}
