//! Tool system for agent execution
//!
//! Tools are the primary way the agent interacts with the world.

pub mod app_tools;
pub mod automation_tools;
pub mod bounty_agent_tools;
pub mod canvas_tools;
pub mod credential_tools;
pub mod document_tools;
pub mod image_gen_tools;
pub mod integration_tools;
pub mod knowledge_tools;
pub mod memory_tools;
pub mod openclaw_tools;
// orchestration_tools removed — external agent work delegation is now handled
// by task_tools (create_bounty, get_task_result) and openclaw_tools (agent management).
pub mod platform_tools;
pub mod revision_tools;
pub mod schema_tools;
pub mod site_tools;
pub mod system_tools;
pub mod task_tools;
pub mod web_tools;
pub mod workspace_tools;

use crate::automations::engine::AutomationEngine;
use crate::embeddings::EmbeddingService;
use crate::integrations::{etl::EtlPipeline, executor::ApiExecutor};
use crate::relay_sync::RelayBounty;
use crate::task_queue::TaskQueue;
use amos_core::{AmosError, AppConfig, PackageToolRegistry, Result};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;

/// Maximum time a single tool execution may run before being cancelled.
const TOOL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(120);

// Re-export core tool types so existing harness code doesn't break
pub use amos_core::tools::{Tool, ToolCategory, ToolResult};

/// A tool entry in the registry, tagged with its owning package (if any).
struct RegisteredTool {
    tool: Arc<dyn Tool>,
    /// `None` = core harness tool, `Some("education")` = from education package
    package: Option<String>,
}

/// Tool registry manages all available tools with package-scoped enable/disable.
///
/// Core harness tools are always active. Package tools are only visible to agents
/// when their package is enabled, preventing tool bloat.
///
/// `enabled_packages` is behind an `RwLock` so the package management API can
/// toggle packages at runtime without restarting the harness.
pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
    /// Set of currently enabled package names (runtime-mutable via API)
    enabled_packages: parking_lot::RwLock<HashSet<String>>,
    db_pool: PgPool,
    config: Arc<AppConfig>,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new(db_pool: PgPool, config: Arc<AppConfig>) -> Self {
        Self {
            tools: HashMap::new(),
            enabled_packages: parking_lot::RwLock::new(HashSet::new()),
            db_pool,
            config,
        }
    }

    /// Register a core harness tool (always active)
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(
            tool.name().to_string(),
            RegisteredTool {
                tool,
                package: None,
            },
        );
    }

    /// Register a tool owned by a package. Only visible when the package is enabled.
    pub fn register_package_tool(&mut self, tool: Arc<dyn Tool>, package: &str) {
        self.tools.insert(
            tool.name().to_string(),
            RegisteredTool {
                tool,
                package: Some(package.to_string()),
            },
        );
    }

    /// Enable a package — its tools become visible to agents.
    /// Safe to call from any thread (uses interior RwLock).
    pub fn enable_package(&self, package: &str) {
        self.enabled_packages.write().insert(package.to_string());
        tracing::info!(package, "Package enabled — tools now active");
    }

    /// Disable a package — its tools are hidden from agents.
    /// Safe to call from any thread (uses interior RwLock).
    pub fn disable_package(&self, package: &str) {
        self.enabled_packages.write().remove(package);
        tracing::info!(package, "Package disabled — tools hidden");
    }

    /// Check if a package is currently enabled
    pub fn is_package_enabled(&self, package: &str) -> bool {
        self.enabled_packages.read().contains(package)
    }

    /// List all enabled package names
    pub fn enabled_packages(&self) -> Vec<String> {
        self.enabled_packages.read().iter().cloned().collect()
    }

    /// Returns true if the tool is active (core tool or from an enabled package)
    fn is_tool_active(&self, entry: &RegisteredTool) -> bool {
        match &entry.package {
            None => true, // core tools always active
            Some(pkg) => self.enabled_packages.read().contains(pkg.as_str()),
        }
    }

    /// Execute a tool by name (only if it's active).
    ///
    /// This method is used for internal/sidecar calls where the caller is trusted
    /// (e.g., the agent proxy). For external agent calls, use `execute_with_trust`
    /// which enforces trust-level gating.
    pub async fn execute(&self, tool_name: &str, params: JsonValue) -> Result<ToolResult> {
        let entry = self
            .tools
            .get(tool_name)
            .ok_or_else(|| AmosError::NotFound {
                entity: "Tool".to_string(),
                id: tool_name.to_string(),
            })?;

        if !self.is_tool_active(entry) {
            return Err(AmosError::NotFound {
                entity: "Tool".to_string(),
                id: tool_name.to_string(),
            });
        }

        match tokio::time::timeout(TOOL_EXECUTION_TIMEOUT, entry.tool.execute(params)).await {
            Ok(result) => result,
            Err(_) => Ok(ToolResult::error(format!(
                "Tool '{}' timed out after {}s",
                tool_name,
                TOOL_EXECUTION_TIMEOUT.as_secs()
            ))),
        }
    }

    /// Execute a tool with trust-level enforcement for external agents.
    ///
    /// Returns an error ToolResult if the agent's trust level is insufficient
    /// for the tool's category.
    pub async fn execute_with_trust(
        &self,
        tool_name: &str,
        params: JsonValue,
        agent_trust_level: u8,
    ) -> Result<ToolResult> {
        let entry = self
            .tools
            .get(tool_name)
            .ok_or_else(|| AmosError::NotFound {
                entity: "Tool".to_string(),
                id: tool_name.to_string(),
            })?;

        if !self.is_tool_active(entry) {
            return Err(AmosError::NotFound {
                entity: "Tool".to_string(),
                id: tool_name.to_string(),
            });
        }

        // Enforce trust-level gating
        let required_trust = crate::routes::trust_level_for_category(entry.tool.category());
        if agent_trust_level < required_trust {
            return Ok(ToolResult::error(format!(
                "Insufficient trust level: tool '{}' requires level {}, agent has level {}",
                tool_name, required_trust, agent_trust_level
            )));
        }

        match tokio::time::timeout(TOOL_EXECUTION_TIMEOUT, entry.tool.execute(params)).await {
            Ok(result) => result,
            Err(_) => Ok(ToolResult::error(format!(
                "Tool '{}' timed out after {}s",
                tool_name,
                TOOL_EXECUTION_TIMEOUT.as_secs()
            ))),
        }
    }

    /// Get a tool by name (only if active)
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .get(name)
            .filter(|e| self.is_tool_active(e))
            .map(|e| e.tool.clone())
    }

    /// List all active tool names
    pub fn list_tools(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|(_, e)| self.is_tool_active(e))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get active tools by category
    pub fn get_by_category(&self, category: ToolCategory) -> Vec<Arc<dyn Tool>> {
        self.tools
            .values()
            .filter(|e| self.is_tool_active(e) && e.tool.category() == category)
            .map(|e| e.tool.clone())
            .collect()
    }

    /// List tool names belonging to a specific package
    pub fn tools_for_package(&self, package: &str) -> Vec<String> {
        self.tools
            .iter()
            .filter(|(_, e)| e.package.as_deref() == Some(package))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get tool schemas for LLM (Bedrock ConverseStream format)
    ///
    /// Only returns schemas for active tools (core + enabled packages).
    /// Bedrock expects camelCase keys: `name`, `description`, `inputSchema`
    pub fn get_tool_schemas(&self) -> Vec<JsonValue> {
        self.tools
            .values()
            .filter(|e| self.is_tool_active(e))
            .map(|e| {
                let tool = &e.tool;
                let mut schema = tool.parameters_schema();
                // Ensure inputSchema is never null — Bedrock requires it.
                if schema.is_null() {
                    schema = serde_json::json!({
                        "json": {
                            "type": "object",
                            "properties": {}
                        }
                    });
                }
                serde_json::json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "inputSchema": {
                        "json": schema
                    }
                })
            })
            .collect()
    }

    /// Create a registry with all default tools
    pub fn default_registry(
        db_pool: PgPool,
        config: Arc<AppConfig>,
        task_queue: Arc<TaskQueue>,
        bedrock: Option<Arc<crate::bedrock::BedrockClient>>,
        api_executor: Arc<ApiExecutor>,
        etl_pipeline: Arc<EtlPipeline>,
        embedding_service: Option<Arc<EmbeddingService>>,
        automation_engine: Arc<AutomationEngine>,
        bounty_cache: Arc<RwLock<Vec<RelayBounty>>>,
    ) -> Self {
        let mut registry = Self::new(db_pool.clone(), config.clone());

        // Register platform tools
        registry.register(Arc::new(platform_tools::PlatformQueryTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(platform_tools::PlatformCreateTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(platform_tools::PlatformUpdateTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(platform_tools::PlatformExecuteTool::new(
            db_pool.clone(),
        )));

        // Register canvas tools
        registry.register(Arc::new(canvas_tools::LoadCanvasTool::new(db_pool.clone())));
        registry.register(Arc::new(canvas_tools::CreateDynamicCanvasTool::new(
            db_pool.clone(),
            bedrock.clone(),
        )));
        registry.register(Arc::new(canvas_tools::CreateFreeformCanvasTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(canvas_tools::UpdateCanvasTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(canvas_tools::PublishCanvasTool::new(
            db_pool.clone(),
        )));

        // Register app tools (interactive multi-view applications)
        registry.register(Arc::new(app_tools::CreateAppTool::new(db_pool.clone())));
        registry.register(Arc::new(app_tools::UpdateAppViewTool::new(db_pool.clone())));

        // Register web tools
        // NOTE: WebSearchTool is intentionally NOT registered here — web search
        // is an agent-only tool (uses Brave API with separate billing). The agent
        // has its own local web_search tool with BRAVE_API_KEY.
        registry.register(Arc::new(web_tools::ViewWebPageTool::new()));

        // Register system tools
        registry.register(Arc::new(system_tools::ReadFileTool::new()));
        registry.register(Arc::new(system_tools::BashTool::new()));

        // Register memory tools (with optional embedding support)
        registry.register(Arc::new(memory_tools::RememberThisTool::new(
            db_pool.clone(),
            embedding_service.clone(),
        )));
        registry.register(Arc::new(memory_tools::SearchMemoryTool::new(
            db_pool.clone(),
            embedding_service.clone(),
        )));

        // Register knowledge base tools (RAG: ingest + semantic search)
        registry.register(Arc::new(knowledge_tools::IngestDocumentTool::new(
            db_pool.clone(),
            embedding_service.clone(),
        )));
        registry.register(Arc::new(knowledge_tools::KnowledgeSearchTool::new(
            db_pool.clone(),
            embedding_service.clone(),
        )));

        // Register workspace awareness tools
        registry.register(Arc::new(workspace_tools::GetWorkspaceSummaryTool::new(
            db_pool.clone(),
        )));

        // Register OpenClaw agent management tools
        registry.register(Arc::new(openclaw_tools::RegisterAgentTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(openclaw_tools::ListAgentsTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(openclaw_tools::AssignTaskTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(openclaw_tools::GetAgentStatusTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(openclaw_tools::StopAgentTool::new(
            db_pool.clone(),
        )));

        // Register schema tools (dynamic collections and records)
        registry.register(Arc::new(schema_tools::DefineCollectionTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(schema_tools::ListCollectionsTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(schema_tools::GetCollectionTool::new(
            db_pool.clone(),
        )));
        let event_tx = automation_engine.create_event_channel();
        registry.register(Arc::new(schema_tools::CreateRecordTool::new(
            db_pool.clone(),
            Some(event_tx.clone()),
        )));
        registry.register(Arc::new(schema_tools::QueryRecordsTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(schema_tools::UpdateRecordTool::new(
            db_pool.clone(),
            Some(event_tx.clone()),
        )));
        registry.register(Arc::new(schema_tools::DeleteRecordTool::new(
            db_pool.clone(),
            Some(event_tx),
        )));

        // Register automation tools
        registry.register(Arc::new(automation_tools::CreateAutomationTool::new(
            automation_engine.clone(),
        )));
        registry.register(Arc::new(automation_tools::ListAutomationsTool::new(
            automation_engine.clone(),
        )));
        registry.register(Arc::new(automation_tools::UpdateAutomationTool::new(
            automation_engine.clone(),
        )));
        registry.register(Arc::new(automation_tools::DeleteAutomationTool::new(
            automation_engine.clone(),
        )));
        registry.register(Arc::new(automation_tools::TestAutomationTool::new(
            automation_engine.clone(),
        )));

        // Register site tools (websites and landing pages)
        registry.register(Arc::new(site_tools::CreateSiteTool::new(db_pool.clone())));
        registry.register(Arc::new(site_tools::CreatePageTool::new(db_pool.clone())));
        registry.register(Arc::new(site_tools::UpdatePageTool::new(db_pool.clone())));
        registry.register(Arc::new(site_tools::PublishSiteTool::new(db_pool.clone())));
        registry.register(Arc::new(site_tools::ListSitesTool::new(db_pool.clone())));

        // Register task queue tools (background tasks and bounties)
        registry.register(Arc::new(task_tools::CreateTaskTool::new(
            task_queue.clone(),
        )));
        registry.register(Arc::new(task_tools::CreateBountyTool::new(
            config.relay.url.clone(),
        )));
        registry.register(Arc::new(task_tools::ListTasksTool::new(task_queue.clone())));
        registry.register(Arc::new(task_tools::GetTaskResultTool::new(
            task_queue.clone(),
        )));
        registry.register(Arc::new(task_tools::CancelTaskTool::new(
            task_queue.clone(),
        )));

        // Register document tools (export documents)
        registry.register(Arc::new(document_tools::GenerateDocumentTool::new(
            config.clone(),
        )));

        // Register image generation tools
        registry.register(Arc::new(image_gen_tools::GenerateImageTool::new()));

        // Register revision and template tools
        registry.register(Arc::new(revision_tools::ListRevisionsTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(revision_tools::GetRevisionTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(revision_tools::RevertEntityTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(revision_tools::ListTemplatesTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(revision_tools::CheckTemplateUpdatesTool::new(
            db_pool.clone(),
        )));

        // Register credential vault tools
        registry.register(Arc::new(credential_tools::CollectCredentialTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(credential_tools::ListVaultCredentialsTool::new(
            db_pool.clone(),
        )));

        // Register integration tools
        registry.register(Arc::new(integration_tools::ListIntegrationsTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(integration_tools::ListConnectionsTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(integration_tools::CreateConnectionTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(integration_tools::TestConnectionTool::new(
            db_pool.clone(),
            api_executor.clone(),
        )));
        registry.register(Arc::new(
            integration_tools::ExecuteIntegrationActionTool::new(api_executor.clone()),
        ));
        registry.register(Arc::new(integration_tools::ListOperationsTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(integration_tools::CreateSyncConfigTool::new(
            db_pool.clone(),
        )));
        registry.register(Arc::new(integration_tools::TriggerSyncTool::new(
            etl_pipeline.clone(),
        )));

        // Register bounty agent tools (autonomous bounty discovery and execution)
        registry.register(Arc::new(
            bounty_agent_tools::DiscoverBountiesTool::new(
                config.relay.url.clone(),
                bounty_cache.clone(),
            )
            .with_db(db_pool.clone()),
        ));
        registry.register(Arc::new(bounty_agent_tools::AssessBountyFitTool::new(
            db_pool.clone(),
            bounty_cache,
        )));
        registry.register(Arc::new(bounty_agent_tools::ClaimBountyTool::new(
            config.relay.url.clone(),
            db_pool.clone(),
        )));
        registry.register(Arc::new(bounty_agent_tools::SubmitBountyProofTool::new(
            config.relay.url.clone(),
            db_pool.clone(),
        )));
        registry.register(Arc::new(bounty_agent_tools::CheckBountyStatusTool::new(
            config.relay.url.clone(),
            db_pool.clone(),
        )));

        registry
    }
}

/// Implement the amos-core PackageToolRegistry trait so packages
/// can register tools without depending on amos-harness.
impl PackageToolRegistry for ToolRegistry {
    fn register_package_tool(&mut self, tool: Arc<dyn Tool>, package: &str) {
        ToolRegistry::register_package_tool(self, tool, package);
    }
}

/// Helper macro to define tool parameter schema
#[macro_export]
macro_rules! tool_schema {
    ($($name:expr => $schema:tt),* $(,)?) => {
        serde_json::json!({
            "type": "object",
            "properties": {
                $(
                    $name: $schema
                ),*
            },
            "required": []
        })
    };
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── ToolResult ───────────────────────────────────────────────────

    #[test]
    fn tool_result_success() {
        let result = ToolResult::success(json!({"count": 42}));
        assert!(result.success);
        assert!(result.data.is_some());
        assert!(result.error.is_none());
        assert!(result.metadata.is_none());
    }

    #[test]
    fn tool_result_error() {
        let result = ToolResult::error("something went wrong".to_string());
        assert!(!result.success);
        assert!(result.data.is_none());
        assert_eq!(result.error.unwrap(), "something went wrong");
    }

    #[test]
    fn tool_result_success_with_metadata() {
        let result =
            ToolResult::success_with_metadata(json!({"items": []}), json!({"total": 0, "page": 1}));
        assert!(result.success);
        assert!(result.data.is_some());
        assert!(result.metadata.is_some());
        assert_eq!(result.metadata.unwrap()["total"], 0);
    }

    #[test]
    fn tool_result_serde_roundtrip() {
        let result = ToolResult::success(json!({"key": "value"}));
        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&serialized).unwrap();
        assert!(deserialized.success);
        assert_eq!(deserialized.data.unwrap()["key"], "value");
    }

    // ── ToolCategory ─────────────────────────────────────────────────

    #[test]
    fn tool_category_as_str() {
        assert_eq!(ToolCategory::Platform.as_str(), "platform");
        assert_eq!(ToolCategory::Canvas.as_str(), "canvas");
        assert_eq!(ToolCategory::Apps.as_str(), "apps");
        assert_eq!(ToolCategory::Web.as_str(), "web");
        assert_eq!(ToolCategory::System.as_str(), "system");
        assert_eq!(ToolCategory::Memory.as_str(), "memory");
        assert_eq!(ToolCategory::Knowledge.as_str(), "knowledge");
        assert_eq!(ToolCategory::OpenClaw.as_str(), "openclaw");
        assert_eq!(ToolCategory::Integration.as_str(), "integration");
        assert_eq!(ToolCategory::Schema.as_str(), "schema");
        assert_eq!(ToolCategory::TaskQueue.as_str(), "task_queue");
        assert_eq!(ToolCategory::Document.as_str(), "document");
        assert_eq!(ToolCategory::ImageGen.as_str(), "image_gen");
        assert_eq!(ToolCategory::Automation.as_str(), "automation");
        assert_eq!(ToolCategory::Education.as_str(), "education");
        assert_eq!(ToolCategory::BountyAgent.as_str(), "bounty_agent");
        assert_eq!(ToolCategory::Other.as_str(), "other");
    }

    #[test]
    fn tool_category_equality() {
        assert_eq!(ToolCategory::Platform, ToolCategory::Platform);
        assert_ne!(ToolCategory::Platform, ToolCategory::Canvas);
    }
}
