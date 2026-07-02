//! # AMOS Harness
//!
//! The per-customer business operating system and agent marketplace.
//!
//! AMOS Harness is an AI-native business operating system deployed per-customer.
//! It provides:
//! - Conversational + canvas interface (the customer's ONLY UI)
//! - Platform for building workflows, automations, integrations, and apps
//! - Control plane for OpenClaw agents (autonomous AI employees)
//! - Task queue with external agent bounties
//!

#![allow(dead_code)]
#![allow(deprecated)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::approx_constant)]
#![allow(clippy::format_in_format_args)]
#![allow(clippy::single_match_else)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_strip)]
#![allow(clippy::wildcard_in_or_patterns)]
#![allow(clippy::manual_map)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::unnecessary_get_then_check)]
#![allow(clippy::new_without_default)]
#![allow(clippy::unnecessary_filter_map)]
#![allow(clippy::collapsible_match)]
//! ## Architecture
//!
//! The harness consists of several key components:
//!
//! - **Agent**: V3 event-driven agent loop with model escalation and streaming
//! - **Canvas**: Dynamic UI generation and rendering engine
//! - **Tools**: Extensible tool system for platform, canvas, web, and system operations
//! - **OpenClaw**: Autonomous AI agent management and orchestration
//! - **Task Queue**: Unified task system with internal sub-agents and external bounties
//! - **Integrations**: Connector framework for third-party services
//! - **Memory**: Working memory with salience-based attention

pub mod agent;
pub mod automations;
pub mod bedrock;
pub mod canvas;
pub mod documents;
pub mod embeddings;
pub mod geo;
pub mod html_sanitizer;
pub mod image_gen;
pub mod integrations;
pub mod memory;
pub mod middleware;
pub mod oauth_refresh;
pub mod openclaw;
pub mod orchestrator;
pub mod packages;
pub mod platform_sync;
pub mod prompt_guard;
pub mod relay_registration;
pub mod relay_sync;
pub mod revisions;
pub mod routes;
pub mod schema;
pub mod server;
pub mod ses;
pub mod sessions;
pub mod shutdown;
pub mod sites;
pub mod state;
pub mod storage;
pub mod task_queue;
pub mod templates;
pub mod tools;

// Re-export commonly used types
pub use server::create_server;
pub use state::AppState;

// Re-export canvas types
pub use canvas::{
    types::{Canvas, CanvasResponse, CanvasType},
    CanvasEngine,
};

// Re-export tool types (Tool, ToolResult, ToolCategory from amos-core; ToolRegistry is local)
pub use amos_core::{Tool, ToolCategory, ToolResult};
pub use tools::ToolRegistry;

// Re-export package types (from amos-core)
pub use amos_core::{AmosPackage, PackageContext, PackageToolRegistry};

// Re-export OpenClaw types
pub use openclaw::fleet::{AgentProfile, FleetManager};
pub use openclaw::{AgentConfig, AgentManager, AgentStatus};

// Re-export platform sync types
pub use platform_sync::{ActivityCounters, PlatformSyncClient};

// Re-export relay sync types
pub use relay_sync::RelaySyncClient;

// Re-export task queue types
pub use task_queue::{Task, TaskCategory, TaskQueue, TaskStatus};

// Re-export document processing types
pub use documents::{DocumentExporter, DocumentProcessor, ExportFormat, ExtractionResult};

// Re-export image generation types
pub use image_gen::ImageGenClient;

use amos_core::Result;

/// Version of the AMOS Harness
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the harness with the given configuration
pub async fn init() -> Result<()> {
    // Any global initialization can go here
    Ok(())
}
