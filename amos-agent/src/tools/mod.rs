//! Local agent tools.
//!
//! These are tools that run locally inside the agent process, without
//! needing the harness. They provide the agent with autonomous capability:
//!
//! - **think**: Internal reasoning / chain-of-thought (no side effects)
//! - **remember**: Store a fact/insight to persistent local memory
//! - **recall**: Search persistent memory for relevant information
//! - **plan**: Create or update a structured plan for the current task
//! - **web_search**: Search the web via Brave Search API
//! - **read_file**: Read a local file
//! - **write_file**: Write content to a local file

pub mod file_tools;
pub mod git_tools;
pub mod memory_tools;
pub mod plan;
pub mod think;
pub mod web_search;

use crate::harness_client::HarnessClient;
use crate::memory::MemoryStore;
use amos_core::types::ToolDefinition;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// All local tool definitions for the LLM.
pub fn local_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        think::definition(),
        memory_tools::remember_definition(),
        memory_tools::recall_definition(),
        plan::definition(),
        web_search::definition(),
        file_tools::read_file_definition(),
        file_tools::write_file_definition(),
        git_tools::git_status_definition(),
    ]
}

/// Convert tool definitions to the JSON schema format expected by LLM APIs.
pub fn tool_definitions_to_json(defs: &[ToolDefinition]) -> Vec<serde_json::Value> {
    defs.iter()
        .map(|d| {
            json!({
                "name": d.name,
                "description": d.description,
                "inputSchema": d.input_schema,
            })
        })
        .collect()
}

/// Extract the raw JSON Schema for a tool, regardless of how it was
/// delivered. This is the function every non-Bedrock provider should use
/// when it needs the schema to hand to its API.
///
/// Handles both shapes we actually see at runtime:
///   - **Harness tools** arrive with `inputSchema: {json: <schema>}` — the
///     envelope the harness uses because Bedrock wants it that way.
///   - **Agent-local tools** arrive with `inputSchema: <schema>` directly.
///
/// Anthropic, OpenAI, and Vertex all want the raw schema under their own
/// field name (`input_schema` / `parameters`) with no envelope. Without
/// this unwrap, harness tools show up at those providers malformed —
/// a silent data bug that only surfaces when a customer uses BYOK with
/// harness tools attached.
pub fn extract_tool_schema(tool: &serde_json::Value) -> serde_json::Value {
    let raw = tool
        .get("inputSchema")
        .or_else(|| tool.get("input_schema"))
        .cloned()
        .unwrap_or_else(|| json!({"type": "object", "properties": {}}));

    // If the schema is wrapped in Bedrock's `{json: ...}` envelope, unwrap.
    match raw.get("json") {
        Some(inner) => inner.clone(),
        None => raw,
    }
}

/// Context needed by tools during execution.
pub struct ToolContext {
    pub memory: Arc<Mutex<MemoryStore>>,
    pub brave_api_key: Option<String>,
    pub work_dir: String,
    /// Optional harness client for memory write-through and fallback search.
    pub harness: Option<Arc<RwLock<HarnessClient>>>,
}

/// Execute a local tool by name with the given input.
pub async fn execute_local_tool(
    name: &str,
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    match name {
        "think" => Ok(think::execute(input)),
        "remember" => memory_tools::remember(input, &ctx.memory, ctx.harness.as_ref())
            .await
            .map_err(|e| e.to_string()),
        "recall" => memory_tools::recall(input, &ctx.memory, ctx.harness.as_ref())
            .await
            .map_err(|e| e.to_string()),
        "plan" => Ok(plan::execute(input)),
        "web_search" => web_search::execute(input, ctx.brave_api_key.as_deref()).await,
        "read_file" => file_tools::read_file(input, &ctx.work_dir),
        "write_file" => file_tools::write_file(input, &ctx.work_dir),
        "git_status" => git_tools::execute(input, &ctx.work_dir),
        _ => Err(format!("Unknown local tool: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_unwraps_bedrock_json_envelope() {
        // Harness-shape tool — schema sits inside `{json: ...}`
        let t = json!({
            "name": "harness_thing",
            "description": "a harness tool",
            "inputSchema": { "json": { "type": "object", "properties": { "x": { "type": "string" } } } }
        });
        let s = extract_tool_schema(&t);
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["x"].is_object());
        assert!(s.get("json").is_none(), "envelope must be removed");
    }

    #[test]
    fn extract_passes_through_raw_schema() {
        // Agent-local-shape tool — schema directly under inputSchema
        let t = json!({
            "name": "think",
            "description": "internal reasoning",
            "inputSchema": { "type": "object", "properties": { "thought": { "type": "string" } } }
        });
        let s = extract_tool_schema(&t);
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["thought"].is_object());
    }

    #[test]
    fn extract_accepts_snake_case_alias() {
        // Some older code paths use `input_schema` instead of `inputSchema`.
        let t = json!({
            "name": "legacy",
            "description": "",
            "input_schema": { "type": "object" }
        });
        let s = extract_tool_schema(&t);
        assert_eq!(s["type"], "object");
    }

    #[test]
    fn extract_defaults_to_empty_object_schema_when_missing() {
        let t = json!({ "name": "noschema", "description": "" });
        let s = extract_tool_schema(&t);
        assert_eq!(s["type"], "object");
        assert!(s["properties"].is_object());
    }
}
