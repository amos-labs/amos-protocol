//! Platform CRUD tools for business data

use super::{Tool, ToolCategory, ToolResult};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::{Column, PgPool, Row};

/// Query records from any module
pub struct PlatformQueryTool {
    db_pool: PgPool,
}

impl PlatformQueryTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PlatformQueryTool {
    fn name(&self) -> &str {
        "platform_query"
    }

    fn description(&self) -> &str {
        "Query records from any platform module with filters and sorting"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "module": {
                    "type": "string",
                    "description": "Module name (table name)"
                },
                "filters": {
                    "type": "object",
                    "description": "Filter conditions (e.g., {\"status\": \"active\"})"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of records to return",
                    "default": 50
                },
                "offset": {
                    "type": "integer",
                    "description": "Number of records to skip",
                    "default": 0
                },
                "order_by": {
                    "type": "string",
                    "description": "Field to sort by"
                },
                "order_direction": {
                    "type": "string",
                    "enum": ["asc", "desc"],
                    "description": "Sort direction"
                }
            },
            "required": ["module"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let module = params["module"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("module is required".to_string()))?;

        let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);
        let offset = params.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);

        // SECURITY: Denylist of sensitive internal tables that agents must never access.
        // Each customer gets their own isolated Docker container and database, so all
        // non-sensitive tables are accessible (including customer-created tables from
        // packages, migrations, etc.). The table name must also pass sanitization.
        const DENIED_TABLES: &[&str] = &[
            "credential_vault",
            "integration_credentials",
            "sessions",
            "memory_entries",
            "harness_settings",
            "llm_providers",
            "_sqlx_migrations",
        ];

        // Sanitize: only allow alphanumeric and underscore (prevents SQL injection)
        if !module
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            || module.is_empty()
        {
            return Ok(ToolResult::error(format!(
                "Invalid module name: '{}'. Module names can only contain letters, numbers, and underscores.",
                module
            )));
        }

        if DENIED_TABLES.contains(&module) {
            return Ok(ToolResult::error(format!(
                "Access denied: '{}' is a restricted system table.",
                module
            )));
        }

        // Verify the table actually exists in the database before querying
        let table_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = $1)"
        )
        .bind(module)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("Database: Table check failed: {}", e)))?;

        if !table_exists {
            return Ok(ToolResult::error(format!(
                "Module '{}' does not exist.",
                module
            )));
        }

        // Safe to interpolate — module passed sanitization (alphanumeric + underscore only),
        // is not in the denylist, and was confirmed to exist in the database
        let query = format!(
            "SELECT * FROM {} ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            module
        );

        let rows = sqlx::query(&query)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| {
                amos_core::AmosError::Internal(format!(
                    "Database: {}",
                    format!("Query failed: {}", e)
                ))
            })?;

        // Convert rows to JSON
        let mut records = Vec::new();
        for row in rows {
            let mut record = serde_json::Map::new();
            for (i, column) in row.columns().iter().enumerate() {
                let name = column.name();
                if let Ok(value) = row.try_get::<String, _>(i) {
                    record.insert(name.to_string(), JsonValue::String(value));
                } else if let Ok(value) = row.try_get::<i64, _>(i) {
                    record.insert(name.to_string(), JsonValue::Number(value.into()));
                } else if let Ok(value) = row.try_get::<bool, _>(i) {
                    record.insert(name.to_string(), JsonValue::Bool(value));
                }
            }
            records.push(JsonValue::Object(record));
        }

        Ok(ToolResult::success(json!({
            "records": records,
            "count": records.len()
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }
}

/// Create a record in any module
pub struct PlatformCreateTool {
    db_pool: PgPool,
}

impl PlatformCreateTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PlatformCreateTool {
    fn name(&self) -> &str {
        "platform_create"
    }

    fn description(&self) -> &str {
        "Create a new record in any platform module"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "module": {
                    "type": "string",
                    "description": "Module name (table name)"
                },
                "data": {
                    "type": "object",
                    "description": "Record data to create"
                }
            },
            "required": ["module", "data"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let module = params["module"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("module is required".to_string()))?;

        let _data = params
            .get("data")
            .ok_or_else(|| amos_core::AmosError::Validation("data is required".to_string()))?;

        // In production, this would use the module system to validate and create records
        // For now, return a stub response
        Ok(ToolResult::success(json!({
            "id": 1,
            "module": module,
            "created": true,
            "message": "Record created successfully (stub)"
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }
}

/// Update a record in any module
pub struct PlatformUpdateTool {
    db_pool: PgPool,
}

impl PlatformUpdateTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PlatformUpdateTool {
    fn name(&self) -> &str {
        "platform_update"
    }

    fn description(&self) -> &str {
        "Update an existing record in any platform module"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "module": {
                    "type": "string",
                    "description": "Module name (table name)"
                },
                "id": {
                    "type": "integer",
                    "description": "Record ID to update"
                },
                "data": {
                    "type": "object",
                    "description": "Fields to update"
                }
            },
            "required": ["module", "id", "data"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let module = params["module"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("module is required".to_string()))?;

        let id = params["id"]
            .as_i64()
            .ok_or_else(|| amos_core::AmosError::Validation("id is required".to_string()))?;

        Ok(ToolResult::success(json!({
            "id": id,
            "module": module,
            "updated": true,
            "message": "Record updated successfully (stub)"
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }
}

/// Execute a module action
pub struct PlatformExecuteTool {
    db_pool: PgPool,
}

impl PlatformExecuteTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for PlatformExecuteTool {
    fn name(&self) -> &str {
        "platform_execute"
    }

    fn description(&self) -> &str {
        "Execute a custom action on a module or record"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "module": {
                    "type": "string",
                    "description": "Module name"
                },
                "action": {
                    "type": "string",
                    "description": "Action name to execute"
                },
                "record_id": {
                    "type": "integer",
                    "description": "Record ID (if action is record-specific)"
                },
                "params": {
                    "type": "object",
                    "description": "Action parameters"
                }
            },
            "required": ["module", "action"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let module = params["module"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("module is required".to_string()))?;

        let action = params["action"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("action is required".to_string()))?;

        Ok(ToolResult::success(json!({
            "module": module,
            "action": action,
            "executed": true,
            "message": "Action executed successfully (stub)"
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }
}
