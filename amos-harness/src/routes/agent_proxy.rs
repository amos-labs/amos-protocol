//! Reverse proxy for agent API endpoints.
//!
//! The harness frontend (app.js) sends chat requests to `/api/v1/agent/chat`,
//! but the actual agent service runs as a sidecar on a separate port (default 3100).
//! This module proxies those requests through to the agent, preserving the SSE
//! streaming response for chat.
//!
//! **BYOK injection**: Before forwarding to the agent, the proxy looks up the active
//! LLM provider from the database. If one is configured, it decrypts the API key
//! from the credential vault and injects `provider_type`, `api_base`, `api_key`,
//! and `model_id` into the JSON body. The agent then uses these to create a
//! per-request provider instead of its default Bedrock provider.
//!
//! Endpoints proxied:
//!   - `POST /api/v1/agent/chat`        → agent `POST /api/v1/chat` (with BYOK + session persistence)
//!   - `POST /api/v1/agent/chat/cancel` → stub (agent doesn't support cancel yet)
//!   - `GET  /api/v1/agent/sessions`    → list recent sessions from DB
//!   - `GET  /api/v1/agent/sessions/:id` → load session messages from DB

use crate::documents::ExtractionResult;
use crate::routes::credentials;
use crate::routes::uploads;
use crate::state::AppState;
use crate::tools::knowledge_tools;
use amos_core::types::{ContentBlock, DocumentSource, ImageSource};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Build agent proxy routes.
///
/// All routes are relative — they get nested under `/api/v1/agent` in `build_routes()`.
pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/chat", post(proxy_chat))
        .route("/chat/cancel", post(cancel_chat))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
}

/// Resolve the agent service URL from environment.
///
/// In ECS Fargate, the agent runs as a sidecar container in the same task,
/// so it's reachable at `localhost:3100`. In local dev, the agent may run
/// on any host/port.
fn agent_base_url() -> String {
    std::env::var("AGENT_URL").unwrap_or_else(|_| "http://localhost:3100".to_string())
}

/// Proxy `POST /api/v1/agent/chat` → agent `POST /api/v1/chat`.
///
/// This is an SSE streaming proxy: we forward the JSON body to the agent,
/// then stream the agent's SSE response byte-for-byte back to the browser.
///
/// **BYOK injection**: Before forwarding, we look up the active LLM provider
/// from the database. If one exists, we decrypt its API key and inject
/// `provider_type`, `api_base`, `api_key`, and `model_id` into the JSON body.
/// This lets the agent create a per-request provider instead of its default.
async fn proxy_chat(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Response, StatusCode> {
    let agent_url = format!("{}/api/v1/chat", agent_base_url());

    // ── Session persistence: parse request, create/continue session ────
    let parsed: serde_json::Value =
        serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));

    let user_message_text = parsed
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let requested_session_id = parsed
        .get("session_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    // Create or continue a session.
    let session_id = match requested_session_id {
        Some(id) => id,
        None => {
            // Derive a title from the first ~60 chars of the user message.
            let title = if user_message_text.len() > 60 {
                Some(format!("{}...", &user_message_text[..57]))
            } else if !user_message_text.is_empty() {
                Some(user_message_text.clone())
            } else {
                None
            };
            match crate::sessions::create_session(&state.db_pool, title.as_deref()).await {
                Ok(session) => session.id,
                Err(e) => {
                    warn!("Failed to create session: {e}");
                    Uuid::new_v4() // Fallback: proceed without persistence
                }
            }
        }
    };

    // Save the user message.
    if !user_message_text.is_empty() {
        let seq = crate::sessions::message_count(&state.db_pool, session_id)
            .await
            .unwrap_or(0);
        let user_msg = amos_core::types::Message {
            role: amos_core::types::Role::User,
            content: vec![ContentBlock::Text {
                text: user_message_text,
            }],
            tool_use_id: None,
            timestamp: chrono::Utc::now(),
        };
        if let Err(e) =
            crate::sessions::save_messages(&state.db_pool, session_id, &[user_msg], seq).await
        {
            warn!("Failed to save user message: {e}");
        }
    }

    // ── Conversation history + workspace context injection ─────────────
    let mut json_body: serde_json::Value =
        serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));

    // For continuing sessions, load prior messages as history for the agent
    if requested_session_id.is_some() {
        match crate::sessions::load_messages(&state.db_pool, session_id).await {
            Ok(history) if !history.is_empty() => {
                if let Ok(hist_val) = serde_json::to_value(&history) {
                    json_body["history"] = hist_val;
                    info!(session_id = %session_id, messages = history.len(), "Injected conversation history");
                }
            }
            Ok(_) => {} // empty history, skip
            Err(e) => warn!("Failed to load session history: {e}"),
        }
    }

    // For new sessions, inject workspace context directly so the agent
    // doesn't need to call harness_get_workspace_summary as its first action.
    if requested_session_id.is_none() {
        match state
            .tool_registry
            .execute("get_workspace_summary", serde_json::json!({}))
            .await
        {
            Ok(result) if result.success => {
                if let Some(data) = result.data {
                    json_body["workspace_context"] = data;
                    info!("Injected workspace context for new session");
                }
            }
            Ok(_) => {} // tool returned error, skip
            Err(e) => warn!("Failed to load workspace context: {e}"),
        }
    }

    // ── Specialist context injection ──────────────────────────────────
    // For new sessions, inject specialist info so the agent knows what's running
    // from the first message — no need to call list_available_specialists.
    if requested_session_id.is_none() {
        if let Some(ref orchestrator) = state.orchestrator {
            orchestrator.refresh_discovery().await;
            let siblings = orchestrator.proxy.get_siblings().await;
            if !siblings.is_empty() {
                use crate::orchestrator::provisioning_tools::SPECIALIST_CATALOG;

                let active: Vec<serde_json::Value> = siblings
                    .iter()
                    .map(|s| {
                        let friendly_name = SPECIALIST_CATALOG
                            .iter()
                            .find(|e| s.packages.contains(&e.slug.to_string()))
                            .map(|e| e.friendly_name)
                            .unwrap_or("Specialist");
                        serde_json::json!({
                            "name": friendly_name,
                            "harness_id": s.harness_id,
                            "packages": s.packages,
                            "healthy": s.healthy.unwrap_or(false),
                        })
                    })
                    .collect();
                json_body["specialists"] = serde_json::json!(active);
                info!(
                    count = active.len(),
                    "Injected specialist context for new session"
                );
            }
        }
    }

    // ── Package system prompt injection ─────────────────────────────────
    // Query enabled packages' system prompts and inject as `package_prompts`
    // array. The agent appends these to the system prompt under a
    // "## Package-Specific Instructions" heading.
    // Package prompts are wrapped with data boundaries to limit injection risk.
    match sqlx::query_as::<_, (String,)>(
        "SELECT system_prompt FROM packages WHERE enabled = true AND system_prompt IS NOT NULL",
    )
    .fetch_all(&state.db_pool)
    .await
    {
        Ok(rows) if !rows.is_empty() => {
            let prompts: Vec<String> = rows
                .into_iter()
                .map(|(p,)| crate::prompt_guard::sanitize("package_prompt", &p, 8000))
                .collect();
            json_body["package_prompts"] = serde_json::json!(prompts);
            info!(count = prompts.len(), "Injected package system prompts");
        }
        Ok(_) => {} // no enabled packages with prompts
        Err(e) => warn!("Failed to load package prompts: {e}"),
    }

    let body = serde_json::to_string(&json_body).unwrap_or(body);

    // ── Attachment & BYOK processing ──────────────────────────────────
    let body = match process_attachments(&state, &body).await {
        Ok(b) => b,
        Err(e) => {
            warn!(
                "Attachment processing failed ({}), forwarding original body",
                e
            );
            body
        }
    };

    let enriched_body = match inject_llm_provider(&state, &body).await {
        Ok(b) => b,
        Err(e) => {
            warn!(
                "LLM provider injection skipped ({}), forwarding original body",
                e
            );
            body
        }
    };

    // Inject session_id into the body so the agent knows about it.
    let enriched_body = match serde_json::from_str::<serde_json::Value>(&enriched_body) {
        Ok(mut json) => {
            json["session_id"] = serde_json::json!(session_id.to_string());
            serde_json::to_string(&json).unwrap_or(enriched_body)
        }
        Err(_) => enriched_body,
    };

    info!(url = %agent_url, session_id = %session_id, byok = enriched_body.contains("provider_type"), "Proxying chat request to agent");

    let client = reqwest::Client::new();
    let agent_response = match client
        .post(&agent_url)
        .header("Content-Type", "application/json")
        .body(enriched_body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to connect to agent service at {}: {}", agent_url, e);
            let error_event = "event: error\ndata: {\"type\":\"error\",\"message\":\"Agent service is not available. Please try again shortly or contact support.\"}\n\n".to_string();
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from(error_event))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let status = agent_response.status();

    if !status.is_success() {
        warn!(status = %status, "Agent returned non-success status");
        let error_body = agent_response.text().await.unwrap_or_default();
        return Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(error_body))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── Stream SSE with keepalive + message collection ────────────────
    // Prepend a chat_meta event so the frontend learns the session_id.
    let chat_meta = format!(
        "event: chat_meta\ndata: {{\"session_id\":\"{}\"}}\n\n",
        session_id
    );

    let data_stream = agent_response.bytes_stream();
    let stream = sse_with_keepalive_and_persist(
        data_stream,
        chat_meta,
        state.db_pool.clone(),
        session_id,
        state.activity_counters.clone(),
    );

    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Look up the active LLM provider from the database, decrypt its API key,
/// and inject `provider_type`, `api_base`, `api_key`, `model_id` into the
/// chat request JSON body.
///
/// Returns the enriched JSON string, or an error string if no provider is
/// configured or decryption fails.
async fn inject_llm_provider(state: &AppState, body: &str) -> Result<String, String> {
    // Parse the incoming JSON body
    let mut json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))?;

    // Don't override if the client already supplied provider config
    if json.get("provider_type").is_some() {
        return Ok(body.to_string());
    }

    // Check the configured provider mode from harness_settings
    let provider_mode = super::settings::get_setting(state, "llm_provider_mode")
        .await
        .unwrap_or_else(|| "shared_bedrock".to_string());

    let obj = json
        .as_object_mut()
        .ok_or_else(|| "body is not a JSON object".to_string())?;

    match provider_mode.as_str() {
        "shared_bedrock" => {
            // Shared Bedrock: inject provider_type and model_id only.
            // The agent picks up AWS creds from its environment (ECS task role).
            let model = super::settings::get_setting(state, "llm_model")
                .await
                .unwrap_or_else(|| "us.anthropic.claude-sonnet-4-6".to_string());

            obj.insert(
                "provider_type".to_string(),
                serde_json::Value::String("bedrock".to_string()),
            );
            obj.insert(
                "model_id".to_string(),
                serde_json::Value::String(model.clone()),
            );

            info!(
                provider = "bedrock",
                model = %model,
                "Injected shared Bedrock provider config"
            );
        }
        "byok" | _ => {
            // BYOK: look up the active LLM provider and inject full credentials.
            let provider = sqlx::query_as::<_, crate::routes::llm_providers::LlmProviderRow>(
                "SELECT * FROM llm_providers WHERE is_active = true LIMIT 1",
            )
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|e| format!("DB error: {e}"))?
            .ok_or_else(|| "no active BYOK provider configured".to_string())?;

            let credential_id = provider
                .credential_id
                .ok_or_else(|| "active provider has no credential".to_string())?;

            // Decrypt the API key from the credential vault
            let api_key =
                credentials::decrypt_credential(&state.db_pool, &state.vault, credential_id)
                    .await
                    .map_err(|status| format!("decrypt failed: HTTP {}", status.as_u16()))?;

            obj.insert(
                "provider_type".to_string(),
                serde_json::Value::String(provider.name.clone()),
            );
            obj.insert(
                "api_base".to_string(),
                serde_json::Value::String(provider.api_base.clone()),
            );
            obj.insert("api_key".to_string(), serde_json::Value::String(api_key));
            obj.insert(
                "model_id".to_string(),
                serde_json::Value::String(provider.default_model.clone()),
            );

            info!(
                provider = %provider.name,
                model = %provider.default_model,
                "Injected BYOK provider config into chat request"
            );
        }
    }

    serde_json::to_string(&json).map_err(|e| format!("JSON serialize: {e}"))
}

/// Process attachments from the chat request body.
///
/// Extracts the `attachments` array (list of upload UUIDs), loads each file,
/// converts it to a `ContentBlock`, and injects the blocks as a `content_blocks`
/// JSON array on the request body. Removes `attachments` before forwarding.
async fn process_attachments(state: &AppState, body: &str) -> Result<String, String> {
    let mut json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))?;

    let attachments = match json.get("attachments").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr.clone(),
        _ => return Ok(body.to_string()), // No attachments — pass through unchanged
    };

    info!(count = attachments.len(), "Processing chat attachments");

    let b64 = base64::engine::general_purpose::STANDARD;
    let mut content_blocks: Vec<ContentBlock> = Vec::new();

    for att_val in &attachments {
        let id_str = att_val
            .as_str()
            .ok_or_else(|| "attachment ID is not a string".to_string())?;
        let upload_id =
            Uuid::parse_str(id_str).map_err(|e| format!("invalid attachment UUID: {e}"))?;

        // Load the file data with a 30s timeout
        let load_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            uploads::load_upload_data(&state.db_pool, &state.storage, upload_id),
        )
        .await;

        let (content_type, filename, data) = match load_result {
            Ok(Ok(tuple)) => tuple,
            Ok(Err(status)) => {
                warn!(%upload_id, status = %status, "Failed to load attachment");
                content_blocks.push(ContentBlock::Text {
                    text: format!(
                        "[Attachment {} could not be loaded]",
                        filename_or_id(id_str)
                    ),
                });
                continue;
            }
            Err(_) => {
                warn!(%upload_id, "Attachment load timed out (30s)");
                content_blocks.push(ContentBlock::Text {
                    text: format!(
                        "[Attachment {} timed out during loading]",
                        filename_or_id(id_str)
                    ),
                });
                continue;
            }
        };

        info!(%upload_id, %content_type, %filename, size = data.len(), "Processing attachment");

        // Route by content type
        let block = if content_type.starts_with("image/") {
            // Direct image — send as base64 Image block
            ContentBlock::Image {
                source: ImageSource {
                    source_type: "base64".to_string(),
                    media_type: content_type.clone(),
                    data: b64.encode(&data),
                },
            }
        } else if content_type == "application/pdf"
            || content_type
                == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            || content_type == "application/msword"
            || content_type == "text/html"
            || content_type == "application/xhtml+xml"
            || filename.to_lowercase().ends_with(".pdf")
            || filename.to_lowercase().ends_with(".docx")
            || filename.to_lowercase().ends_with(".html")
            || filename.to_lowercase().ends_with(".htm")
        {
            // Document — run extraction pipeline
            let extract_result = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                state
                    .document_processor
                    .extract(&data, &filename, &content_type),
            )
            .await;

            match extract_result {
                Ok(ExtractionResult::Text(text)) => {
                    info!(%filename, chars = text.len(), "Extracted text from document");
                    // Background-ingest into knowledge base for RAG
                    if let Some(ref emb_svc) = state.embedding_service {
                        let pool = state.db_pool.clone();
                        let svc = emb_svc.clone();
                        let title = filename.clone();
                        let doc_text = text.clone();
                        tokio::spawn(async move {
                            knowledge_tools::background_ingest(
                                pool,
                                svc,
                                title,
                                doc_text,
                                "upload".to_string(),
                            )
                            .await;
                        });
                    }
                    {
                        // Wrap extracted text to prevent prompt injection from document content
                        let wrapped =
                            crate::prompt_guard::sanitize("uploaded_document", &text, 50_000);
                        ContentBlock::Text {
                            text: format!("[Document: {}]\n\n{}", filename, wrapped),
                        }
                    }
                }
                Ok(ExtractionResult::Pages(pages)) => {
                    let combined: String = pages
                        .iter()
                        .map(|p| format!("--- Page {} ---\n{}", p.page_number, p.text))
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    info!(%filename, pages = pages.len(), "Extracted pages from document");
                    // Background-ingest into knowledge base for RAG
                    if let Some(ref emb_svc) = state.embedding_service {
                        let pool = state.db_pool.clone();
                        let svc = emb_svc.clone();
                        let title = filename.clone();
                        let doc_text = combined.clone();
                        tokio::spawn(async move {
                            knowledge_tools::background_ingest(
                                pool,
                                svc,
                                title,
                                doc_text,
                                "upload".to_string(),
                            )
                            .await;
                        });
                    }
                    {
                        // Wrap extracted text to prevent prompt injection from document content
                        let wrapped =
                            crate::prompt_guard::sanitize("uploaded_document", &combined, 100_000);
                        ContentBlock::Text {
                            text: format!("[Document: {}]\n\n{}", filename, wrapped),
                        }
                    }
                }
                Ok(ExtractionResult::RawDocument(format, name, raw_bytes)) => {
                    info!(%filename, format, "Sending raw document for vision analysis");
                    ContentBlock::Document {
                        source: DocumentSource {
                            format,
                            name,
                            data: b64.encode(&raw_bytes),
                        },
                    }
                }
                Ok(ExtractionResult::RenderedPages(pages)) => {
                    // Each rendered page is an image — send the first few
                    info!(%filename, pages = pages.len(), "Document rendered to page images");
                    let mut first = true;
                    for (media_type, img_bytes) in pages.into_iter().take(10) {
                        if !first {
                            content_blocks.push(ContentBlock::Image {
                                source: ImageSource {
                                    source_type: "base64".to_string(),
                                    media_type,
                                    data: b64.encode(&img_bytes),
                                },
                            });
                        } else {
                            first = false;
                            content_blocks.push(ContentBlock::Image {
                                source: ImageSource {
                                    source_type: "base64".to_string(),
                                    media_type,
                                    data: b64.encode(&img_bytes),
                                },
                            });
                        }
                    }
                    continue; // Already pushed blocks
                }
                Ok(ExtractionResult::Unsupported) => {
                    warn!(%filename, %content_type, "Document extraction unsupported");
                    ContentBlock::Text {
                        text: format!(
                            "[Document '{}' ({}): format not supported for text extraction]",
                            filename, content_type
                        ),
                    }
                }
                Err(_) => {
                    warn!(%filename, "Document extraction timed out (30s)");
                    ContentBlock::Text {
                        text: format!("[Document '{}': processing timed out]", filename),
                    }
                }
            }
        } else {
            // Unsupported file type
            ContentBlock::Text {
                text: format!(
                    "[Attachment '{}' ({}): file type not supported for inline viewing]",
                    filename, content_type
                ),
            }
        };

        content_blocks.push(block);
    }

    // Inject content_blocks and remove attachments from the JSON body
    let obj = json
        .as_object_mut()
        .ok_or_else(|| "body is not a JSON object".to_string())?;
    obj.remove("attachments");

    if !content_blocks.is_empty() {
        let blocks_json = serde_json::to_value(&content_blocks)
            .map_err(|e| format!("failed to serialize content blocks: {e}"))?;
        obj.insert("content_blocks".to_string(), blocks_json);
        info!(
            blocks = content_blocks.len(),
            "Injected content blocks into chat request"
        );
    }

    serde_json::to_string(&json).map_err(|e| format!("JSON serialize: {e}"))
}

/// Helper: return filename if parseable, otherwise the raw ID string.
fn filename_or_id(id: &str) -> &str {
    id
}

/// Stub for `POST /api/v1/agent/chat/cancel`.
///
/// The agent doesn't support cancellation yet. Return 200 so the frontend
/// doesn't show an error — the AbortController on the client side will
/// close the SSE stream regardless.
async fn cancel_chat() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "message": "Cancel acknowledged (client-side abort)"
    }))
}

/// List recent sessions for the sidebar.
async fn list_sessions(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let limit: i64 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);

    match crate::sessions::list_sessions(&state.db_pool, limit).await {
        Ok(sessions) => Json(serde_json::json!({ "sessions": sessions })).into_response(),
        Err(e) => {
            warn!("Failed to list sessions: {e}");
            Json(serde_json::json!({ "sessions": [] })).into_response()
        }
    }
}

/// Load a session and its messages for the frontend to rebuild the conversation.
async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid session ID" })),
            )
                .into_response()
        }
    };

    // Verify session exists
    match crate::sessions::get_session(&state.db_pool, session_id).await {
        Ok(Some(_session)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Session not found" })),
            )
                .into_response()
        }
        Err(e) => {
            warn!("Failed to get session {session_id}: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to load session" })),
            )
                .into_response();
        }
    };

    // Load messages
    match crate::sessions::load_messages(&state.db_pool, session_id).await {
        Ok(messages) => {
            // Convert to the format the frontend expects: array of {role, content}
            let formatted: Vec<serde_json::Value> = messages
                .into_iter()
                .map(|msg| {
                    let role = match msg.role {
                        amos_core::types::Role::User => "user",
                        amos_core::types::Role::Assistant => "assistant",
                        amos_core::types::Role::System => "system",
                        amos_core::types::Role::Tool => "tool",
                    };
                    serde_json::json!({
                        "role": role,
                        "content": msg.content,
                    })
                })
                .collect();

            Json(serde_json::json!({ "messages": formatted })).into_response()
        }
        Err(e) => {
            warn!("Failed to load messages for session {session_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to load messages" })),
            )
                .into_response()
        }
    }
}

/// Wrap an SSE byte stream with keepalive comments, a prepended chat_meta event,
/// and background persistence of the assistant response.
///
/// Emits `:keepalive\n\n` every 15 seconds when the upstream hasn't sent
/// any data. Also collects `message_delta` content from the stream and saves
/// the assistant message to the database when the stream ends.
fn sse_with_keepalive_and_persist(
    upstream: impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
    chat_meta_event: String,
    db_pool: sqlx::PgPool,
    session_id: Uuid,
    activity_counters: std::sync::Arc<crate::platform_sync::ActivityCounters>,
) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    use futures::StreamExt;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);

    tokio::spawn(async move {
        // Send the chat_meta event first so the frontend learns the session_id.
        if tx.send(Ok(Bytes::from(chat_meta_event))).await.is_err() {
            return;
        }

        let mut upstream = std::pin::pin!(upstream);
        let keepalive_interval = std::time::Duration::from_secs(15);

        // Accumulate assistant text from message_delta events.
        let mut assistant_text = String::new();
        // Token usage from agent_end event.
        let mut final_input_tokens: i64 = 0;
        let mut final_output_tokens: i64 = 0;
        let mut final_model_id: Option<String> = None;
        // Buffer for incomplete SSE lines across chunk boundaries.
        let mut line_buffer = String::new();

        loop {
            match tokio::time::timeout(keepalive_interval, upstream.next()).await {
                Ok(Some(Ok(bytes))) => {
                    // Parse SSE lines to collect assistant text for persistence.
                    if let Ok(chunk) = std::str::from_utf8(&bytes) {
                        line_buffer.push_str(chunk);
                        // Process complete lines
                        while let Some(pos) = line_buffer.find('\n') {
                            let line = line_buffer[..pos].trim().to_string();
                            line_buffer = line_buffer[pos + 1..].to_string();
                            if line.starts_with("data:") {
                                let json_str = line[5..].trim();
                                if let Ok(data) =
                                    serde_json::from_str::<serde_json::Value>(json_str)
                                {
                                    match data.get("type").and_then(|t| t.as_str()) {
                                        Some("message_delta") => {
                                            if let Some(content) =
                                                data.get("content").and_then(|c| c.as_str())
                                            {
                                                assistant_text.push_str(content);
                                            }
                                        }
                                        Some("agent_end") => {
                                            final_input_tokens = data
                                                .get("total_input_tokens")
                                                .and_then(|v| v.as_i64())
                                                .unwrap_or(0);
                                            final_output_tokens = data
                                                .get("total_output_tokens")
                                                .and_then(|v| v.as_i64())
                                                .unwrap_or(0);
                                            final_model_id = data
                                                .get("model_id")
                                                .and_then(|v| v.as_str())
                                                .map(|s| s.to_string());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }

                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Ok(Some(Err(e))) => {
                    tracing::warn!("Upstream SSE stream error: {e}");
                    let error_event = format!(
                        "event: error\ndata: {{\"error\":\"network error: {}\"}}\n\n",
                        e.to_string().replace('"', "'")
                    );
                    let _ = tx.send(Ok(Bytes::from(error_event))).await;
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    if tx
                        .send(Ok(Bytes::from_static(b":keepalive\n\n")))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }

        // Stream ended — save the assistant response if we captured any text.
        if !assistant_text.is_empty() {
            let seq = crate::sessions::message_count(&db_pool, session_id)
                .await
                .unwrap_or(0);
            let assistant_msg = amos_core::types::Message {
                role: amos_core::types::Role::Assistant,
                content: vec![amos_core::types::ContentBlock::Text {
                    text: assistant_text,
                }],
                tool_use_id: None,
                timestamp: chrono::Utc::now(),
            };
            if let Err(e) =
                crate::sessions::save_messages(&db_pool, session_id, &[assistant_msg], seq).await
            {
                tracing::warn!("Failed to save assistant message: {e}");
            }
        }

        // Always record token usage — even for tool-only responses with no text.
        // This is critical for metered billing: every token must be counted.
        let _ = crate::sessions::touch_session(
            &db_pool,
            session_id,
            2,
            final_input_tokens,
            final_output_tokens,
        )
        .await;

        // Stamp the assistant message we just inserted with model_id + token
        // counts so the per-message audit trail reflects what actually ran.
        // Pre-fix: every messages row had model_id=NULL, input/output=0 — the
        // schema columns existed but nothing wrote to them.
        if final_input_tokens > 0 || final_output_tokens > 0 || final_model_id.is_some() {
            if let Err(e) = crate::sessions::stamp_last_assistant_message_usage(
                &db_pool,
                session_id,
                final_model_id.as_deref(),
                final_input_tokens,
                final_output_tokens,
            )
            .await
            {
                tracing::warn!("Failed to stamp assistant message usage: {e}");
            }
        }

        if final_input_tokens > 0 || final_output_tokens > 0 {
            use std::sync::atomic::Ordering::Relaxed;
            activity_counters
                .tokens_input
                .fetch_add(final_input_tokens as u64, Relaxed);
            activity_counters
                .tokens_output
                .fetch_add(final_output_tokens as u64, Relaxed);
            activity_counters.messages.fetch_add(2, Relaxed);
            if let Some(model) = &final_model_id {
                activity_counters
                    .record_model_usage(
                        model,
                        final_input_tokens as u64,
                        final_output_tokens as u64,
                    )
                    .await;
            }
        }
    });

    tokio_stream::wrappers::ReceiverStream::new(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_base_url_defaults_to_localhost() {
        std::env::remove_var("AGENT_URL");
        assert_eq!(agent_base_url(), "http://localhost:3100");
    }

    #[test]
    fn agent_base_url_reads_env() {
        std::env::set_var("AGENT_URL", "http://agent-sidecar:3100");
        assert_eq!(agent_base_url(), "http://agent-sidecar:3100");
        std::env::remove_var("AGENT_URL");
    }
}
