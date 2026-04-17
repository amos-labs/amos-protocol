//! Agent tools for entity revision tracking and template management.
//!
//! Three tools:
//! - `query_revisions`: List or get specific revision(s) for an entity
//! - `revert_entity`: Revert an entity to a previous version
//! - `manage_templates`: List templates or check for updates on a subscribed entity

use crate::revisions::{RevertRequest, RevisionService, TemplateService};
use crate::tools::{Tool, ToolCategory, ToolResult};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

// ── QueryRevisionsTool ──────────────────────────────────────────────────

pub struct QueryRevisionsTool {
    db_pool: PgPool,
}

impl QueryRevisionsTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for QueryRevisionsTool {
    fn name(&self) -> &str {
        "query_revisions"
    }

    fn description(&self) -> &str {
        "Query revision history for an entity. Returns versions with timestamps, change types, diffs, and who made each change. Optionally provide a version number to get a specific revision's full snapshot and diff."
    }

    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity_type": {
                    "type": "string",
                    "description": "Entity type: 'integration', 'canvas', 'collection', 'site', 'page'"
                },
                "entity_id": {
                    "type": "string",
                    "description": "UUID of the entity"
                },
                "version": {
                    "type": "integer",
                    "description": "Optional: specific version number to retrieve (1-based). Omit to list all revisions."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return when listing (default 20)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Offset for pagination when listing (default 0)"
                }
            },
            "required": ["entity_type", "entity_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let entity_type = params["entity_type"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let entity_id = match params["entity_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            Some(id) => id,
            None => {
                return Ok(ToolResult::error(
                    "Invalid or missing entity_id".to_string(),
                ))
            }
        };

        let service = RevisionService::new(self.db_pool.clone());

        // If a specific version is requested, return that single revision
        if let Some(version) = params["version"].as_i64() {
            match service
                .get_revision(&entity_type, entity_id, version as i32)
                .await
            {
                Ok(revision) => {
                    return Ok(ToolResult::success(
                        serde_json::to_value(&revision).unwrap(),
                    ))
                }
                Err(e) => return Ok(ToolResult::error(e.to_string())),
            }
        }

        // Otherwise list all revisions
        let limit = params["limit"].as_i64().unwrap_or(20);
        let offset = params["offset"].as_i64().unwrap_or(0);

        match service
            .list_revisions(&entity_type, entity_id, limit, offset)
            .await
        {
            Ok(response) => Ok(ToolResult::success(serde_json::json!({
                "revisions": response.revisions,
                "total": response.total,
            }))),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

// ── RevertEntityTool ───────────────────────────────────────────────────

pub struct RevertEntityTool {
    db_pool: PgPool,
}

impl RevertEntityTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for RevertEntityTool {
    fn name(&self) -> &str {
        "revert_entity"
    }

    fn description(&self) -> &str {
        "Revert an entity to a previous version. Creates a new revision with the old snapshot (non-destructive)."
    }

    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity_type": {
                    "type": "string",
                    "description": "Entity type: 'integration', 'canvas', 'collection', 'site', 'page'"
                },
                "entity_id": {
                    "type": "string",
                    "description": "UUID of the entity"
                },
                "target_version": {
                    "type": "integer",
                    "description": "Version number to revert to"
                },
                "reason": {
                    "type": "string",
                    "description": "Optional reason for the revert"
                }
            },
            "required": ["entity_type", "entity_id", "target_version"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let entity_type = params["entity_type"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let entity_id = match params["entity_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            Some(id) => id,
            None => {
                return Ok(ToolResult::error(
                    "Invalid or missing entity_id".to_string(),
                ))
            }
        };
        let target_version = match params["target_version"].as_i64() {
            Some(v) => v as i32,
            None => return Ok(ToolResult::error("Missing target_version".to_string())),
        };

        let service = RevisionService::new(self.db_pool.clone());
        let request = RevertRequest {
            entity_type,
            entity_id,
            target_version,
            changed_by: "ai_agent".to_string(),
        };

        match service.revert_to_version(request).await {
            Ok(revision) => Ok(ToolResult::success(serde_json::json!({
                "reverted": true,
                "new_version": revision.version,
                "reverted_to": target_version,
                "revision_id": revision.id,
            }))),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

// ── ManageTemplatesTool ─────────────────────────────────────────────────

pub struct ManageTemplatesTool {
    db_pool: PgPool,
}

impl ManageTemplatesTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for ManageTemplatesTool {
    fn name(&self) -> &str {
        "manage_templates"
    }

    fn description(&self) -> &str {
        "List available templates or check if an entity's upstream template has updates. To list: provide optional entity_type filter. To check updates: provide entity_type + entity_id."
    }

    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity_type": {
                    "type": "string",
                    "description": "Entity type filter: 'integration', 'canvas', 'collection'. Optional for listing, required for checking updates."
                },
                "entity_id": {
                    "type": "string",
                    "description": "UUID of the entity to check for template updates. When provided, checks for updates instead of listing."
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let entity_type = params["entity_type"].as_str();
        let entity_id = params["entity_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok());

        // If entity_id is provided, check for updates on that specific entity
        if let (Some(et), Some(eid)) = (entity_type, entity_id) {
            let service = TemplateService::new(self.db_pool.clone());
            return match service.check_for_updates(et, eid).await {
                Ok(Some(result)) => Ok(ToolResult::success(serde_json::to_value(&result).unwrap())),
                Ok(None) => Ok(ToolResult::success(serde_json::json!({
                    "message": "Entity is not subscribed to any template",
                    "has_update": false,
                }))),
                Err(e) => Ok(ToolResult::error(e.to_string())),
            };
        }

        // Otherwise list templates
        let service = TemplateService::new(self.db_pool.clone());
        match service.list_templates(entity_type).await {
            Ok(templates) => Ok(ToolResult::success(serde_json::json!({
                "templates": templates.iter().map(|t| serde_json::json!({
                    "slug": t.slug,
                    "name": t.name,
                    "entity_type": t.entity_type,
                    "current_version": t.current_version,
                    "category": t.category,
                    "description": t.description,
                })).collect::<Vec<_>>(),
                "total": templates.len(),
            }))),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mock_pool() -> PgPool {
        use sqlx::postgres::PgPoolOptions;
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent_test_db")
            .unwrap()
    }

    fn all_tools() -> Vec<Box<dyn Tool>> {
        let pool = mock_pool();
        vec![
            Box::new(QueryRevisionsTool::new(pool.clone())),
            Box::new(RevertEntityTool::new(pool.clone())),
            Box::new(ManageTemplatesTool::new(pool)),
        ]
    }

    #[tokio::test]
    async fn test_query_revisions_tool_metadata() {
        let pool = mock_pool();
        let tool = QueryRevisionsTool::new(pool);
        assert_eq!(tool.name(), "query_revisions");
        assert_eq!(tool.category(), ToolCategory::Platform);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["entity_type"].is_object());
        assert!(schema["properties"]["entity_id"].is_object());
        assert!(schema["properties"]["version"].is_object());
    }

    #[tokio::test]
    async fn test_revert_entity_tool_metadata() {
        let pool = mock_pool();
        let tool = RevertEntityTool::new(pool);
        assert_eq!(tool.name(), "revert_entity");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["target_version"].is_object());
    }

    #[tokio::test]
    async fn test_manage_templates_tool_metadata() {
        let pool = mock_pool();
        let tool = ManageTemplatesTool::new(pool);
        assert_eq!(tool.name(), "manage_templates");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["entity_type"].is_object());
    }

    #[tokio::test]
    async fn all_revision_tools_have_unique_names() {
        let tools = all_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        let mut deduped = names.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(names.len(), deduped.len(), "All tool names must be unique");
    }

    #[tokio::test]
    async fn all_revision_tools_are_platform_category() {
        for tool in all_tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Platform,
                "Tool '{}' should be Platform category",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn all_revision_tools_have_descriptions() {
        for tool in all_tools() {
            assert!(
                !tool.description().is_empty(),
                "Tool '{}' should have a description",
                tool.name()
            );
            assert!(
                tool.description().len() > 20,
                "Tool '{}' description should be meaningful (>20 chars)",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn query_revisions_requires_entity_type_and_id() {
        let pool = mock_pool();
        let tool = QueryRevisionsTool::new(pool);
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("entity_type")));
        assert!(required.contains(&json!("entity_id")));
    }

    #[tokio::test]
    async fn revert_entity_requires_all_three() {
        let pool = mock_pool();
        let tool = RevertEntityTool::new(pool);
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("entity_type")));
        assert!(required.contains(&json!("entity_id")));
        assert!(required.contains(&json!("target_version")));
    }

    #[tokio::test]
    async fn manage_templates_has_no_required_fields() {
        let pool = mock_pool();
        let tool = ManageTemplatesTool::new(pool);
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(
            required.is_empty(),
            "manage_templates should have no required fields"
        );
    }

    #[tokio::test]
    async fn all_schemas_are_object_type() {
        for tool in all_tools() {
            let schema = tool.parameters_schema();
            assert_eq!(
                schema["type"],
                "object",
                "Tool '{}' schema should be type: object",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn query_revisions_returns_error_on_invalid_uuid() {
        let pool = mock_pool();
        let tool = QueryRevisionsTool::new(pool);
        let result = tool
            .execute(json!({
                "entity_type": "integration",
                "entity_id": "not-a-uuid"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Invalid"));
    }

    #[tokio::test]
    async fn revert_entity_returns_error_on_missing_target_version() {
        let pool = mock_pool();
        let tool = RevertEntityTool::new(pool);
        let result = tool
            .execute(json!({
                "entity_type": "integration",
                "entity_id": Uuid::new_v4().to_string()
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing target_version"));
    }
}
