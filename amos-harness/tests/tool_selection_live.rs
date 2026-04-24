//! Live tool-selection regression test — Phase 2.5 of the test harness plan.
//!
//! Calls Haiku via real Bedrock with the harness's real tool schemas and
//! asserts the model picks the expected tool for a canonical prompt. Catches
//! the regression class from the plan's signal list: "empty site containers —
//! tool description confused the agent." Static schema tests don't see this;
//! the golden bounty harness (Phase 4) is months out.
//!
//! `#[ignore]` so `cargo test` skips this by default — real Bedrock calls
//! cost money and need AWS creds. CI runs it main-only via `--ignored`,
//! reusing the `claude_github` OIDC role the live-model-probe already uses.
//!
//! Tools are instantiated with a lazy PgPool — schema/description methods
//! don't touch the DB, so no Postgres service needed. That keeps this test
//! cheap and independent of `integration-smoke`.

use amos_core::types::{ContentBlock, Message, Role};
use amos_harness::bedrock::BedrockClient;
use amos_harness::tools::site_tools::{CreateLandingPageTool, CreateSiteTool, ManagePageTool};
use amos_harness::tools::Tool;
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;

/// A pool that never actually connects. Safe because we only call the Tool
/// trait's metadata methods (`name`, `description`, `parameters_schema`) —
/// none of which touch the DB.
fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://localhost/fake").expect("lazy pool")
}

/// Tool spec in the shape the harness `BedrockClient` expects — the client
/// wraps this in `{toolSpec: ...}` itself (see `build_converse_request`).
/// We do the `{json: <schema>}` inner envelope here since that's part of
/// the spec proper.
fn tool_to_bedrock(tool: &dyn Tool) -> serde_json::Value {
    json!({
        "name": tool.name(),
        "description": tool.description(),
        "inputSchema": { "json": tool.parameters_schema() }
    })
}

fn user_msg(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::Text { text: text.into() }],
        tool_use_id: None,
        timestamp: Utc::now(),
    }
}

fn first_tool_use(msg: &Message) -> Option<&str> {
    msg.content.iter().find_map(|c| match c {
        ContentBlock::ToolUse { name, .. } => Some(name.as_str()),
        _ => None,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Landing-page prompt must select `create_landing_page`, not the
// multi-page alternatives. This is the regression that broke for Jana
// on 2026-04-19 (empty site container).
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "live Bedrock call — run via CI or `cargo test -- --ignored`"]
async fn haiku_picks_create_landing_page_for_landing_prompt() {
    let client = BedrockClient::new(None, None, None).expect("bedrock client from env");

    let landing = CreateLandingPageTool::new(lazy_pool());
    let site = CreateSiteTool::new(lazy_pool());
    let page = ManagePageTool::new(lazy_pool());

    let tools = vec![
        tool_to_bedrock(&landing),
        tool_to_bedrock(&site),
        tool_to_bedrock(&page),
    ];

    let messages = vec![user_msg(
        "Create a landing page for a product called 'Buyers Not Bots' at \
         slug 'buyers-not-bots'. It needs a hero section pitching the value of \
         verified-human-only communities and a single call-to-action button.",
    )];

    let (response, _usage) = client
        .converse(
            "us.anthropic.claude-haiku-4-5-20251001-v1:0",
            "You are an AI assistant that helps users build websites. \
             Use the available tools to complete their request in as few calls as possible.",
            &messages,
            &tools,
        )
        .await
        .expect("bedrock converse");

    let picked = first_tool_use(&response).unwrap_or_else(|| {
        panic!(
            "expected a tool_use in the response; got text-only. \
             Content blocks: {:?}",
            response.content
        )
    });

    assert_eq!(
        picked, "create_landing_page",
        "Haiku picked `{picked}` for a single-page landing-page prompt — \
         tool-selection regression. Check description of `create_landing_page` \
         vs. distractors for drift."
    );
}
