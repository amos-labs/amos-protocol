//! HTTP routes for the agent's API server.
//!
//! When running as a service (Docker deployment), the agent exposes:
//! - `POST /api/v1/chat` - SSE streaming chat endpoint
//! - `GET /health` - Health check
//! - `GET /.well-known/agent.json` - Agent Card (served separately)
//!
//! The chat endpoint accepts a JSON body and returns Server-Sent Events.

use crate::{
    agent_loop::{self, AgentEvent, LoopConfig},
    harness_client::HarnessClient,
    provider::ModelProvider,
    tools::ToolContext,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Shared state for route handlers.
#[derive(Clone)]
pub struct AgentState {
    pub provider: Arc<dyn ModelProvider>,
    pub tool_ctx: Arc<ToolContext>,
    pub harness: Arc<RwLock<HarnessClient>>,
    pub loop_config: LoopConfig,
}

/// Chat request body.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub session_id: Option<String>,
    /// BYOK: provider type override (e.g. "anthropic", "openai")
    #[serde(default)]
    pub provider_type: Option<String>,
    /// BYOK: API base URL override
    #[serde(default)]
    pub api_base: Option<String>,
    /// BYOK: API key override
    #[serde(default)]
    pub api_key: Option<String>,
    /// BYOK: model ID override
    #[serde(default)]
    pub model_id: Option<String>,
    /// Content blocks from processed attachments (images, documents).
    /// Injected by the harness proxy after processing uploaded files.
    #[serde(default)]
    pub content_blocks: Option<Vec<amos_core::types::ContentBlock>>,
    /// When true, the agent restricts itself to research and planning tools.
    #[serde(default)]
    pub plan_mode: Option<bool>,
    /// Conversation history from prior messages in this session.
    /// Injected by the harness proxy to give the agent conversational context.
    #[serde(default)]
    pub history: Option<Vec<amos_core::types::Message>>,
    /// Workspace context (collections, canvases, sites, knowledge base).
    /// Injected by the harness proxy for new sessions so the agent doesn't need
    /// to call harness_get_workspace_summary on first message.
    #[serde(default)]
    pub workspace_context: Option<serde_json::Value>,
    /// System prompts from enabled packages (education, CRM, etc.).
    /// Injected by the harness proxy. Appended to the system prompt under
    /// a "## Package-Specific Instructions" heading.
    #[serde(default)]
    pub package_prompts: Option<Vec<String>>,
    /// Task context type (e.g. "bounty") — adjusts compaction window and
    /// enables self-evaluation for multi-step autonomous tasks.
    #[serde(default)]
    pub task_context: Option<String>,
}

/// Chat response for non-streaming mode.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub text: String,
    pub session_id: Option<String>,
}

/// System prompt prepended when plan mode is active.
const PLAN_MODE_PROMPT: &str = r#"You are in PLAN MODE. The user wants to discuss and plan before you build anything.

Rules:
1. DO NOT call any tools that create, update, or delete data (no create_app, create_site, define_collection, create_record, create_page, create_automation, update_record, delete_record, etc.)
2. You MAY use research tools: get_workspace_summary, knowledge_search, query_records, list_collections, think, plan, web_search, list_sites, recall
3. Analyze what the user wants and present a clear, structured plan:
   - What collections/schemas will be created (fields, types)
   - What app views or site pages will be built
   - What automations or integrations are needed
   - Estimated number of steps
4. Ask clarifying questions if requirements are ambiguous
5. When the user approves the plan, tell them to turn off Plan Mode to begin building."#;

/// Create the agent HTTP router.
pub fn agent_router(state: AgentState) -> Router {
    Router::new()
        .route("/api/v1/chat", post(chat_sse))
        .route("/health", get(health))
        .with_state(state)
}

/// Health check endpoint.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "amos-agent",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// SSE streaming chat endpoint.
///
/// Accepts a chat message and returns a stream of Server-Sent Events
/// corresponding to the agent's think-act-observe loop.
async fn chat_sse(
    State(state): State<AgentState>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, StatusCode> {
    info!(message_len = req.message.len(), provider = ?req.provider_type, "Chat request received");

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    // If BYOK provider config is supplied, create a per-request provider.
    // Otherwise fall back to the default provider from startup config.
    let provider: Arc<dyn crate::provider::ModelProvider> =
        if let Some(ref provider_type) = req.provider_type {
            match crate::provider::create_provider(
                provider_type,
                req.model_id.as_deref().unwrap_or(""),
                req.api_base.as_deref(),
                req.api_key.as_deref(),
            ) {
                Ok(p) => {
                    info!(provider = %provider_type, "Using BYOK provider for this request");
                    Arc::from(p)
                }
                Err(e) => {
                    error!("Failed to create BYOK provider: {e}");
                    // Fall back to default
                    state.provider.clone()
                }
            }
        } else {
            state.provider.clone()
        };

    // Override model_id in loop config if provided
    let mut loop_config = state.loop_config.clone();
    if let Some(ref model_id) = req.model_id {
        loop_config.model_id = model_id.clone();
    }

    // Prepend plan mode instructions when active
    if req.plan_mode.unwrap_or(false) {
        loop_config.system_prompt =
            format!("{}\n\n{}", PLAN_MODE_PROMPT, loop_config.system_prompt);
    }

    // Widen compaction window for bounty execution (more context needed)
    if req.task_context.as_deref() == Some("bounty") {
        loop_config.compaction.preserve_recent = 15;
    }

    // Append package-specific instructions to the system prompt
    if let Some(ref prompts) = req.package_prompts {
        if !prompts.is_empty() {
            let combined = prompts.join("\n\n---\n\n");
            loop_config.system_prompt = format!(
                "{}\n\n## Package-Specific Instructions\n\n{}",
                loop_config.system_prompt, combined
            );
            info!(count = prompts.len(), "Applied package system prompts");
        }
    }

    let tool_ctx = state.tool_ctx.clone();
    let harness = state.harness.clone();
    let message = req.message.clone();
    let content_blocks = req.content_blocks;
    let history = req.history;
    let workspace_context = req.workspace_context;

    // Run the agent loop in a background task
    tokio::spawn(async move {
        let h = harness.read().await;
        let result = agent_loop::run_agent_loop(
            &loop_config,
            provider.as_ref(),
            &tool_ctx,
            Some(&h),
            &message,
            content_blocks,
            history,
            workspace_context,
            Some(event_tx.clone()),
        )
        .await;

        if let Err(e) = &result {
            error!("Agent loop error: {e}");
            // Send error event so the frontend can display it
            let _ = event_tx
                .send(agent_loop::AgentEvent::Error {
                    message: format!("{e}"),
                })
                .await;
        }
    });

    // Convert the mpsc receiver into an SSE stream
    let stream = async_stream::stream! {
        while let Some(event) = event_rx.recv().await {
            let event_type = match &event {
                AgentEvent::TurnStart { .. } => "turn_start",
                AgentEvent::TextDelta { .. } => "message_delta",
                AgentEvent::ToolStart { .. } => "tool_start",
                AgentEvent::ToolEnd { .. } => "tool_end",
                AgentEvent::ToolInputDelta { .. } => "tool_input_delta",
                AgentEvent::TurnEnd { .. } => "turn_end",
                AgentEvent::Done { .. } => "agent_end",
                AgentEvent::Compacted { .. } => "compacted",
                AgentEvent::HookDenied { .. } => "hook_denied",
                AgentEvent::Error { .. } => "error",
            };

            let data = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().event(event_type).data(data));
        }
    };

    Ok(Sse::new(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_deserialization() {
        let json = r#"{"message": "hello"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "hello");
        assert!(req.session_id.is_none());
    }

    #[test]
    fn test_chat_request_with_session() {
        let json = r#"{"message": "hello", "session_id": "abc-123"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.session_id, Some("abc-123".to_string()));
    }

    #[test]
    fn test_chat_request_with_plan_mode() {
        let json = r#"{"message": "Build me a CRM", "plan_mode": true}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.plan_mode, Some(true));
    }

    #[test]
    fn test_chat_request_plan_mode_defaults_to_none() {
        let json = r#"{"message": "hello"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert!(req.plan_mode.is_none());
    }

    #[test]
    fn test_plan_mode_prompt_content() {
        assert!(PLAN_MODE_PROMPT.contains("PLAN MODE"));
        assert!(PLAN_MODE_PROMPT.contains("DO NOT call"));
    }
}
