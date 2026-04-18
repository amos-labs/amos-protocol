//! HTTP routes and WebSocket handlers

pub mod agent_proxy;
pub mod automations;
pub mod bots;
pub mod bounties;
pub mod canvas;
pub mod confirm;
pub mod credentials;
pub mod data;
pub mod fleet;
pub mod harness_info;
pub mod health;
pub mod hooks;
pub mod integrations;
pub mod llm_providers;
pub mod oauth;
pub mod packages;
pub mod revisions;
pub mod settings;
pub mod sites;
pub mod uploads;
pub mod wallet;

use crate::middleware::{self, RateLimiter};
use crate::state::AppState;
use axum::{extract::DefaultBodyLimit, extract::State, routing::get, Json, Router};
use std::sync::Arc;

/// Build all application routes
pub fn build_routes(state: Arc<AppState>) -> Router {
    // ── Rate limiters ──────────────────────────────────────────────
    // Chat endpoint: tight limit (LLM calls are expensive)
    let chat_limiter = RateLimiter::new(20, 2.0); // 20 burst, 2 req/s sustained
                                                  // General API: moderate limit
    let api_limiter = RateLimiter::new(100, 20.0); // 100 burst, 20 req/s sustained
                                                   // Public endpoints: generous but bounded
    let public_limiter = RateLimiter::new(200, 40.0); // 200 burst, 40 req/s sustained

    // ── Public routes (no auth required) ────────────────────────────
    let public_routes = Router::new()
        // Health check (no rate limit — used by load balancers)
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        // EAP discovery endpoints
        .route("/.well-known/agent.json", get(well_known_agent_json))
        .route("/api/v1/tools", get(list_tools))
        // Auth page routes (login/register pages must be accessible)
        .route("/login", get(canvas::serve_login))
        .route("/register", get(canvas::serve_register))
        .route("/forgot-password", get(canvas::serve_forgot_password))
        // Token exchange: platform redirects here with ?token=<jwt>
        .route("/auth/callback", get(middleware::token_exchange))
        // OpenClaw agent management (agent sidecar uses these internally)
        .nest("/api/v1/agents", bots::routes(state.clone()))
        // Public canvas route
        .route("/c/{slug}", get(canvas::serve_public_canvas))
        // Public site serving
        .route("/s/{slug}", axum::routing::get(sites::serve_site_index))
        .route(
            "/s/{slug}/{*path}",
            axum::routing::get(sites::serve_site_page),
        )
        .route(
            "/s/{slug}/submit/{collection}",
            axum::routing::post(sites::handle_form_submit),
        )
        // Solana config (public, no auth needed)
        .nest("/api/v1/config", wallet::public_routes(state.clone()))
        // Webhook ingress routes (external triggers, auth via webhook secret)
        .nest("/api/v1/hooks", hooks::routes(state.clone()))
        // OAuth2 authorization code flow: external providers redirect here
        // after the user consents, so these endpoints must be public.
        // Security is via state_token (per-flow) + short-TTL + PKCE.
        .nest("/api/v1/oauth", oauth::routes(state.clone()))
        .layer({
            let limiter = public_limiter;
            axum::middleware::from_fn(move |req, next| {
                middleware::rate_limit_middleware(req, next, limiter.clone())
            })
        })
        .with_state(state.clone());

    // ── Protected routes (require auth) ─────────────────────────────
    let protected_routes = Router::new()
        // Canvas routes
        .nest("/api/v1/canvases", canvas::routes(state.clone()))
        // Agent proxy routes (forward chat to agent sidecar service)
        // Chat gets its own stricter rate limit
        .nest(
            "/api/v1/agent",
            agent_proxy::routes(state.clone()).layer({
                let limiter = chat_limiter;
                axum::middleware::from_fn(move |req, next| {
                    middleware::rate_limit_middleware(req, next, limiter.clone())
                })
            }),
        )
        // Upload routes (25 MB body limit for file uploads)
        .nest(
            "/api/v1/uploads",
            uploads::routes(state.clone()).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        // Integration routes
        .nest("/api/v1/integrations", integrations::routes(state.clone()))
        // Credential vault routes (Secure Input Canvas target)
        .nest("/api/v1/credentials", credentials::routes(state.clone()))
        // LLM Provider routes (BYOK - Bring Your Own Key)
        .nest(
            "/api/v1/llm-providers",
            llm_providers::routes(state.clone()),
        )
        // Revision and template routes
        .nest("/api/v1", revisions::routes(state.clone()))
        // Data API routes (collection/record CRUD for canvas components)
        .nest("/api/v1/data", data::routes(state.clone()))
        // Harness info route (multi-harness discovery)
        .nest("/api/v1/harness", harness_info::routes(state.clone()))
        // Bounty proxy routes (forwards to AMOS Network Relay)
        .nest("/api/v1/bounties", bounties::routes(state.clone()))
        // Fleet management routes (autonomous bounty agents)
        .nest("/api/v1/fleet", fleet::routes(state.clone()))
        // Wallet connection routes (link Solana wallets)
        .nest("/api/v1/wallet", wallet::routes(state.clone()))
        // Harness settings routes (model selection, provider mode)
        .nest("/api/v1/settings", settings::routes(state.clone()))
        // Package management routes
        .nest("/api/v1/packages", packages::routes(state.clone()))
        // Site management routes
        .nest("/api/v1/sites", sites::routes(state.clone()))
        // Tool confirmation routes (destructive command approve/deny)
        .nest("/api/v1/tools", confirm::routes(state.clone()))
        // Automation monitoring routes (failed runs, dead-letter queue)
        .nest("/api/v1/automations", automations::routes(state.clone()))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::authenticate,
        ))
        .layer({
            let limiter = api_limiter;
            axum::middleware::from_fn(move |req, next| {
                middleware::rate_limit_middleware(req, next, limiter.clone())
            })
        })
        .with_state(state.clone());

    // Merge: public routes first, then protected
    Router::new().merge(public_routes).merge(protected_routes)
}

/// `GET /.well-known/agent.json` — EAP Agent Card discovery endpoint.
async fn well_known_agent_json(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let harness_role = std::env::var("AMOS_HARNESS_ROLE").unwrap_or_else(|_| "primary".into());
    let tool_count = state.tool_registry.list_tools().len();

    Json(serde_json::json!({
        "name": "amos-harness",
        "description": "AMOS Harness — per-customer AI operating system with tool execution",
        "url": format!("{}://{}:{}", "https", state.config.server.host, state.config.server.port),
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "eap/1.0",
        "capabilities": {
            "streaming": true,
            "pushNotifications": false,
            "batchExecution": false
        },
        "skills": [],
        "provider": {
            "name": "AMOS Labs",
            "model": "multi-model (BYOK)"
        },
        "role": harness_role,
        "tools_available": tool_count,
        "contact": "https://amoslabs.com"
    }))
}

/// `GET /api/v1/tools` — EAP tool discovery endpoint.
///
/// Returns all available tools with their names, descriptions, categories,
/// and parameter schemas.
async fn list_tools(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tools: Vec<serde_json::Value> = state
        .tool_registry
        .list_tools()
        .iter()
        .filter_map(|name| {
            let tool = state.tool_registry.get(name)?;
            Some(serde_json::json!({
                "name": tool.name(),
                "description": tool.description(),
                "category": format!("{:?}", tool.category()),
                "parameters_schema": tool.parameters_schema(),
                "required_trust_level": trust_level_for_category(tool.category()),
            }))
        })
        .collect();

    Json(serde_json::json!({
        "tools": tools,
        "count": tools.len()
    }))
}

/// Map tool categories to minimum trust levels per the EAP spec.
pub(crate) fn trust_level_for_category(category: amos_core::tools::ToolCategory) -> u8 {
    use amos_core::tools::ToolCategory;
    match category {
        ToolCategory::System
        | ToolCategory::Web
        | ToolCategory::Memory
        | ToolCategory::Knowledge => 1,
        ToolCategory::Schema | ToolCategory::Canvas | ToolCategory::Apps => 2,
        ToolCategory::Integration | ToolCategory::Automation | ToolCategory::TaskQueue => 3,
        ToolCategory::OpenClaw
        | ToolCategory::Document
        | ToolCategory::ImageGen
        | ToolCategory::BountyAgent => 3,
        ToolCategory::Platform => 4,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amos_core::tools::ToolCategory;

    // ── Trust level mapping ────────────────────────────────────────────

    #[test]
    fn test_trust_level_system_tools_are_level_1() {
        assert_eq!(trust_level_for_category(ToolCategory::System), 1);
        assert_eq!(trust_level_for_category(ToolCategory::Web), 1);
        assert_eq!(trust_level_for_category(ToolCategory::Memory), 1);
        assert_eq!(trust_level_for_category(ToolCategory::Knowledge), 1);
    }

    #[test]
    fn test_trust_level_workspace_tools_are_level_2() {
        assert_eq!(trust_level_for_category(ToolCategory::Schema), 2);
        assert_eq!(trust_level_for_category(ToolCategory::Canvas), 2);
        assert_eq!(trust_level_for_category(ToolCategory::Apps), 2);
    }

    #[test]
    fn test_trust_level_integration_tools_are_level_3() {
        assert_eq!(trust_level_for_category(ToolCategory::Integration), 3);
        assert_eq!(trust_level_for_category(ToolCategory::Automation), 3);
        assert_eq!(trust_level_for_category(ToolCategory::TaskQueue), 3);
        assert_eq!(trust_level_for_category(ToolCategory::OpenClaw), 3);
        assert_eq!(trust_level_for_category(ToolCategory::Document), 3);
        assert_eq!(trust_level_for_category(ToolCategory::ImageGen), 3);
    }

    #[test]
    fn test_trust_level_platform_tools_are_level_4() {
        assert_eq!(trust_level_for_category(ToolCategory::Platform), 4);
    }

    #[test]
    fn test_trust_level_default_is_level_2() {
        // Other/unknown categories default to 2
        assert_eq!(trust_level_for_category(ToolCategory::Other), 2);
    }

    #[test]
    fn test_trust_levels_are_in_range() {
        let categories = [
            ToolCategory::System,
            ToolCategory::Web,
            ToolCategory::Memory,
            ToolCategory::Knowledge,
            ToolCategory::Schema,
            ToolCategory::Canvas,
            ToolCategory::Apps,
            ToolCategory::Integration,
            ToolCategory::Automation,
            ToolCategory::TaskQueue,
            ToolCategory::OpenClaw,
            ToolCategory::Document,
            ToolCategory::ImageGen,
            ToolCategory::Platform,
            ToolCategory::Other,
            ToolCategory::Education,
            ToolCategory::Autoresearch,
            ToolCategory::Orchestrator,
            ToolCategory::BountyAgent,
        ];
        for cat in categories {
            let level = trust_level_for_category(cat);
            assert!(
                (1..=5).contains(&level),
                "Trust level {} for {:?} out of range",
                level,
                cat
            );
        }
    }

    #[test]
    fn test_trust_levels_are_monotonically_ordered() {
        // ReadOnly (1) < WorkspaceWrite (2) < Integration (3) < FullAccess (4)
        assert!(
            trust_level_for_category(ToolCategory::System)
                < trust_level_for_category(ToolCategory::Schema)
        );
        assert!(
            trust_level_for_category(ToolCategory::Schema)
                < trust_level_for_category(ToolCategory::Integration)
        );
        assert!(
            trust_level_for_category(ToolCategory::Integration)
                < trust_level_for_category(ToolCategory::Platform)
        );
    }
}
