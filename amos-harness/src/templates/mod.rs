//! Module/template apply engine (platform build-plan **P1**, slice 1).
//!
//! A **template** is runtime data — a composition of **components**, each a
//! reusable bundle of collections + canvas views + automations — that the
//! harness applies to itself. This is the runtime-data successor to the
//! compiled package-seed mechanism: modules are *data the substrate holds*,
//! applied via an MCP verb the actor calls, not compiled Rust.
//!
//! Composition is the point: the generic CRM is a set of reusable components,
//! and a vertical (e.g. construction) is more components composed on top — so
//! verticals reuse components rather than duplicating them.
//!
//! `apply` is **idempotent**: collections upsert by name; canvases and
//! automations are created only when one of the same name doesn't already
//! exist, so re-applying a template makes nothing new.

use crate::automations::{ActionType, TriggerType};
use crate::canvas::types::CanvasType;
use crate::canvas::CanvasEngine;
use crate::schema::{FieldDefinition, SchemaEngine};
use amos_core::{AmosError, AppConfig, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// A module/template: a named, versioned **composition of components**.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub components: Vec<Component>,
}

/// The reusable unit: a bundle of collections + canvases + automations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub collections: Vec<CollectionDef>,
    #[serde(default)]
    pub canvases: Vec<CanvasDef>,
    #[serde(default)]
    pub automations: Vec<AutomationDef>,
}

/// A collection to define (maps onto `SchemaEngine::define_collection`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionDef {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub fields: Vec<FieldDefinition>,
}

/// A canvas view to create (maps onto `CanvasEngine::create_canvas`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Canvas type slug (e.g. `datagrid`, `form`, `dashboard`). Default `datagrid`.
    #[serde(default = "default_canvas_type")]
    pub canvas_type: String,
    #[serde(default)]
    pub html_content: Option<String>,
    #[serde(default)]
    pub js_content: Option<String>,
    #[serde(default)]
    pub css_content: Option<String>,
    #[serde(default)]
    pub data_sources: Option<JsonValue>,
    #[serde(default)]
    pub layout_config: Option<JsonValue>,
}

fn default_canvas_type() -> String {
    "datagrid".to_string()
}

/// An automation to create (maps onto `AutomationEngine::create_automation`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub trigger_type: String,
    #[serde(default = "empty_obj")]
    pub trigger_config: JsonValue,
    #[serde(default)]
    pub condition: Option<JsonValue>,
    pub action_type: String,
    #[serde(default = "empty_obj")]
    pub action_config: JsonValue,
}

fn empty_obj() -> JsonValue {
    json!({})
}

/// What applying one component did — the names created-or-already-present.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComponentReport {
    pub component: String,
    pub version: String,
    pub collections_applied: Vec<String>,
    pub canvases_applied: Vec<String>,
    pub automations_applied: Vec<String>,
}

/// The result of applying a whole template, grouped by component.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApplyReport {
    pub template: String,
    pub version: String,
    pub components: Vec<ComponentReport>,
}

/// Applies templates to the harness runtime, driving the existing engines.
pub struct TemplateEngine {
    db_pool: PgPool,
    config: Arc<AppConfig>,
}

impl TemplateEngine {
    pub fn new(db_pool: PgPool, config: Arc<AppConfig>) -> Self {
        Self { db_pool, config }
    }

    /// Apply a template idempotently, component by component.
    pub async fn apply(&self, tmpl: &Template) -> Result<ApplyReport> {
        let mut report = ApplyReport {
            template: tmpl.name.clone(),
            version: tmpl.version.clone(),
            components: Vec::with_capacity(tmpl.components.len()),
        };
        for component in &tmpl.components {
            report
                .components
                .push(self.apply_component(component).await?);
        }
        Ok(report)
    }

    async fn apply_component(&self, c: &Component) -> Result<ComponentReport> {
        let mut r = ComponentReport {
            component: c.name.clone(),
            version: c.version.clone(),
            ..Default::default()
        };

        // Collections — define_collection upserts by name (idempotent).
        let schema = SchemaEngine::new(self.db_pool.clone());
        for col in &c.collections {
            schema
                .define_collection(
                    &col.name,
                    &col.display_name,
                    col.description.as_deref(),
                    col.fields.clone(),
                )
                .await?;
            r.collections_applied.push(col.name.clone());
        }

        // Canvases — create only if one of the same name doesn't exist
        // (create_canvas always INSERTs; the slug is unique, so guard by name).
        let canvas_engine = CanvasEngine::new(self.db_pool.clone(), self.config.clone());
        for cv in &c.canvases {
            if !self.canvas_exists(&cv.name).await? {
                canvas_engine
                    .create_canvas(
                        cv.name.clone(),
                        cv.description.clone(),
                        CanvasType::from_str(&cv.canvas_type),
                        cv.html_content.clone().unwrap_or_default(),
                        cv.js_content.clone(),
                        cv.css_content.clone(),
                        cv.data_sources.clone(),
                        None,
                        cv.layout_config.clone(),
                    )
                    .await?;
            }
            r.canvases_applied.push(cv.name.clone());
        }

        // Automations — validate types, then create only if name is new.
        for a in &c.automations {
            if TriggerType::from_str(&a.trigger_type).is_none() {
                return Err(AmosError::Validation(format!(
                    "invalid trigger_type '{}' in automation '{}'",
                    a.trigger_type, a.name
                )));
            }
            if ActionType::from_str(&a.action_type).is_none() {
                return Err(AmosError::Validation(format!(
                    "invalid action_type '{}' in automation '{}'",
                    a.action_type, a.name
                )));
            }
            if !self.automation_exists(&a.name).await? {
                sqlx::query(
                    r#"INSERT INTO automations
                         (name, description, trigger_type, trigger_config, condition, action_type, action_config)
                       VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
                )
                .bind(&a.name)
                .bind(a.description.as_deref())
                .bind(&a.trigger_type)
                .bind(&a.trigger_config)
                .bind(&a.condition)
                .bind(&a.action_type)
                .bind(&a.action_config)
                .execute(&self.db_pool)
                .await
                .map_err(|e| AmosError::Internal(format!("Failed to create automation: {e}")))?;
            }
            r.automations_applied.push(a.name.clone());
        }

        Ok(r)
    }

    async fn canvas_exists(&self, name: &str) -> Result<bool> {
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM canvases WHERE name = $1 LIMIT 1")
                .bind(name)
                .fetch_optional(&self.db_pool)
                .await
                .map_err(|e| AmosError::Internal(format!("canvas existence check: {e}")))?;
        Ok(id.is_some())
    }

    async fn automation_exists(&self, name: &str) -> Result<bool> {
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM automations WHERE name = $1 LIMIT 1")
                .bind(name)
                .fetch_optional(&self.db_pool)
                .await
                .map_err(|e| AmosError::Internal(format!("automation existence check: {e}")))?;
        Ok(id.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::FieldType;

    // The shipped proof artifact — two composed components.
    const MINI_CRM: &str = include_str!("../../templates/mini-crm.json");

    #[test]
    fn mini_crm_parses_as_two_components() {
        let t: Template = serde_json::from_str(MINI_CRM).expect("mini-crm.json should parse");
        assert_eq!(t.name, "mini-crm");
        let comp_names: Vec<&str> = t.components.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(comp_names, vec!["contacts", "pipeline"]);

        // contacts component: contacts collection + a "Contacts" datagrid canvas.
        let contacts = t.components.iter().find(|c| c.name == "contacts").unwrap();
        assert_eq!(contacts.collections.len(), 1);
        assert_eq!(contacts.collections[0].name, "contacts");
        assert_eq!(contacts.canvases.len(), 1);
        assert_eq!(contacts.canvases[0].name, "Contacts");
        assert_eq!(contacts.canvases[0].canvas_type, "datagrid");

        // pipeline component: deals collection with a reference to contacts.
        let pipeline = t.components.iter().find(|c| c.name == "pipeline").unwrap();
        let deals = &pipeline.collections[0];
        assert_eq!(deals.name, "deals");
        assert!(
            deals
                .fields
                .iter()
                .any(|f| matches!(f.field_type, FieldType::Reference)),
            "deals should carry a reference field to contacts"
        );
    }

    #[test]
    fn canvas_type_defaults_to_datagrid() {
        let json = r#"{"name":"t","version":"1","components":[
            {"name":"c","version":"1","canvases":[{"name":"X"}]}]}"#;
        let t: Template = serde_json::from_str(json).unwrap();
        assert_eq!(t.components[0].canvases[0].canvas_type, "datagrid");
    }

    #[test]
    fn empty_sections_default_to_empty() {
        let t: Template =
            serde_json::from_str(r#"{"name":"t","version":"1"}"#).expect("minimal template parses");
        assert!(t.components.is_empty());
        let t2: Template = serde_json::from_str(
            r#"{"name":"t","version":"1","components":[{"name":"c","version":"1"}]}"#,
        )
        .unwrap();
        let c = &t2.components[0];
        assert!(c.collections.is_empty() && c.canvases.is_empty() && c.automations.is_empty());
    }
}
