//! Template tools — the actor's interface to apply a module/template.
//!
//! MCP is the seam: the customer's AI (or the managed agent) calls
//! `apply_template` to seed this environment with a composition of components
//! (collections + canvas views + automations). Idempotent.

use super::{Tool, ToolCategory, ToolResult};
use crate::templates::{Template, TemplateEngine};
use amos_core::{AppConfig, Result};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::Arc;

/// Apply a module/template (a composition of components) to this environment.
pub struct ApplyTemplateTool {
    db_pool: PgPool,
    config: Arc<AppConfig>,
}

impl ApplyTemplateTool {
    pub fn new(db_pool: PgPool, config: Arc<AppConfig>) -> Self {
        Self { db_pool, config }
    }
}

#[async_trait]
impl Tool for ApplyTemplateTool {
    fn name(&self) -> &str {
        "apply_template"
    }

    fn description(&self) -> &str {
        "Apply a module/template to this environment. A template is a composition of components; each component bundles collections, canvas views, and automations. Idempotent — re-applying creates no duplicates. Pass { name, version, components: [{ name, version, collections[], canvases[], automations[] }] }."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Template name" },
                "version": { "type": "string", "description": "Template version" },
                "components": {
                    "type": "array",
                    "description": "The components to compose, each a bundle of collections/canvases/automations",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "version": { "type": "string" },
                            "collections": { "type": "array" },
                            "canvases": { "type": "array" },
                            "automations": { "type": "array" }
                        },
                        "required": ["name", "version"]
                    }
                }
            },
            "required": ["name", "version", "components"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let tmpl: Template = serde_json::from_value(params)
            .map_err(|e| amos_core::AmosError::Validation(format!("invalid template: {e}")))?;
        let engine = TemplateEngine::new(self.db_pool.clone(), self.config.clone());
        let report = engine.apply(&tmpl).await?;

        let collections: usize = report
            .components
            .iter()
            .map(|c| c.collections_applied.len())
            .sum();
        let canvases: usize = report
            .components
            .iter()
            .map(|c| c.canvases_applied.len())
            .sum();
        let automations: usize = report
            .components
            .iter()
            .map(|c| c.automations_applied.len())
            .sum();

        Ok(ToolResult::success(json!({
            "template": report.template,
            "version": report.version,
            "components": report.components,
            "message": format!(
                "Applied template '{}' v{}: {} component(s), {} collections, {} canvases, {} automations",
                report.template, report.version, report.components.len(), collections, canvases, automations
            )
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}
