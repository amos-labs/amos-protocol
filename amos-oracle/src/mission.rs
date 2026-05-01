//! Mission source: where Oracle reads "what good looks like."
//!
//! The constitutional prompt + the supporting docs the Oracle consults before
//! every decision. Versioned so that decisions can be audited against the
//! specific mission state they were made under.
//!
//! AMOS-first impl reads from the local filesystem (v2 thesis,
//! `AGENT_CONTEXT.md`, seed bounty catalog). Future generic extraction would
//! turn this into a trait with injected sources.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::Result;

/// Versioned snapshot of the mission, suitable for inlining into a prompt.
///
/// Sections are kept separate so the Oracle prompt can prioritize them
/// differently (constitutional claims > strategic thesis > operational notes).
#[derive(Debug, Clone)]
pub struct MissionSnapshot {
    pub version: String,
    pub constitutional_provisions: String,
    pub strategic_thesis: String,
    pub operational_context: String,
}

/// Source for the constitutional prompt + supporting mission documents.
///
/// Implementations must be deterministic: the same snapshot produced for a
/// given version tag must be byte-identical across runs. This is what lets
/// decisions be re-evaluated against the exact prompt state they were made
/// under.
#[async_trait]
pub trait MissionSource: Send + Sync {
    /// Fetch the current mission snapshot. Used on Oracle startup + on prompt
    /// hot-reload.
    async fn current(&self) -> Result<MissionSnapshot>;

    /// Fetch a specific historical version (for audit / replay).
    async fn at_version(&self, version: &str) -> Result<MissionSnapshot>;
}

/// AMOS-specific mission source.
///
/// Reads from `docs/core/thesis.md` (strategic thesis — current content
/// reflects the bounded-autonomous-economic-organism rewrite),
/// `AGENT_CONTEXT.md` (operational context), and the constitutional prompt
/// (separate artifact, council-signed).
pub struct AmosMissionSource {
    pub thesis_path: PathBuf,
    pub agent_context_path: PathBuf,
    pub constitutional_prompt_path: PathBuf,
    pub version: String,
}

impl AmosMissionSource {
    pub fn new(project_root: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        let root = project_root.into();
        Self {
            thesis_path: root.join("docs/core/thesis.md"),
            agent_context_path: root.join("AGENT_CONTEXT.md"),
            constitutional_prompt_path: root.join("amos-oracle/prompts/amos_constitutional_v1.md"),
            version: version.into(),
        }
    }
}

#[async_trait]
impl MissionSource for AmosMissionSource {
    async fn current(&self) -> Result<MissionSnapshot> {
        let constitutional = tokio::fs::read_to_string(&self.constitutional_prompt_path)
            .await
            .map_err(|e| {
                crate::OracleError::MissionSource(format!(
                    "reading constitutional prompt at {:?}: {}",
                    self.constitutional_prompt_path, e
                ))
            })?;
        let thesis = tokio::fs::read_to_string(&self.thesis_path)
            .await
            .map_err(|e| {
                crate::OracleError::MissionSource(format!(
                    "reading thesis at {:?}: {}",
                    self.thesis_path, e
                ))
            })?;
        let agent_context = tokio::fs::read_to_string(&self.agent_context_path)
            .await
            .map_err(|e| {
                crate::OracleError::MissionSource(format!(
                    "reading agent context at {:?}: {}",
                    self.agent_context_path, e
                ))
            })?;

        Ok(MissionSnapshot {
            version: self.version.clone(),
            constitutional_provisions: constitutional,
            strategic_thesis: thesis,
            operational_context: agent_context,
        })
    }

    async fn at_version(&self, version: &str) -> Result<MissionSnapshot> {
        // MVP: only current version supported. Historical-version replay is a
        // follow-up (requires git-based or artifact-versioned mission docs).
        if version == self.version {
            self.current().await
        } else {
            Err(crate::OracleError::MissionSource(format!(
                "historical version {} not supported yet (current: {})",
                version, self.version
            )))
        }
    }
}
