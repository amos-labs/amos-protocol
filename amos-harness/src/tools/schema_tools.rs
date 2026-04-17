//! Dynamic schema tools — the AI agent's interface to the collection/record system.
//!
//! These tools let the agent define data collections (like "contacts", "deals"),
//! then create, query, update, and delete records within them.

use super::{Tool, ToolCategory, ToolResult};
use crate::automations::TriggerEvent;
use crate::schema::{FieldDefinition, SchemaEngine};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

// ── DefineCollection ─────────────────────────────────────────────────────

/// Define or update a data collection's schema.
pub struct DefineCollectionTool {
    db_pool: PgPool,
}

impl DefineCollectionTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for DefineCollectionTool {
    fn name(&self) -> &str {
        "define_collection"
    }

    fn description(&self) -> &str {
        "Define or update a data collection (like a database table). Use this to create structured data storage for contacts, deals, tasks, products, invoices, or any business data. Provide the collection name, display name, and an array of field definitions with types."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Machine-readable slug (lowercase, underscores). Examples: 'contacts', 'deals', 'support_tickets'"
                },
                "display_name": {
                    "type": "string",
                    "description": "Human-readable name. Examples: 'Contacts', 'Deals', 'Support Tickets'"
                },
                "description": {
                    "type": "string",
                    "description": "What this collection is for"
                },
                "fields": {
                    "type": "array",
                    "description": "Array of field definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Field slug (lowercase, underscores)"
                            },
                            "display_name": {
                                "type": "string",
                                "description": "Human-readable field label"
                            },
                            "field_type": {
                                "type": "string",
                                "enum": ["text", "rich_text", "number", "decimal", "boolean", "date", "date_time", "enum", "reference", "email", "url", "phone", "json"],
                                "description": "Field data type"
                            },
                            "required": {
                                "type": "boolean",
                                "description": "Whether this field is required (default: false)"
                            },
                            "unique": {
                                "type": "boolean",
                                "description": "Whether values must be unique (default: false)"
                            },
                            "description": {
                                "type": "string",
                                "description": "What this field is for"
                            },
                            "default_value": {
                                "description": "Default value if not provided when creating a record"
                            },
                            "options": {
                                "type": "object",
                                "description": "Type-specific options. For 'enum': {\"choices\": [\"a\", \"b\"]}. For 'reference': {\"collection\": \"other_collection\"}."
                            }
                        },
                        "required": ["name", "display_name", "field_type"]
                    }
                }
            },
            "required": ["name", "display_name", "fields"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("name is required".to_string()))?;

        let display_name = params["display_name"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("display_name is required".to_string())
        })?;

        let description = params.get("description").and_then(|v| v.as_str());

        let fields_val = params
            .get("fields")
            .ok_or_else(|| amos_core::AmosError::Validation("fields is required".to_string()))?;

        let fields: Vec<FieldDefinition> =
            serde_json::from_value(fields_val.clone()).map_err(|e| {
                amos_core::AmosError::Validation(format!("Invalid field definitions: {}", e))
            })?;

        let engine = SchemaEngine::new(self.db_pool.clone());
        let collection = engine
            .define_collection(name, display_name, description, fields)
            .await?;

        Ok(ToolResult::success(json!({
            "collection_id": collection.id.to_string(),
            "name": collection.name,
            "display_name": collection.display_name,
            "field_count": collection.fields.len(),
            "fields": collection.fields.iter().map(|f| json!({
                "name": f.name,
                "type": format!("{:?}", f.field_type).to_lowercase()
            })).collect::<Vec<_>>(),
            "message": format!("Collection '{}' defined successfully with {} fields", collection.display_name, collection.fields.len())
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── DescribeCollections ──────────────────────────────────────────────────

/// List all collections or get the detailed schema for a specific one.
pub struct DescribeCollectionsTool {
    db_pool: PgPool,
}

impl DescribeCollectionsTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for DescribeCollectionsTool {
    fn name(&self) -> &str {
        "describe_collections"
    }

    fn description(&self) -> &str {
        "List all data collections or get the full schema of a specific one. Provide 'name' to get a single collection's detailed schema with all field definitions; omit 'name' to list all collections with summaries."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Optional: collection slug name (e.g. 'contacts', 'deals'). If provided, returns the full schema for that collection. If omitted, lists all collections."
                }
            },
            "required": []
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let engine = SchemaEngine::new(self.db_pool.clone());

        // If name is provided, return detailed schema for that collection
        if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
            let collection = engine.get_collection(name).await?;
            return Ok(ToolResult::success(
                serde_json::to_value(&collection).map_err(|e| {
                    amos_core::AmosError::Internal(format!("Failed to serialize collection: {}", e))
                })?,
            ));
        }

        // Otherwise list all collections
        let collections = engine.list_collections().await?;

        let result: Vec<JsonValue> = collections
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "display_name": c.display_name,
                    "description": c.description,
                    "field_count": c.fields.len(),
                    "fields": c.fields.iter().map(|f| json!({
                        "name": f.name,
                        "display_name": f.display_name,
                        "type": format!("{:?}", f.field_type).to_lowercase(),
                        "required": f.required,
                    })).collect::<Vec<_>>()
                })
            })
            .collect();

        Ok(ToolResult::success(json!({
            "collections": result,
            "count": result.len()
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── CreateRecord ─────────────────────────────────────────────────────────

/// Create a new record in a data collection.
pub struct CreateRecordTool {
    db_pool: PgPool,
    event_tx: Option<mpsc::Sender<TriggerEvent>>,
}

impl CreateRecordTool {
    pub fn new(db_pool: PgPool, event_tx: Option<mpsc::Sender<TriggerEvent>>) -> Self {
        Self { db_pool, event_tx }
    }
}

#[async_trait]
impl Tool for CreateRecordTool {
    fn name(&self) -> &str {
        "create_record"
    }

    fn description(&self) -> &str {
        "Create a new record in a data collection. Provide the collection name and the record data as a JSON object with field names as keys."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "collection": {
                    "type": "string",
                    "description": "Collection slug name (e.g. 'contacts', 'deals')"
                },
                "data": {
                    "type": "object",
                    "description": "Record data — keys are field names, values are the data. Example: {\"name\": \"John\", \"email\": \"john@example.com\", \"stage\": \"lead\"}"
                }
            },
            "required": ["collection", "data"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let collection_name = params["collection"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("collection is required".to_string())
        })?;

        let data = params
            .get("data")
            .cloned()
            .ok_or_else(|| amos_core::AmosError::Validation("data is required".to_string()))?;

        let engine = match &self.event_tx {
            Some(tx) => SchemaEngine::with_event_sender(self.db_pool.clone(), tx.clone()),
            None => SchemaEngine::new(self.db_pool.clone()),
        };
        let record = engine.create_record(collection_name, data).await?;

        Ok(ToolResult::success(json!({
            "record_id": record.id.to_string(),
            "collection": record.collection_name,
            "data": record.data,
            "created_at": record.created_at.to_rfc3339(),
            "message": format!("Record created in '{}'", record.collection_name)
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── QueryRecords ─────────────────────────────────────────────────────────

/// Query and filter records from a data collection.
pub struct QueryRecordsTool {
    db_pool: PgPool,
}

impl QueryRecordsTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for QueryRecordsTool {
    fn name(&self) -> &str {
        "query_records"
    }

    fn description(&self) -> &str {
        "Query records from a data collection with optional filters, sorting, and pagination. Filters use exact equality matching — e.g. {\"stage\": \"lead\"} returns only records where stage equals 'lead'."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "collection": {
                    "type": "string",
                    "description": "Collection slug name"
                },
                "filters": {
                    "type": "object",
                    "description": "Equality filters as key-value pairs. Example: {\"stage\": \"lead\", \"priority\": \"high\"}"
                },
                "sort_by": {
                    "type": "string",
                    "description": "Field to sort by (default: 'created_at'). Can be any field name or 'created_at'/'updated_at'."
                },
                "sort_dir": {
                    "type": "string",
                    "enum": ["asc", "desc"],
                    "description": "Sort direction (default: 'desc')"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum records to return (default: 50, max: 200)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Number of records to skip (for pagination, default: 0)"
                }
            },
            "required": ["collection"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let collection_name = params["collection"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("collection is required".to_string())
        })?;

        let filters = params.get("filters").cloned();
        let sort_by = params.get("sort_by").and_then(|v| v.as_str());
        let sort_dir = params.get("sort_dir").and_then(|v| v.as_str());
        let limit = params
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(50)
            .min(200);
        let offset = params.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);

        let engine = SchemaEngine::new(self.db_pool.clone());
        let (records, total) = engine
            .query_records(collection_name, filters, sort_by, sort_dir, limit, offset)
            .await?;

        let result: Vec<JsonValue> = records
            .iter()
            .map(|r| {
                json!({
                    "id": r.id.to_string(),
                    "data": r.data,
                    "created_at": r.created_at.to_rfc3339(),
                    "updated_at": r.updated_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(ToolResult::success(json!({
            "records": result,
            "count": result.len(),
            "total": total,
            "collection": collection_name,
            "limit": limit,
            "offset": offset
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── UpdateRecord ─────────────────────────────────────────────────────────

/// Update an existing record (merge semantics — new fields override, existing fields preserved).
pub struct UpdateRecordTool {
    db_pool: PgPool,
    event_tx: Option<mpsc::Sender<TriggerEvent>>,
}

impl UpdateRecordTool {
    pub fn new(db_pool: PgPool, event_tx: Option<mpsc::Sender<TriggerEvent>>) -> Self {
        Self { db_pool, event_tx }
    }
}

#[async_trait]
impl Tool for UpdateRecordTool {
    fn name(&self) -> &str {
        "update_record"
    }

    fn description(&self) -> &str {
        "Update an existing record. Only the provided fields are changed — other fields are preserved. Provide the record ID and the fields to update."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "record_id": {
                    "type": "string",
                    "description": "UUID of the record to update"
                },
                "data": {
                    "type": "object",
                    "description": "Fields to update — only provided fields change, others are preserved. Example: {\"stage\": \"qualified\"}"
                }
            },
            "required": ["record_id", "data"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let record_id_str = params["record_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("record_id is required".to_string()))?;

        let record_id = Uuid::parse_str(record_id_str).map_err(|_| {
            amos_core::AmosError::Validation(format!("Invalid UUID: {}", record_id_str))
        })?;

        let data = params
            .get("data")
            .cloned()
            .ok_or_else(|| amos_core::AmosError::Validation("data is required".to_string()))?;

        let engine = match &self.event_tx {
            Some(tx) => SchemaEngine::with_event_sender(self.db_pool.clone(), tx.clone()),
            None => SchemaEngine::new(self.db_pool.clone()),
        };
        let record = engine.update_record(record_id, data).await?;

        Ok(ToolResult::success(json!({
            "record_id": record.id.to_string(),
            "collection": record.collection_name,
            "data": record.data,
            "updated_at": record.updated_at.to_rfc3339(),
            "message": "Record updated successfully"
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}

// ── DeleteRecord ─────────────────────────────────────────────────────────

/// Delete a record by ID.
pub struct DeleteRecordTool {
    db_pool: PgPool,
    event_tx: Option<mpsc::Sender<TriggerEvent>>,
}

impl DeleteRecordTool {
    pub fn new(db_pool: PgPool, event_tx: Option<mpsc::Sender<TriggerEvent>>) -> Self {
        Self { db_pool, event_tx }
    }
}

#[async_trait]
impl Tool for DeleteRecordTool {
    fn name(&self) -> &str {
        "delete_record"
    }

    fn description(&self) -> &str {
        "Delete a record by its ID. This action is permanent."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "record_id": {
                    "type": "string",
                    "description": "UUID of the record to delete"
                }
            },
            "required": ["record_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let record_id_str = params["record_id"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("record_id is required".to_string()))?;

        let record_id = Uuid::parse_str(record_id_str).map_err(|_| {
            amos_core::AmosError::Validation(format!("Invalid UUID: {}", record_id_str))
        })?;

        let engine = match &self.event_tx {
            Some(tx) => SchemaEngine::with_event_sender(self.db_pool.clone(), tx.clone()),
            None => SchemaEngine::new(self.db_pool.clone()),
        };
        engine.delete_record(record_id).await?;

        Ok(ToolResult::success(json!({
            "deleted": true,
            "record_id": record_id_str,
            "message": "Record deleted successfully"
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Schema
    }
}
