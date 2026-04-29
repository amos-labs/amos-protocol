//! End-to-end smoke tests for the core customer-facing flows.
//!
//! These tests run against a real Postgres with the full harness migration set
//! applied — they're the L2 layer of the regression-catching pyramid, sitting
//! above the static catalog/schema invariants and the Bedrock live probe.
//!
//! Each test picks unique slugs so they can run in parallel against a shared
//! DB without stepping on each other. `DATABASE_URL` must be set; CI points
//! it at the `services: postgres` container.

use amos_harness::relay_sync::RelayBounty;
use amos_harness::schema::{FieldDefinition, FieldType, SchemaEngine};
use amos_harness::sites::SiteEngine;
use amos_harness::tools::bounty_agent_tools::{BountyWorkspaceTool, VerifyBountyTool};
use amos_harness::tools::site_tools::{
    CreateLandingPageTool, CreateSiteTool, ManagePageTool, PublishSiteTool,
};
use amos_harness::tools::Tool;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

// ═════════════════════════════════════════════════════════════════════════
// Per-test pool. PgPool is bound to the tokio runtime that created it,
// and #[tokio::test] spawns a fresh runtime per test, so a shared static
// pool goes stale. Each test connects from scratch. Migrations are
// idempotent — sqlx tracks applied versions in `_sqlx_migrations`.
// ═════════════════════════════════════════════════════════════════════════

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set — point at a Postgres with pgvector");
    let p = PgPool::connect(&url)
        .await
        .expect("connect to test Postgres");
    sqlx::migrate!("./migrations")
        .run(&p)
        .await
        .expect("harness migrations apply cleanly");
    p
}

fn unique(prefix: &str) -> String {
    // Slugs must match [a-z0-9-], so strip hyphens from the UUID.
    format!("{}-{}", prefix, Uuid::new_v4().simple())
}

// ═════════════════════════════════════════════════════════════════════════
// Landing page — the exact path that broke for Jana on 2026-04-19.
// ═════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_landing_page_is_immediately_live() {
    let pool = pool().await;
    let tool = CreateLandingPageTool::new(pool.clone());
    let slug = unique("smoke-landing");

    let result = tool
        .execute(json!({
            "name": "Smoke Test Landing",
            "slug": slug,
            "html_content": "<h1 id='hero'>Hello from smoke test</h1><p>Body copy.</p>",
            "description": "Smoke test landing page"
        }))
        .await
        .expect("tool returns Ok");
    assert!(
        result.success,
        "create_landing_page should succeed: {:?}",
        result.error
    );

    // One call → site exists, page exists at '/', site is published.
    let engine = SiteEngine::new(pool);
    let (site, page) = engine
        .get_page(&slug, "/")
        .await
        .expect("page should exist at / after create_landing_page");
    assert!(
        site.is_published,
        "site must be auto-published — otherwise /s/{{slug}} returns 404"
    );
    assert_eq!(page.path, "/");
    assert!(
        page.html_content.contains("Hello from smoke test"),
        "rendered html should contain user-supplied content, got: {}",
        page.html_content
    );
}

// ═════════════════════════════════════════════════════════════════════════
// Multi-page site — create_site + manage_page + publish.
// ═════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_site_then_manage_pages_renders_both() {
    let pool = pool().await;
    let site_tool = CreateSiteTool::new(pool.clone());
    let page_tool = ManagePageTool::new(pool.clone());
    let publish_tool = PublishSiteTool::new(pool.clone());
    let slug = unique("smoke-multi");

    site_tool
        .execute(json!({ "name": "Multi Site", "slug": slug }))
        .await
        .expect("create_site ok");

    page_tool
        .execute(json!({
            "site_slug": slug,
            "path": "/",
            "title": "Home",
            "html_content": "<h1>Home</h1>"
        }))
        .await
        .expect("home page ok");

    page_tool
        .execute(json!({
            "site_slug": slug,
            "path": "/about",
            "title": "About",
            "html_content": "<h1>About</h1>"
        }))
        .await
        .expect("about page ok");

    publish_tool
        .execute(json!({ "site_slug": slug }))
        .await
        .expect("publish ok");

    let engine = SiteEngine::new(pool);
    let pages = engine.list_pages(&slug).await.expect("list pages");
    assert_eq!(pages.len(), 2, "expected 2 pages, got {}", pages.len());

    let paths: Vec<&str> = pages.iter().map(|p| p.path.as_str()).collect();
    assert!(paths.contains(&"/"));
    assert!(paths.contains(&"/about"));
}

// ═════════════════════════════════════════════════════════════════════════
// Collection + record CRUD — the backbone of every runtime-defined schema.
// ═════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn collection_record_round_trip() {
    let pool = pool().await;
    let engine = SchemaEngine::new(pool);
    // Collection names must be unique globally; migrations seed some standard
    // ones, so scope ours with a UUID-like suffix.
    let name = format!("smoke_leads_{}", Uuid::new_v4().simple());

    let fields = vec![
        FieldDefinition {
            name: "email".to_string(),
            display_name: "Email".to_string(),
            field_type: FieldType::Email,
            required: true,
            unique: false,
            default_value: None,
            description: None,
            options: json!({}),
        },
        FieldDefinition {
            name: "source".to_string(),
            display_name: "Source".to_string(),
            field_type: FieldType::Text,
            required: false,
            unique: false,
            default_value: Some(json!("organic")),
            description: None,
            options: json!({}),
        },
    ];

    let collection = engine
        .define_collection(&name, "Smoke Leads", Some("smoke test"), fields)
        .await
        .expect("define_collection ok");
    assert_eq!(collection.name, name);

    let record = engine
        .create_record(&name, json!({ "email": "jana@example.com" }))
        .await
        .expect("create_record ok");
    // Default should have applied.
    assert_eq!(record.data["source"], json!("organic"));

    let fetched = engine.get_record(record.id).await.expect("get_record ok");
    assert_eq!(fetched.data["email"], json!("jana@example.com"));
}

// ═════════════════════════════════════════════════════════════════════════
// Schema validation rejects bad data — blocks regressions in validate path.
// ═════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn collection_rejects_invalid_record_type() {
    let pool = pool().await;
    let engine = SchemaEngine::new(pool);
    let name = format!("smoke_numbers_{}", Uuid::new_v4().simple());

    engine
        .define_collection(
            &name,
            "Smoke Numbers",
            None,
            vec![FieldDefinition {
                name: "count".to_string(),
                display_name: "Count".to_string(),
                field_type: FieldType::Number,
                required: true,
                unique: false,
                default_value: None,
                description: None,
                options: json!({}),
            }],
        )
        .await
        .expect("define_collection ok");

    // String in a number field must be rejected.
    let err = engine
        .create_record(&name, json!({ "count": "not-a-number" }))
        .await;
    assert!(
        err.is_err(),
        "schema validation should reject a string in a number field"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// Bounty workspace — claimed bounty gets an isolated scratch directory.
// ═════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bounty_workspace_allocates_dir_for_claimed_bounty() {
    let pool = pool().await;
    let bounty_id = format!("smoke-bounty-{}", Uuid::new_v4().simple());

    // Point the workspace base at a per-test temp dir so we don't fight
    // other parallel tests over /tmp/amos-bounties.
    let temp_root =
        std::env::temp_dir().join(format!("amos-bounty-test-{}", Uuid::new_v4().simple()));
    std::env::set_var("AMOS__BOUNTY_WORKSPACE_BASE", &temp_root);

    // Seed an agent + an active claim. The workspace tool gates on this row
    // so the call would fail without it.
    let agent_id: i32 = sqlx::query_scalar(
        "INSERT INTO openclaw_agents (name, display_name, role, capabilities) \
         VALUES ($1, $2, 'worker', '[]'::jsonb) RETURNING id",
    )
    .bind(format!("smoke-agent-{}", Uuid::new_v4().simple()))
    .bind("Smoke Agent")
    .fetch_one(&pool)
    .await
    .expect("seed agent");
    sqlx::query(
        "INSERT INTO bounty_claims (agent_id, bounty_id, status, fit_score, reward_tokens) \
         VALUES ($1, $2, 'claimed', 0.9, 100)",
    )
    .bind(agent_id)
    .bind(&bounty_id)
    .execute(&pool)
    .await
    .expect("seed claim");

    let tool = BountyWorkspaceTool::new(pool.clone());
    let result = tool
        .execute(json!({ "bounty_id": bounty_id }))
        .await
        .expect("tool returns Ok");
    assert!(
        result.success,
        "expected success, got error: {:?}",
        result.error
    );

    let data = result.data.expect("data present");
    let workspace = data["workspace"].as_str().expect("workspace path");
    let repo = data["subdirs"]["repo"].as_str().expect("repo subdir");
    let output = data["subdirs"]["output"].as_str().expect("output subdir");
    let logs = data["subdirs"]["logs"].as_str().expect("logs subdir");

    for path in [workspace, repo, output, logs] {
        assert!(
            std::path::Path::new(path).is_dir(),
            "directory should exist: {path}"
        );
    }

    // Cleanup.
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn bounty_workspace_rejects_unclaimed_bounty() {
    let pool = pool().await;
    let tool = BountyWorkspaceTool::new(pool);

    // Bounty ID that we never inserted into bounty_claims.
    let result = tool
        .execute(json!({ "bounty_id": format!("never-claimed-{}", Uuid::new_v4().simple()) }))
        .await
        .expect("tool returns Ok");
    assert!(!result.success, "should reject when no active claim exists");
    let err = result.error.unwrap_or_default();
    assert!(
        err.contains("No active claim"),
        "error should explain the missing-claim case, got: {err}"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// Verify bounty — runs the bounty's test_command in the workspace.
// ═════════════════════════════════════════════════════════════════════════

async fn seed_claim(pool: &PgPool, bounty_id: &str) -> i32 {
    let agent_id: i32 = sqlx::query_scalar(
        "INSERT INTO openclaw_agents (name, display_name, role, capabilities) \
         VALUES ($1, $2, 'worker', '[]'::jsonb) RETURNING id",
    )
    .bind(format!("verify-agent-{}", Uuid::new_v4().simple()))
    .bind("Verify Agent")
    .fetch_one(pool)
    .await
    .expect("seed agent");
    sqlx::query(
        "INSERT INTO bounty_claims (agent_id, bounty_id, status, fit_score, reward_tokens) \
         VALUES ($1, $2, 'claimed', 0.9, 100)",
    )
    .bind(agent_id)
    .bind(bounty_id)
    .execute(pool)
    .await
    .expect("seed claim");
    agent_id
}

fn fixture_bounty(bounty_id: &str, test_command: Option<&str>) -> RelayBounty {
    let parsed_id = Uuid::parse_str(bounty_id).unwrap_or_else(|_| Uuid::new_v4());
    RelayBounty {
        id: parsed_id,
        title: "verify-test".into(),
        description: "smoke fixture".into(),
        reward_tokens: 100,
        deadline: "2099-01-01".into(),
        required_capabilities: vec![],
        category: "infrastructure".into(),
        status: None,
        pr_url: None,
        poster_wallet: None,
        revision_count: 0,
        policy: None,
        min_trust_level: None,
        tier: None,
        acceptance_criteria: None,
        repo_url: None,
        test_command: test_command.map(String::from),
    }
}

#[tokio::test]
async fn verify_bounty_passes_when_test_command_succeeds() {
    let pool = pool().await;
    let bounty_id = Uuid::new_v4().to_string();

    let temp_root =
        std::env::temp_dir().join(format!("amos-verify-pass-{}", Uuid::new_v4().simple()));
    std::env::set_var("AMOS__BOUNTY_WORKSPACE_BASE", &temp_root);

    seed_claim(&pool, &bounty_id).await;

    // Allocate workspace + populate repo dir (mimics the agent doing work).
    let workspace_tool = BountyWorkspaceTool::new(pool.clone());
    workspace_tool
        .execute(json!({ "bounty_id": bounty_id }))
        .await
        .expect("workspace ok");

    // Cache a bounty whose test_command is trivially-passing.
    let cache = std::sync::Arc::new(tokio::sync::RwLock::new(vec![fixture_bounty(
        &bounty_id,
        Some("true"),
    )]));

    let verify = VerifyBountyTool::new(pool.clone(), cache, "http://localhost:99999".into());
    let result = verify
        .execute(json!({ "bounty_id": bounty_id }))
        .await
        .expect("verify ok");
    assert!(result.success, "verify_bounty: {:?}", result.error);
    let data = result.data.expect("data");
    assert_eq!(data["status"], json!("passed"));
    assert_eq!(data["exit_code"], json!(0));

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn verify_bounty_fails_when_test_command_exits_nonzero() {
    let pool = pool().await;
    let bounty_id = Uuid::new_v4().to_string();

    let temp_root =
        std::env::temp_dir().join(format!("amos-verify-fail-{}", Uuid::new_v4().simple()));
    std::env::set_var("AMOS__BOUNTY_WORKSPACE_BASE", &temp_root);

    seed_claim(&pool, &bounty_id).await;
    BountyWorkspaceTool::new(pool.clone())
        .execute(json!({ "bounty_id": bounty_id }))
        .await
        .expect("workspace ok");

    let cache = std::sync::Arc::new(tokio::sync::RwLock::new(vec![fixture_bounty(
        &bounty_id,
        Some("exit 7"),
    )]));

    let verify = VerifyBountyTool::new(pool.clone(), cache, "http://localhost:99999".into());
    let result = verify
        .execute(json!({ "bounty_id": bounty_id }))
        .await
        .expect("verify ok");
    assert!(result.success);
    let data = result.data.expect("data");
    assert_eq!(data["status"], json!("failed"));
    assert_eq!(data["exit_code"], json!(7));

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn verify_bounty_skips_when_no_test_command_set() {
    let pool = pool().await;
    let bounty_id = Uuid::new_v4().to_string();

    seed_claim(&pool, &bounty_id).await;

    let cache = std::sync::Arc::new(tokio::sync::RwLock::new(vec![fixture_bounty(
        &bounty_id, None,
    )]));

    let verify = VerifyBountyTool::new(pool.clone(), cache, "http://localhost:99999".into());
    let result = verify
        .execute(json!({ "bounty_id": bounty_id }))
        .await
        .expect("verify ok");
    assert!(result.success);
    assert_eq!(result.data.unwrap()["status"], json!("skipped"));
}
