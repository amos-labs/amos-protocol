/// AMOS Treasury Instructions Module
///
/// Organizes all instruction handlers into logical groups:
/// - admin: Initialization and configuration
/// - revenue: Revenue receipt and distribution
/// - claims: Stake registration and revenue claims
/// - transparency: Read-only queries and views
pub mod admin;
pub mod claims;
pub mod revenue;
pub mod transparency;

// Re-export instruction handlers for convenient access
pub use admin::*;
pub use claims::*;
pub use revenue::*;
pub use transparency::*;
