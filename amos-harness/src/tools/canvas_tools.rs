//! Canvas manipulation tools

use super::{Tool, ToolCategory, ToolResult};
use crate::bedrock::BedrockClient;
use crate::canvas::{generator, CanvasEngine, CanvasType};
use amos_core::{AmosError, AppConfig, Result};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Load and display an existing canvas
pub struct LoadCanvasTool {
    db_pool: PgPool,
}

impl LoadCanvasTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for LoadCanvasTool {
    fn name(&self) -> &str {
        "load_canvas"
    }

    fn description(&self) -> &str {
        "Load and display an existing canvas by its slug"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "slug": {
                    "type": "string",
                    "description": "The canvas slug to load"
                },
                "data_context": {
                    "type": "object",
                    "description": "Optional data context for rendering"
                }
            },
            "required": ["slug"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let slug = params["slug"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("slug is required".to_string()))?;

        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config);

        let canvas = engine.get_canvas_by_slug(slug).await?;
        let data_context = params.get("data_context").cloned();

        let response = engine.render_canvas(&canvas, data_context).await?;

        Ok(ToolResult::success(
            serde_json::to_value(response)
                .map_err(|e| AmosError::Internal(format!("Failed to serialize response: {}", e)))?,
        ))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Canvas
    }
}

/// Create a new data-driven canvas
pub struct CreateDynamicCanvasTool {
    db_pool: PgPool,
    bedrock: Option<Arc<BedrockClient>>,
}

impl CreateDynamicCanvasTool {
    pub fn new(db_pool: PgPool, bedrock: Option<Arc<BedrockClient>>) -> Self {
        Self { db_pool, bedrock }
    }
}

#[async_trait]
impl Tool for CreateDynamicCanvasTool {
    fn name(&self) -> &str {
        "create_dynamic_canvas"
    }

    fn description(&self) -> &str {
        "Create a new data-driven canvas (list, table, dashboard, form, etc.)"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Canvas name"
                },
                "canvas_type": {
                    "type": "string",
                    "enum": ["dynamic", "dashboard", "datagrid", "form", "detail", "kanban", "calendar", "report", "wizard"],
                    "description": "Type of canvas to create"
                },
                "description": {
                    "type": "string",
                    "description": "What should this canvas do?"
                },
                "data_sources": {
                    "type": "object",
                    "description": "Data source configuration"
                },
                "actions": {
                    "type": "object",
                    "description": "Action buttons configuration"
                }
            },
            "required": ["name", "canvas_type", "description"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("name is required".to_string()))?;

        let canvas_type_str = params["canvas_type"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("canvas_type is required".to_string())
        })?;

        let canvas_type = CanvasType::from_str(canvas_type_str);

        let description = params["description"].as_str().unwrap_or("");

        // Generate canvas using AI
        let generate_request = generator::GenerateCanvasRequest {
            module_definition: None,
            canvas_type: canvas_type.clone(),
            description: description.to_string(),
            requirements: None,
            sample_data: params.get("data_sources").cloned(),
        };

        let generated =
            generator::generate_canvas(generate_request, self.bedrock.as_deref()).await?;

        // Create canvas in database
        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config);

        let canvas = engine
            .create_canvas(
                name.to_string(),
                Some(description.to_string()),
                canvas_type,
                generated.html_content,
                generated.js_content,
                generated.css_content,
                params.get("data_sources").cloned(),
                params.get("actions").cloned(),
                None,
            )
            .await?;

        Ok(ToolResult::success(json!({
            "canvas_id": canvas.id.to_string(),
            "slug": canvas.slug,
            "warnings": generated.warnings
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Canvas
    }
}

/// Create a freeform canvas with custom HTML/JS/CSS
pub struct CreateFreeformCanvasTool {
    db_pool: PgPool,
}

impl CreateFreeformCanvasTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for CreateFreeformCanvasTool {
    fn name(&self) -> &str {
        "create_freeform_canvas"
    }

    fn description(&self) -> &str {
        "Create a custom freeform canvas with full HTML, CSS, and JavaScript control"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Canvas name"
                },
                "description": {
                    "type": "string",
                    "description": "What should this canvas do?"
                },
                "html_content": {
                    "type": "string",
                    "description": "Custom HTML content"
                },
                "js_content": {
                    "type": "string",
                    "description": "Custom JavaScript code"
                },
                "css_content": {
                    "type": "string",
                    "description": "Custom CSS styles"
                }
            },
            "required": ["name", "description", "html_content"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("name is required".to_string()))?;

        let description = params["description"].as_str().unwrap_or("");
        let html_content = params["html_content"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("html_content is required".to_string())
        })?;

        let js_content = params
            .get("js_content")
            .and_then(|v| v.as_str())
            .map(String::from);
        let css_content = params
            .get("css_content")
            .and_then(|v| v.as_str())
            .map(String::from);

        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config);

        let canvas = engine
            .create_canvas(
                name.to_string(),
                Some(description.to_string()),
                CanvasType::Freeform,
                html_content.to_string(),
                js_content,
                css_content,
                None,
                None,
                None,
            )
            .await?;

        Ok(ToolResult::success(json!({
            "canvas_id": canvas.id.to_string(),
            "slug": canvas.slug
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Canvas
    }
}

/// Update an existing canvas
pub struct UpdateCanvasTool {
    db_pool: PgPool,
}

impl UpdateCanvasTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for UpdateCanvasTool {
    fn name(&self) -> &str {
        "update_canvas"
    }

    fn description(&self) -> &str {
        "Update an existing canvas's content or configuration"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "canvas_id": {
                    "type": "string",
                    "description": "Canvas UUID to update"
                },
                "name": {
                    "type": "string",
                    "description": "New canvas name"
                },
                "html_content": {
                    "type": "string",
                    "description": "Updated HTML content"
                },
                "js_content": {
                    "type": "string",
                    "description": "Updated JavaScript content"
                },
                "css_content": {
                    "type": "string",
                    "description": "Updated CSS content"
                },
                "description": {
                    "type": "string",
                    "description": "Updated canvas description"
                }
            },
            "required": ["canvas_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let canvas_id_str = params["canvas_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("canvas_id is required".to_string()))?;
        let canvas_id = Uuid::parse_str(canvas_id_str).map_err(|e| {
            amos_core::AmosError::Validation(format!("Invalid canvas_id UUID: {}", e))
        })?;

        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config);

        let updates = crate::canvas::CanvasUpdate {
            name: params
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from),
            description: params
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from),
            html_content: params
                .get("html_content")
                .and_then(|v| v.as_str())
                .map(String::from),
            js_content: params
                .get("js_content")
                .and_then(|v| v.as_str())
                .map(String::from),
            css_content: params
                .get("css_content")
                .and_then(|v| v.as_str())
                .map(String::from),
            data_sources: params.get("data_sources").cloned(),
            actions: params.get("actions").cloned(),
            metadata: None,
        };

        let canvas = engine.update_canvas(canvas_id, updates).await?;

        Ok(ToolResult::success(json!({
            "canvas_id": canvas.id.to_string(),
            "slug": canvas.slug,
            "version": canvas.version
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Canvas
    }
}

/// Surgically update a specific section of a canvas using search-and-replace.
pub struct PatchCanvasTool {
    db_pool: PgPool,
}

impl PatchCanvasTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PatchCanvasTool {
    fn name(&self) -> &str {
        "patch_canvas"
    }

    fn description(&self) -> &str {
        "Surgically update a specific section of a canvas without rewriting the entire content. Use this instead of update_canvas when you only need to change a button, heading, section, or style. Provide the exact existing content to find and the new content to replace it with. Supports patching HTML, CSS, and/or JS independently."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "canvas_id": {
                    "type": "string",
                    "description": "Canvas UUID to patch"
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
            "required": ["canvas_id", "patches"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let canvas_id_str = params["canvas_id"]
            .as_str()
            .ok_or_else(|| AmosError::Validation("canvas_id is required".to_string()))?;
        let canvas_id = Uuid::parse_str(canvas_id_str)
            .map_err(|e| AmosError::Validation(format!("Invalid canvas_id UUID: {}", e)))?;

        let patches = params["patches"]
            .as_array()
            .ok_or_else(|| AmosError::Validation("patches must be an array".to_string()))?;

        if patches.is_empty() {
            return Err(AmosError::Validation("patches array is empty".to_string()));
        }

        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config);

        // Fetch current canvas
        let canvas = engine.get_canvas(canvas_id).await?;

        let mut html = canvas.html_content.clone().unwrap_or_default();
        let mut css = canvas.css_content.clone().unwrap_or_default();
        let mut js = canvas.js_content.clone().unwrap_or_default();
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
        let updates = crate::canvas::CanvasUpdate {
            html_content: Some(html),
            css_content: Some(css),
            js_content: Some(js),
            ..Default::default()
        };

        let canvas = engine.update_canvas(canvas_id, updates).await?;

        Ok(ToolResult::success(json!({
            "canvas_id": canvas.id.to_string(),
            "slug": canvas.slug,
            "version": canvas.version,
            "applied": applied,
            "errors": errors,
            "message": format!("{} patch(es) applied, {} error(s)", applied.len(), errors.len())
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Canvas
    }
}

/// Publish a canvas (make it publicly accessible)
pub struct PublishCanvasTool {
    db_pool: PgPool,
}

impl PublishCanvasTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PublishCanvasTool {
    fn name(&self) -> &str {
        "publish_canvas"
    }

    fn description(&self) -> &str {
        "Make a canvas publicly accessible via a unique URL"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "canvas_id": {
                    "type": "string",
                    "description": "Canvas UUID to publish"
                }
            },
            "required": ["canvas_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let canvas_id_str = params["canvas_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("canvas_id is required".to_string()))?;
        let canvas_id = Uuid::parse_str(canvas_id_str).map_err(|e| {
            amos_core::AmosError::Validation(format!("Invalid canvas_id UUID: {}", e))
        })?;

        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config.clone());

        let public_slug = engine.publish_canvas(canvas_id).await?;
        let public_url = format!("{}/c/{}", config.server.rails_url, public_slug);

        Ok(ToolResult::success(json!({
            "public_slug": public_slug,
            "public_url": public_url
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Canvas
    }
}
