/// AMOS Bounty Program Instructions Module
///
/// This module organizes all instruction handlers by category:
/// - admin: Program initialization and governance
/// - distribution: Core bounty submission and token distribution (treasury)
/// - escrow: Commercial bounty escrow (create, release, refund)
/// - decay: Token decay mechanics
/// - trust: AI agent trust system management
/// - metrics: Platform metrics oracle
/// - prepare: Account preparation
/// - claims: Bounty claiming, timeout, and release
/// - dispute: Dispute filing, resolution, and timeout
/// - registry: Contribution type registry with graduated freeze
pub mod admin;
pub mod claims;
pub mod decay;
pub mod dispute;
pub mod distribution;
pub mod escrow;
pub mod metrics;
pub mod prepare;
pub mod registry;
pub mod trust;

pub use admin::*;
pub use claims::*;
pub use decay::*;
pub use dispute::*;
pub use distribution::*;
pub use escrow::*;
pub use metrics::*;
pub use prepare::*;
pub use registry::*;
pub use trust::*;
