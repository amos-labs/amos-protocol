//! AMOS Agent binary - standalone autonomous agent.
//!
//! Connects to the AMOS Harness using the same protocol as any external agent.
//! Provides local tools (think, remember, plan, web_search, file I/O) and
//! accesses harness tools via HTTP.
//!
//! ## Modes
//!
//! - **Interactive** (default): reads from stdin, prints to stderr.
//! - **Service** (`--serve`): starts an HTTP API (SSE chat + health) and a
//!   background task consumer that polls the harness for work.

use amos_agent::{
    agent_card::{agent_card_router, AgentCard},
    agent_loop::{self, LoopConfig},
    config::{AgentConfig, Cli},
    harness_client::HarnessClient,
    memory::MemoryStore,
    provider,
    routes::{self, AgentState},
    task_consumer::{self, TaskConsumerConfig},
    tools::ToolContext,
};
use clap::Parser;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI args
    let cli = Cli::parse();
    let config = AgentConfig::from(cli);

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    info!(
        name = %config.agent_name,
        harness = %config.harness_url,
        port = config.agent_port,
        serve = config.serve,
        "Starting AMOS Agent"
    );

    // Initialize memory store
    let memory_store = MemoryStore::open(&config.memory_db)
        .map_err(|e| anyhow::anyhow!("Failed to open memory database: {e}"))?;
    let mem_count = memory_store.count().unwrap_or(0);
    info!(path = %config.memory_db, memories = mem_count, "Memory store initialized");

    let memory = Arc::new(tokio::sync::Mutex::new(memory_store));

    // Initialize harness client (before tool context, since ToolContext needs a reference)
    let mut harness = HarnessClient::new(&config.harness_url, config.agent_token.clone());

    // Register with the harness (retry to handle sidecar startup race)
    let card_url = format!(
        "http://localhost:{}/.well-known/agent.json",
        config.agent_port
    );
    let max_retries = 10;
    for attempt in 1..=max_retries {
        match harness.register(&config.agent_name, Some(&card_url)).await {
            Ok(()) => {
                info!(
                    tools = harness.harness_tools.len(),
                    "Connected to harness, {} tools available",
                    harness.harness_tools.len()
                );
                break;
            }
            Err(e) => {
                if attempt == max_retries {
                    warn!("Could not connect to harness after {max_retries} attempts: {e}. Running in standalone mode.");
                } else {
                    debug!("Harness not ready (attempt {attempt}/{max_retries}): {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
    }

    // Create the default model provider.
    // In serve/BYOK mode, a missing API key is non-fatal: per-request BYOK
    // providers will be created for each chat request. The default provider
    // is only used as a fallback if no BYOK config is supplied.
    let model_provider: Arc<dyn provider::ModelProvider> = match provider::create_provider(
        &config.model_provider,
        &config.model_id,
        config.api_base.as_deref(),
        config.api_key.as_deref(),
    ) {
        Ok(p) => Arc::from(p),
        Err(e) if config.serve => {
            warn!("Default provider not available ({e}). BYOK per-request providers will be used.");
            Arc::new(provider::NoOpProvider)
        }
        Err(e) => return Err(e.into()),
    };

    let loop_config = LoopConfig {
        max_iterations: config.max_iterations,
        model_id: config.model_id.clone(),
        ..Default::default()
    };

    // Wrap harness in RwLock for shared access
    let harness = Arc::new(RwLock::new(harness));

    // Create tool context (after harness is wrapped so we can share it)
    let tool_ctx = Arc::new(ToolContext {
        memory: memory.clone(),
        brave_api_key: config.brave_api_key.clone(),
        work_dir: config.work_dir.clone(),
        harness: Some(harness.clone()),
    });

    // Seed local memory from harness if the local store is empty (fresh container).
    // This ensures agent memories survive container restarts.
    if mem_count == 0 {
        info!("Local memory empty, seeding from harness...");
        let h = harness.read().await;
        match h
            .execute_tool(
                "search_memory",
                serde_json::json!({"query": "", "limit": 50, "category": "agent_memory"}),
                None,
            )
            .await
        {
            Ok(resp) if !resp.is_error => {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&resp.content) {
                    if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                        let mem = memory.lock().await;
                        let mut seeded = 0;
                        for r in results {
                            let content = r
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or_default();
                            let id = r.get("id").and_then(|i| i.as_str()).unwrap_or("unknown");
                            if !content.is_empty() && mem.remember(id, content, &[]).is_ok() {
                                seeded += 1;
                            }
                        }
                        if seeded > 0 {
                            info!(count = seeded, "Seeded local memory from harness");
                        }
                    }
                }
            }
            Ok(_) => info!("No memories to seed from harness"),
            Err(e) => debug!("Could not seed memories from harness: {e}"),
        }
        drop(h);
    }

    // Start the Agent Card server in the background
    let agent_card = AgentCard {
        url: format!("http://localhost:{}", config.agent_port),
        ..AgentCard::default()
    };
    let card_router = agent_card_router(agent_card);

    if config.serve {
        // ─── Service mode ───────────────────────────────────────────────
        // Merge the agent card routes with the API routes on a single port.
        info!(
            port = config.agent_port,
            "Service mode: starting HTTP API + task consumer"
        );

        let agent_state = AgentState {
            provider: model_provider.clone(),
            tool_ctx: tool_ctx.clone(),
            harness: harness.clone(),
            loop_config: loop_config.clone(),
        };

        let app = routes::agent_router(agent_state).merge(card_router);

        // Start heartbeat loop
        let heartbeat_harness = harness.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let h = heartbeat_harness.read().await;
                if let Err(e) = h.heartbeat().await {
                    debug!("Heartbeat failed: {e}");
                }
            }
        });

        // Start task consumer
        let tc_config = TaskConsumerConfig {
            max_iterations: config.max_iterations,
            ..Default::default()
        };
        tokio::spawn(task_consumer::run_task_consumer(
            tc_config,
            harness.clone(),
            model_provider.clone(),
            tool_ctx.clone(),
            loop_config,
        ));

        // Bind and serve
        let listener = TcpListener::bind(format!("0.0.0.0:{}", config.agent_port)).await?;
        info!(port = config.agent_port, "Agent HTTP server listening");
        axum::serve(listener, app).await?;
    } else {
        // ─── Interactive mode ───────────────────────────────────────────
        let card_port = config.agent_port;
        tokio::spawn(async move {
            let listener = TcpListener::bind(format!("0.0.0.0:{}", card_port))
                .await
                .expect("Failed to bind Agent Card server");
            info!(port = card_port, "Agent Card server listening");
            axum::serve(listener, card_router).await.ok();
        });

        // Heartbeat
        let heartbeat_harness = harness.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let h = heartbeat_harness.read().await;
                if let Err(e) = h.heartbeat().await {
                    debug!("Heartbeat failed: {e}");
                }
            }
        });

        info!("AMOS Agent ready. Type a message to begin (Ctrl+C to quit).");

        let stdin = tokio::io::stdin();
        let reader = tokio::io::BufReader::new(stdin);

        use tokio::io::AsyncBufReadExt;
        let mut lines = reader.lines();

        loop {
            eprint!("\n> ");
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    if line == "/quit" || line == "/exit" {
                        info!("Goodbye!");
                        break;
                    }

                    // Set up event channel for streaming output
                    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

                    // Print streaming output
                    let print_handle = tokio::spawn(async move {
                        while let Some(event) = event_rx.recv().await {
                            match event {
                                agent_loop::AgentEvent::TextDelta { content } => {
                                    eprint!("{}", content);
                                }
                                agent_loop::AgentEvent::ToolStart {
                                    tool_name,
                                    is_local,
                                    input_summary,
                                } => {
                                    let loc = if is_local { "local" } else { "harness" };
                                    if let Some(summary) = input_summary {
                                        eprintln!("\n[{loc}] {summary}...");
                                    } else {
                                        eprintln!("\n[{loc}] {tool_name}...");
                                    }
                                }
                                agent_loop::AgentEvent::ToolEnd {
                                    tool_name,
                                    duration_ms,
                                    is_error: true,
                                    result_summary,
                                    metadata: _,
                                } => {
                                    let msg = result_summary
                                        .unwrap_or_else(|| format!("{tool_name} failed"));
                                    eprintln!("[error] {msg} ({duration_ms}ms)");
                                }
                                agent_loop::AgentEvent::Error { message } => {
                                    eprintln!("\n[ERROR] {message}");
                                }
                                _ => {}
                            }
                        }
                    });

                    // Run the agent loop
                    let h = harness.read().await;
                    match agent_loop::run_agent_loop(
                        &loop_config,
                        model_provider.as_ref(),
                        &tool_ctx,
                        Some(&h),
                        &line,
                        None,
                        None, // no history in interactive mode
                        None, // no workspace context in interactive mode
                        Some(event_tx),
                    )
                    .await
                    {
                        Ok(_final_text) => {
                            eprintln!(); // newline after streaming
                        }
                        Err(e) => {
                            eprintln!("\nAgent error: {e}");
                        }
                    }
                    drop(h);

                    let _ = print_handle.await;
                }
                Ok(None) => break, // EOF
                Err(e) => {
                    error!("Input error: {e}");
                    break;
                }
            }
        }
    }

    Ok(())
}
