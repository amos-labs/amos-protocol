//! Contribution type registry: the mapping from a bounty's `category` string
//! (human-facing) to its on-chain `contribution_type` (numeric) + multiplier.
//!
//! Lives in the on-chain `ContributionTypeRegistry` PDA but Oracle needs a
//! local view for decision-time computations (points calculation, routing).
//! AMOS-first impl hardcodes the current 12 types and mirrors the on-chain
//! values; future generic extraction would pull from-chain dynamically.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Result;

/// One contribution type registered in the on-chain registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionType {
    pub type_id: u8,
    pub name: String,
    pub base_multiplier_bps: u16,
    pub pool_category: PoolCategory,
    pub trust_required: u8,
    pub frozen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PoolCategory {
    Technical,
    Growth,
}

/// Lookup interface for Oracle decision-making.
#[async_trait]
pub trait ContributionRegistry: Send + Sync {
    /// Map a human-facing category string (e.g. `"infrastructure"`,
    /// `"discovery"`) to its on-chain type_id + metadata.
    async fn lookup_by_name(&self, name: &str) -> Result<Option<ContributionType>>;

    /// Map a type_id back to metadata.
    async fn lookup_by_id(&self, type_id: u8) -> Result<Option<ContributionType>>;

    /// List all currently-active types.
    async fn list(&self) -> Result<Vec<ContributionType>>;
}

/// AMOS-specific registry. Mirrors the on-chain state of
/// `amos-solana/programs/amos-bounty/src/constants.rs`.
///
/// **Invariant:** the entries here must match the on-chain registry PDA at
/// runtime. This is a cache for decision-making, not the authority.
/// `refresh()` should be called periodically or on governance-proposal events.
pub struct AmosContributionRegistry {
    types: std::sync::Arc<tokio::sync::RwLock<Vec<ContributionType>>>,
}

impl Default for AmosContributionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AmosContributionRegistry {
    pub fn new() -> Self {
        Self {
            types: std::sync::Arc::new(tokio::sync::RwLock::new(default_amos_types())),
        }
    }

    /// Replace the cached list. For use by a background refresh task that
    /// reads the on-chain registry PDA.
    pub async fn refresh(&self, new_types: Vec<ContributionType>) {
        let mut w = self.types.write().await;
        *w = new_types;
    }
}

#[async_trait]
impl ContributionRegistry for AmosContributionRegistry {
    async fn lookup_by_name(&self, name: &str) -> Result<Option<ContributionType>> {
        let r = self.types.read().await;
        Ok(r.iter().find(|t| t.name == name).cloned())
    }

    async fn lookup_by_id(&self, type_id: u8) -> Result<Option<ContributionType>> {
        let r = self.types.read().await;
        Ok(r.iter().find(|t| t.type_id == type_id).cloned())
    }

    async fn list(&self) -> Result<Vec<ContributionType>> {
        let r = self.types.read().await;
        Ok(r.clone())
    }
}

/// Mirror of the on-chain contribution types as of 2026-04-20.
///
/// Source of truth: `amos-solana/programs/amos-bounty/src/constants.rs`. If
/// the on-chain registry changes (e.g. governance-added type), update here.
fn default_amos_types() -> Vec<ContributionType> {
    use PoolCategory::*;
    vec![
        ContributionType {
            type_id: 0,
            name: "infrastructure".into(),
            base_multiplier_bps: 13000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 1,
            name: "bug_fix".into(),
            base_multiplier_bps: 12000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 2,
            name: "testing_qa".into(),
            base_multiplier_bps: 11000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 3,
            name: "research".into(),
            base_multiplier_bps: 10000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 4,
            name: "feature".into(),
            base_multiplier_bps: 10000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 5,
            name: "design".into(),
            base_multiplier_bps: 10000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 6,
            name: "content_marketing".into(),
            base_multiplier_bps: 9000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 7,
            name: "documentation".into(),
            base_multiplier_bps: 8000,
            pool_category: Technical,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 8,
            name: "bug_report".into(),
            base_multiplier_bps: 10000,
            pool_category: Growth,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 9,
            name: "referral".into(),
            base_multiplier_bps: 6000,
            pool_category: Growth,
            trust_required: 1,
            frozen: false,
        },
        ContributionType {
            type_id: 10,
            name: "signup".into(),
            base_multiplier_bps: 4000,
            pool_category: Growth,
            trust_required: 0,
            frozen: false,
        },
        ContributionType {
            type_id: 11,
            name: "discovery".into(),
            base_multiplier_bps: 15000,
            pool_category: Technical,
            trust_required: 3,
            frozen: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_registry_has_12_types_including_discovery() {
        let r = AmosContributionRegistry::new();
        let list = r.list().await.unwrap();
        assert_eq!(list.len(), 12);
        let discovery = r.lookup_by_name("discovery").await.unwrap();
        assert!(discovery.is_some());
        assert_eq!(discovery.as_ref().unwrap().type_id, 11);
        assert_eq!(discovery.as_ref().unwrap().base_multiplier_bps, 15000);
    }

    #[tokio::test]
    async fn lookup_by_id_symmetric() {
        let r = AmosContributionRegistry::new();
        let infra = r.lookup_by_name("infrastructure").await.unwrap().unwrap();
        let by_id = r.lookup_by_id(infra.type_id).await.unwrap().unwrap();
        assert_eq!(infra.name, by_id.name);
    }
}
