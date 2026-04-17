//! Site tools — the AI agent's interface for creating websites and landing pages.
//!
//! These tools let the agent create multi-page public websites with full HTML/CSS/JS,
//! SEO metadata, analytics, and form handling that feeds into collections.

use super::{Tool, ToolCategory, ToolResult};
use crate::sites::SiteEngine;
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;

// ── CreateSite ───────────────────────────────────────────────────────────

/// Create a new website or landing page site.
pub struct CreateSiteTool {
    db_pool: PgPool,
}

impl CreateSiteTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for CreateSiteTool {
    fn name(&self) -> &str {
        "create_site"
    }

    fn description(&self) -> &str {
        "Create a new website or landing page. This creates the site container — then use create_page to add pages to it. Sites are served at /s/{slug} as standalone public web pages."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Site name (e.g. 'Acme Marketing Site')"
                },
                "slug": {
                    "type": "string",
                    "description": "URL slug (lowercase, hyphens). The site will be at /s/{slug}. Example: 'acme-marketing'"
                },
                "description": {
                    "type": "string",
                    "description": "What this site is for"
                },
                "settings": {
                    "type": "object",
                    "description": "Site-wide settings: {\"analytics_id\": \"G-XXXX\", \"theme_color\": \"#4f46e5\"}"
                }
            },
            "required": ["name", "slug"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("name is required".to_string()))?;

        let slug = params["slug"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("slug is required".to_string()))?;

        let description = params.get("description").and_then(|v| v.as_str());
        let settings = params.get("settings").cloned();

        let engine = SiteEngine::new(self.db_pool.clone());
        let site = engine
            .create_site(name, slug, description, settings)
            .await?;

        Ok(ToolResult::success(json!({
            "site_id": site.id.to_string(),
            "slug": site.slug,
            "url": format!("/s/{}", site.slug),
            "message": format!("Site '{}' created. Add pages with create_page.", site.name)
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── CreatePage ───────────────────────────────────────────────────────────

/// Create or update a page on a site.
pub struct CreatePageTool {
    db_pool: PgPool,
}

impl CreatePageTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for CreatePageTool {
    fn name(&self) -> &str {
        "create_page"
    }

    fn description(&self) -> &str {
        "Create or update a page on a website. Provide the full HTML content for the page body, plus optional CSS and JavaScript. The page is wrapped in a complete HTML document with SEO meta tags automatically. If the page already exists at that path, it is updated. For forms, set form_collection to the collection name that receives submissions, and add data-collection attribute to your <form> tags."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "site_slug": {
                    "type": "string",
                    "description": "Site slug to add the page to"
                },
                "path": {
                    "type": "string",
                    "description": "URL path for this page. Use '/' for homepage, '/about' for about page, etc."
                },
                "title": {
                    "type": "string",
                    "description": "Page title (shown in browser tab)"
                },
                "description": {
                    "type": "string",
                    "description": "Page description"
                },
                "html_content": {
                    "type": "string",
                    "description": "HTML content for the page body. Write complete, responsive HTML. Do NOT include <html>, <head>, or <body> tags — those are added automatically."
                },
                "css_content": {
                    "type": "string",
                    "description": "CSS styles for this page"
                },
                "js_content": {
                    "type": "string",
                    "description": "JavaScript for this page"
                },
                "meta_title": {
                    "type": "string",
                    "description": "SEO title override (defaults to title)"
                },
                "meta_description": {
                    "type": "string",
                    "description": "SEO meta description"
                },
                "form_collection": {
                    "type": "string",
                    "description": "Collection slug that receives form submissions from this page. Add data-collection attribute to <form> tags."
                }
            },
            "required": ["site_slug", "path", "title", "html_content"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let site_slug = params["site_slug"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("site_slug is required".to_string()))?;

        let path = params["path"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("path is required".to_string()))?;

        let title = params["title"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("title is required".to_string()))?;

        let html_content = params["html_content"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("html_content is required".to_string())
        })?;

        let description = params.get("description").and_then(|v| v.as_str());
        let css_content = params.get("css_content").and_then(|v| v.as_str());
        let js_content = params.get("js_content").and_then(|v| v.as_str());
        let meta_title = params.get("meta_title").and_then(|v| v.as_str());
        let meta_description = params.get("meta_description").and_then(|v| v.as_str());
        let form_collection = params.get("form_collection").and_then(|v| v.as_str());

        let engine = SiteEngine::new(self.db_pool.clone());
        let page = engine
            .upsert_page(
                site_slug,
                path,
                title,
                description,
                html_content,
                css_content,
                js_content,
                meta_title,
                meta_description,
                form_collection,
            )
            .await?;

        let page_url = format!(
            "/s/{}{}",
            site_slug,
            if page.path == "/" {
                "".to_string()
            } else {
                page.path.clone()
            }
        );
        Ok(ToolResult::success_with_metadata(
            json!({
                "page_id": page.id.to_string(),
                "site_slug": site_slug,
                "path": page.path,
                "url": page_url,
                "form_collection": page.form_collection,
                "message": format!("Page '{}' created at /s/{}{}", page.title, site_slug, page.path)
            }),
            json!({
                "__canvas_action": "preview_site",
                "site_slug": site_slug,
                "url": page_url
            }),
        ))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── UpdatePage ───────────────────────────────────────────────────────────

/// Update specific fields of an existing page.
pub struct UpdatePageTool {
    db_pool: PgPool,
}

impl UpdatePageTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for UpdatePageTool {
    fn name(&self) -> &str {
        "update_page"
    }

    fn description(&self) -> &str {
        "Update an existing page's content. This is a convenience wrapper — calling create_page with the same site_slug and path also updates the page."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "site_slug": {
                    "type": "string",
                    "description": "Site slug"
                },
                "path": {
                    "type": "string",
                    "description": "Page path (e.g. '/', '/about')"
                },
                "title": {
                    "type": "string",
                    "description": "Updated page title"
                },
                "html_content": {
                    "type": "string",
                    "description": "Updated HTML content"
                },
                "css_content": {
                    "type": "string",
                    "description": "Updated CSS"
                },
                "js_content": {
                    "type": "string",
                    "description": "Updated JavaScript"
                }
            },
            "required": ["site_slug", "path", "title", "html_content"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        // Delegate to upsert_page (same behavior as create_page)
        let site_slug = params["site_slug"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("site_slug is required".to_string()))?;

        let path = params["path"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("path is required".to_string()))?;

        let title = params["title"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("title is required".to_string()))?;

        let html_content = params["html_content"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("html_content is required".to_string())
        })?;

        let css_content = params.get("css_content").and_then(|v| v.as_str());
        let js_content = params.get("js_content").and_then(|v| v.as_str());

        let engine = SiteEngine::new(self.db_pool.clone());
        let page = engine
            .upsert_page(
                site_slug,
                path,
                title,
                None,
                html_content,
                css_content,
                js_content,
                None,
                None,
                None,
            )
            .await?;

        let page_url = format!(
            "/s/{}{}",
            site_slug,
            if page.path == "/" {
                "".to_string()
            } else {
                page.path.clone()
            }
        );
        Ok(ToolResult::success_with_metadata(
            json!({
                "page_id": page.id.to_string(),
                "path": page.path,
                "url": page_url,
                "message": format!("Page '{}' updated", page.title)
            }),
            json!({
                "__canvas_action": "preview_site",
                "site_slug": site_slug,
                "url": page_url
            }),
        ))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── PatchPage ───────────────────────────────────────────────────────────

/// Surgically update a specific section of a page using search-and-replace.
/// Avoids full-page rewrites that introduce unintended changes.
pub struct PatchPageTool {
    db_pool: PgPool,
}

impl PatchPageTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PatchPageTool {
    fn name(&self) -> &str {
        "patch_page"
    }

    fn description(&self) -> &str {
        "Surgically update a specific section of a page without rewriting the entire content. Use this instead of update_page when you only need to change a button, heading, section, or style. Provide the exact existing content to find and the new content to replace it with. Supports patching HTML, CSS, and/or JS independently."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "site_slug": {
                    "type": "string",
                    "description": "Site slug"
                },
                "path": {
                    "type": "string",
                    "description": "Page path (e.g. '/', '/about')"
                },
                "patches": {
                    "type": "array",
                    "description": "Array of patches to apply. Each patch targets html, css, or js content.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "target": {
                                "type": "string",
                                "enum": ["html", "css", "js"],
                                "description": "Which content to patch: html, css, or js"
                            },
                            "old": {
                                "type": "string",
                                "description": "The exact existing content to find (must match exactly)"
                            },
                            "new": {
                                "type": "string",
                                "description": "The replacement content"
                            }
                        },
                        "required": ["target", "old", "new"]
                    }
                }
            },
            "required": ["site_slug", "path", "patches"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let site_slug = params["site_slug"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("site_slug is required".to_string()))?;

        let path = params["path"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("path is required".to_string()))?;

        let patches = params["patches"]
            .as_array()
            .ok_or_else(|| amos_core::AmosError::Validation("patches must be an array".to_string()))?;

        if patches.is_empty() {
            return Err(amos_core::AmosError::Validation(
                "patches array is empty".to_string(),
            ));
        }

        let engine = SiteEngine::new(self.db_pool.clone());

        // Fetch the current page
        let (_site, page) = engine.get_page(site_slug, path).await?;

        let mut html = page.html_content.clone();
        let mut css = page.css_content.clone().unwrap_or_default();
        let mut js = page.js_content.clone().unwrap_or_default();
        let mut applied = Vec::new();
        let mut errors = Vec::new();

        for (i, patch) in patches.iter().enumerate() {
            let target = patch["target"].as_str().unwrap_or("html");
            let old = match patch["old"].as_str() {
                Some(s) => s,
                None => {
                    errors.push(format!("patch[{}]: 'old' is required", i));
                    continue;
                }
            };
            let new = match patch["new"].as_str() {
                Some(s) => s,
                None => {
                    errors.push(format!("patch[{}]: 'new' is required", i));
                    continue;
                }
            };

            let content = match target {
                "html" => &mut html,
                "css" => &mut css,
                "js" => &mut js,
                _ => {
                    errors.push(format!("patch[{}]: invalid target '{}'", i, target));
                    continue;
                }
            };

            if content.contains(old) {
                *content = content.replacen(old, new, 1);
                applied.push(format!("patch[{}]: {} updated", i, target));
            } else {
                errors.push(format!(
                    "patch[{}]: '{}' not found in {} content (no match)",
                    i,
                    if old.len() > 60 {
                        format!("{}...", &old[..60])
                    } else {
                        old.to_string()
                    },
                    target
                ));
            }
        }

        if applied.is_empty() {
            return Ok(ToolResult::error(format!(
                "No patches applied — none of the old content fragments were found: {}",
                errors.join("; ")
            )));
        }

        // Save the patched content
        let _page = engine
            .upsert_page(
                site_slug,
                path,
                &page.title,
                page.description.as_deref(),
                &html,
                if css.is_empty() { None } else { Some(css.as_str()) },
                if js.is_empty() { None } else { Some(js.as_str()) },
                page.meta_title.as_deref(),
                page.meta_description.as_deref(),
                page.form_collection.as_deref(),
            )
            .await?;

        let page_url = format!(
            "/s/{}{}",
            site_slug,
            if path == "/" {
                "".to_string()
            } else {
                path.to_string()
            }
        );
        Ok(ToolResult::success_with_metadata(
            json!({
                "applied": applied,
                "errors": errors,
                "url": page_url,
                "message": format!("{} patch(es) applied, {} error(s)", applied.len(), errors.len())
            }),
            json!({
                "__canvas_action": "preview_site",
                "site_slug": site_slug,
                "url": page_url
            }),
        ))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── PublishSite ──────────────────────────────────────────────────────────

/// Publish a site to make it publicly accessible.
pub struct PublishSiteTool {
    db_pool: PgPool,
}

impl PublishSiteTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PublishSiteTool {
    fn name(&self) -> &str {
        "publish_site"
    }

    fn description(&self) -> &str {
        "Publish a site to make it publicly accessible at /s/{slug}. All published pages on the site become visible."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "site_slug": {
                    "type": "string",
                    "description": "Site slug to publish"
                }
            },
            "required": ["site_slug"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let site_slug = params["site_slug"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("site_slug is required".to_string()))?;

        let engine = SiteEngine::new(self.db_pool.clone());
        let site = engine.publish_site(site_slug, true).await?;

        // Get page count
        let pages = engine.list_pages(site_slug).await?;

        let site_url = format!("/s/{}", site.slug);
        Ok(ToolResult::success_with_metadata(
            json!({
                "site_slug": site.slug,
                "is_published": true,
                "url": site_url,
                "page_count": pages.len(),
                "pages": pages.iter().map(|p| json!({
                    "path": p.path,
                    "title": p.title,
                    "url": format!("/s/{}{}", site.slug, if p.path == "/" { "".to_string() } else { p.path.clone() })
                })).collect::<Vec<_>>(),
                "message": format!("Site '{}' published with {} pages at /s/{}", site.name, pages.len(), site.slug)
            }),
            json!({
                "__canvas_action": "preview_site",
                "site_slug": site.slug,
                "url": site_url
            }),
        ))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── ListSites ────────────────────────────────────────────────────────────

/// List all sites.
pub struct ListSitesTool {
    db_pool: PgPool,
}

impl ListSitesTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for ListSitesTool {
    fn name(&self) -> &str {
        "list_sites"
    }

    fn description(&self) -> &str {
        "List all websites and landing pages that have been created."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: JsonValue) -> Result<ToolResult> {
        let engine = SiteEngine::new(self.db_pool.clone());
        let sites = engine.list_sites().await?;

        let result: Vec<JsonValue> = sites
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "slug": s.slug,
                    "url": format!("/s/{}", s.slug),
                    "is_published": s.is_published,
                    "description": s.description,
                    "domain": s.domain,
                })
            })
            .collect();

        Ok(ToolResult::success(json!({
            "sites": result,
            "count": result.len()
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}
