//! End-to-end smoke tests for the core customer-facing flows.
//!
//! These tests run against a real Postgres with the full harness migration set
//! applied — they're the L2 layer of the regression-catching pyramid, sitting
//! above the static catalog/schema invariants and the Bedrock live probe.
//!
//! Each test picks unique slugs so they can run in parallel against a shared
//! DB without stepping on each other. `DATABASE_URL` must be set; CI points
//! it at the `services: postgres` container.

use amos_harness::schema::{FieldDefinition, FieldType, SchemaEngine};
use amos_harness::sites::SiteEngine;
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
