//! Integration management tools for AI agents
//!
//! 6 tools (consolidated from 8):
//! - `query_integrations`: List integrations, connections, or operations in one tool
//! - `create_integration`: Define a new integration type dynamically
//! - `create_connection`: Create a connection with credentials
//! - `test_connection`: Test if a connection works
//! - `execute_integration_action`: Execute an API operation
//! - `sync_integration`: Create sync config or trigger a sync job

use crate::integrations::etl::EtlPipeline;
use crate::integrations::executor::ApiExecutor;
use crate::integrations::types::{IntegrationRow, OperationRow};
use crate::tools::{Tool, ToolCategory, ToolResult};
use amos_core::Result;
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════
// Query Integrations Tool (merged: list_integrations + list_connections + list_operations)
// ═══════════════════════════════════════════════════════════════════════════

pub struct QueryIntegrationsTool {
    db_pool: PgPool,
}

impl QueryIntegrationsTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

/// Helper row type for connection list queries (includes integration name join)
#[derive(Debug, Clone, sqlx::FromRow)]
struct ConnectionWithIntegration {
    id: Uuid,
    integration_id: Uuid,
    credential_id: Option<Uuid>,
    name: Option<String>,
    status: String,
    health: String,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    error_message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    integration_name: String,
}

#[async_trait]
impl Tool for QueryIntegrationsTool {
    fn name(&self) -> &str {
        "query_integrations"
    }

    fn description(&self) -> &str {
        "Query integration data. Use 'resource' to choose what to list: 'integrations' (available types like CRM, email), 'connections' (active connections, optionally by integration_id), or 'operations' (API operations for a specific integration_id)."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "resource": {
                    "type": "string",
                    "enum": ["integrations", "connections", "operations"],
                    "description": "What to query: integrations (types), connections (active links), or operations (available API actions)"
                },
                "integration_id": {
                    "type": "string",
                    "description": "Filter by integration UUID (required for operations, optional for connections)"
                }
            },
            "required": ["resource"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let resource = params["resource"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("resource is required".to_string()))?;

        match resource {
            "integrations" => self.list_integrations().await,
            "connections" => {
                let integration_id = params
                    .get("integration_id")
                    .and_then(|v| v.as_str())
                    .map(Uuid::from_str)
                    .transpose()
                    .map_err(|_| {
                        amos_core::AmosError::Validation(
                            "Invalid integration_id UUID format".to_string(),
                        )
                    })?;
                self.list_connections(integration_id).await
            }
            "operations" => {
                let integration_id = params["integration_id"]
                    .as_str()
                    .ok_or_else(|| {
                        amos_core::AmosError::Validation(
                            "integration_id is required for operations".to_string(),
                        )
                    })
                    .and_then(|s| {
                        Uuid::from_str(s).map_err(|_| {
                            amos_core::AmosError::Validation(
                                "Invalid integration_id UUID".to_string(),
                            )
                        })
                    })?;
                self.list_operations(integration_id).await
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown resource '{}'. Use: integrations, connections, operations",
                resource
            ))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }
}

impl QueryIntegrationsTool {
    async fn list_integrations(&self) -> Result<ToolResult> {
        let integrations: Vec<IntegrationRow> = sqlx::query_as(
            r#"
            SELECT id, name, connector_type, endpoint_url, status,
                   credentials, last_sync_at, error_message, sync_config,
                   available_actions, metadata, created_at, updated_at
            FROM integrations
            ORDER BY name ASC
            LIMIT 200
            "#,
        )
        .fetch_all(&self.db_pool)
        .await?;

        let result: Vec<JsonValue> = integrations
            .iter()
            .map(|i| {
                json!({
                    "id": i.id,
                    "name": i.name,
                    "connector_type": i.connector_type,
                    "endpoint_url": i.endpoint_url,
                    "status": i.status,
                    "last_sync_at": i.last_sync_at,
                    "error_message": i.error_message,
                    "created_at": i.created_at,
                    "updated_at": i.updated_at,
                })
            })
            .collect();

        let count = result.len();
        Ok(ToolResult::success(json!({
            "integrations": result,
            "count": count
        })))
    }

    async fn list_connections(&self, integration_id: Option<Uuid>) -> Result<ToolResult> {
        let connections: Vec<ConnectionWithIntegration> = if let Some(int_id) = integration_id {
            sqlx::query_as(
                r#"
                SELECT c.id, c.integration_id, c.credential_id, c.name, c.status,
                       c.health, c.last_used_at, c.last_sync_at, c.error_message,
                       c.created_at, c.updated_at,
                       i.name as integration_name
                FROM integration_connections c
                JOIN integrations i ON c.integration_id = i.id
                WHERE c.integration_id = $1
                ORDER BY c.created_at DESC
                LIMIT 200
                "#,
            )
            .bind(int_id)
            .fetch_all(&self.db_pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT c.id, c.integration_id, c.credential_id, c.name, c.status,
                       c.health, c.last_used_at, c.last_sync_at, c.error_message,
                       c.created_at, c.updated_at,
                       i.name as integration_name
                FROM integration_connections c
                JOIN integrations i ON c.integration_id = i.id
                ORDER BY c.created_at DESC
                LIMIT 200
                "#,
            )
            .fetch_all(&self.db_pool)
            .await?
        };

        let result: Vec<JsonValue> = connections
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "integration_id": c.integration_id,
                    "integration_name": c.integration_name,
                    "credential_id": c.credential_id,
                    "name": c.name,
                    "status": c.status,
                    "health": c.health,
                    "last_used_at": c.last_used_at,
                    "last_sync_at": c.last_sync_at,
                    "error_message": c.error_message,
                    "created_at": c.created_at,
                    "updated_at": c.updated_at,
                })
            })
            .collect();

        let count = result.len();
        Ok(ToolResult::success(json!({
            "connections": result,
            "count": count
        })))
    }

    async fn list_operations(&self, integration_id: Uuid) -> Result<ToolResult> {
        let operations: Vec<OperationRow> = sqlx::query_as(
            r#"
            SELECT id, integration_id, operation_id, name, description, http_method,
                   path_template, request_schema, response_schema, pagination_strategy,
                   requires_confirmation, is_destructive, status, examples, metadata,
                   created_at, updated_at
            FROM integration_operations
            WHERE integration_id = $1
            ORDER BY name ASC
            LIMIT 500
            "#,
        )
        .bind(integration_id)
        .fetch_all(&self.db_pool)
        .await?;

        let result: Vec<JsonValue> = operations
            .iter()
            .map(|op| {
                json!({
                    "id": op.id,
                    "operation_id": op.operation_id,
                    "name": op.name,
                    "description": op.description,
                    "http_method": op.http_method,
                    "path_template": op.path_template,
                    "request_schema": op.request_schema,
                    "response_schema": op.response_schema,
                    "pagination_strategy": op.pagination_strategy,
                    "requires_confirmation": op.requires_confirmation,
                    "is_destructive": op.is_destructive,
                    "status": op.status,
                })
            })
            .collect();

        let count = result.len();
        Ok(ToolResult::success(json!({
            "operations": result,
            "count": count
        })))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Manage Integration Tool (create, update, delete)
// ═══════════════════════════════════════════════════════════════════════════

/// Create, update, or delete integration definitions
pub struct ManageIntegrationTool {
    db_pool: PgPool,
}

impl ManageIntegrationTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

/// Row returned from integration INSERT/UPDATE
#[derive(Debug, sqlx::FromRow)]
struct IntegrationMutationRow {
    id: Uuid,
    name: String,
    connector_type: String,
    status: String,
    endpoint_url: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
impl Tool for ManageIntegrationTool {
    fn name(&self) -> &str {
        "manage_integration"
    }

    fn description(&self) -> &str {
        "Create, update, or delete integration definitions. Operations: 'create' (define a new integration type like GoDaddy or Mailchimp with optional inline operations), 'update' (change name, endpoint, status, metadata, or add operations), 'delete' (remove integration and all its connections/operations). Use query_integrations to list existing integrations."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "update", "delete"],
                    "description": "Operation to perform"
                },
                "integration_id": {
                    "type": "string",
                    "description": "UUID of the integration (required for update/delete)"
                },
                "name": {
                    "type": "string",
                    "description": "Integration name (e.g. 'GoDaddy', 'Mailchimp', 'Custom CRM')"
                },
                "connector_type": {
                    "type": "string",
                    "description": "Type of connector (required for create)",
                    "enum": ["rest_api", "graphql", "webhook", "database", "custom"]
                },
                "endpoint_url": {
                    "type": "string",
                    "description": "Base API endpoint URL (e.g. 'https://api.godaddy.com/v1')"
                },
                "status": {
                    "type": "string",
                    "description": "Integration status (for update)",
                    "enum": ["active", "disconnected", "error", "disabled"]
                },
                "available_actions": {
                    "type": "array",
                    "description": "List of available action names",
                    "items": { "type": "string" }
                },
                "metadata": {
                    "type": "object",
                    "description": "Additional metadata (auth docs, rate limits, etc.)"
                },
                "operations": {
                    "type": "array",
                    "description": "Define API operations inline. Each: {\"operation_id\": \"list_domains\", \"name\": \"List Domains\", \"http_method\": \"GET\", \"path_template\": \"/domains\"}",
                    "items": {
                        "type": "object",
                        "properties": {
                            "operation_id": { "type": "string" },
                            "name": { "type": "string" },
                            "description": { "type": "string" },
                            "http_method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"] },
                            "path_template": { "type": "string" },
                            "request_schema": { "type": "object" },
                            "response_schema": { "type": "object" },
                            "requires_confirmation": { "type": "boolean" },
                            "is_destructive": { "type": "boolean" }
                        },
                        "required": ["operation_id", "name", "http_method", "path_template"]
                    }
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let operation = params["operation"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("operation is required".to_string()))?;

        match operation {
            "create" => self.create(params).await,
            "update" => self.update(params).await,
            "delete" => self.delete(params).await,
            _ => Ok(ToolResult::error(format!(
                "Unknown operation '{}'. Use: create, update, delete",
                operation
            ))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }
}

impl ManageIntegrationTool {
    async fn create(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("name is required for create".to_string())
        })?;

        let connector_type = params["connector_type"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("connector_type is required for create".to_string())
        })?;

        let endpoint_url = params
            .get("endpoint_url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let available_actions = params
            .get("available_actions")
            .cloned()
            .unwrap_or_else(|| json!([]));

        let metadata = params.get("metadata").cloned().unwrap_or_else(|| json!({}));

        let integration: IntegrationMutationRow = sqlx::query_as(
            r#"
            INSERT INTO integrations
                (name, connector_type, endpoint_url, status, credentials,
                 available_actions, metadata, sync_config)
            VALUES ($1, $2, $3, 'active', '{}', $4, $5, '{}')
            RETURNING id, name, connector_type, status, endpoint_url, created_at, updated_at
            "#,
        )
        .bind(name)
        .bind(connector_type)
        .bind(endpoint_url)
        .bind(&available_actions)
        .bind(&metadata)
        .fetch_one(&self.db_pool)
        .await?;

        let ops_created = self.upsert_operations(integration.id, &params).await;

        Ok(ToolResult::success(json!({
            "integration_id": integration.id,
            "name": integration.name,
            "connector_type": integration.connector_type,
            "endpoint_url": integration.endpoint_url,
            "status": integration.status,
            "operations_created": ops_created,
            "message": format!(
                "Integration '{}' created{}. Next: create a connection with credentials.",
                integration.name,
                if ops_created > 0 { format!(" with {} operations", ops_created) } else { String::new() }
            )
        })))
    }

    async fn update(&self, params: JsonValue) -> Result<ToolResult> {
        let integration_id = params["integration_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation(
                    "integration_id is required for update".to_string(),
                )
            })
            .and_then(|s| {
                Uuid::from_str(s).map_err(|_| {
                    amos_core::AmosError::Validation("Invalid integration_id UUID".to_string())
                })
            })?;

        // Build dynamic SET clause from provided fields
        let mut sets = Vec::new();
        let mut bind_idx = 1u32;
        let mut binds: Vec<String> = Vec::new();

        if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
            sets.push(format!("name = ${}", bind_idx));
            binds.push(name.to_string());
            bind_idx += 1;
        }
        if let Some(endpoint_url) = params.get("endpoint_url").and_then(|v| v.as_str()) {
            sets.push(format!("endpoint_url = ${}", bind_idx));
            binds.push(endpoint_url.to_string());
            bind_idx += 1;
        }
        if let Some(status) = params.get("status").and_then(|v| v.as_str()) {
            sets.push(format!("status = ${}", bind_idx));
            binds.push(status.to_string());
            bind_idx += 1;
        }
        if let Some(connector_type) = params.get("connector_type").and_then(|v| v.as_str()) {
            sets.push(format!("connector_type = ${}", bind_idx));
            binds.push(connector_type.to_string());
            bind_idx += 1;
        }

        // Always bump updated_at
        sets.push("updated_at = NOW()".to_string());

        // Handle JSONB fields via direct queries if present
        if let Some(metadata) = params.get("metadata") {
            sqlx::query("UPDATE integrations SET metadata = $1, updated_at = NOW() WHERE id = $2")
                .bind(metadata)
                .bind(integration_id)
                .execute(&self.db_pool)
                .await?;
        }
        if let Some(available_actions) = params.get("available_actions") {
            sqlx::query(
                "UPDATE integrations SET available_actions = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(available_actions)
            .bind(integration_id)
            .execute(&self.db_pool)
            .await?;
        }

        // Apply string field updates if any
        if !binds.is_empty() {
            let set_clause = sets.join(", ");
            let query = format!(
                "UPDATE integrations SET {} WHERE id = ${}",
                set_clause, bind_idx
            );
            let mut q = sqlx::query(&query);
            for b in &binds {
                q = q.bind(b);
            }
            q = q.bind(integration_id);
            q.execute(&self.db_pool).await?;
        }

        // Add new operations if provided
        let ops_created = self.upsert_operations(integration_id, &params).await;

        // Fetch updated row
        let integration: IntegrationMutationRow = sqlx::query_as(
            "SELECT id, name, connector_type, status, endpoint_url, created_at, updated_at \
             FROM integrations WHERE id = $1",
        )
        .bind(integration_id)
        .fetch_one(&self.db_pool)
        .await?;

        Ok(ToolResult::success(json!({
            "integration_id": integration.id,
            "name": integration.name,
            "connector_type": integration.connector_type,
            "endpoint_url": integration.endpoint_url,
            "status": integration.status,
            "operations_added": ops_created,
            "updated_at": integration.updated_at,
            "message": format!("Integration '{}' updated", integration.name)
        })))
    }

    async fn delete(&self, params: JsonValue) -> Result<ToolResult> {
        let integration_id = params["integration_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation(
                    "integration_id is required for delete".to_string(),
                )
            })
            .and_then(|s| {
                Uuid::from_str(s).map_err(|_| {
                    amos_core::AmosError::Validation("Invalid integration_id UUID".to_string())
                })
            })?;

        // Delete operations, then credentials+connections, then the integration itself
        let ops_deleted =
            sqlx::query("DELETE FROM integration_operations WHERE integration_id = $1")
                .bind(integration_id)
                .execute(&self.db_pool)
                .await
                .map(|r| r.rows_affected())
                .unwrap_or(0);

        // Delete sync configs for connections of this integration
        let _ = sqlx::query(
            "DELETE FROM integration_sync_configs WHERE connection_id IN \
             (SELECT id FROM integration_connections WHERE integration_id = $1)",
        )
        .bind(integration_id)
        .execute(&self.db_pool)
        .await;

        // Delete credentials for connections
        let _ = sqlx::query("DELETE FROM integration_credentials WHERE integration_id = $1")
            .bind(integration_id)
            .execute(&self.db_pool)
            .await;

        // Delete connections
        let conns_deleted =
            sqlx::query("DELETE FROM integration_connections WHERE integration_id = $1")
                .bind(integration_id)
                .execute(&self.db_pool)
                .await
                .map(|r| r.rows_affected())
                .unwrap_or(0);

        // Delete the integration
        let result = sqlx::query("DELETE FROM integrations WHERE id = $1")
            .bind(integration_id)
            .execute(&self.db_pool)
            .await?;

        if result.rows_affected() == 0 {
            return Ok(ToolResult::error(format!(
                "Integration {} not found",
                integration_id
            )));
        }

        Ok(ToolResult::success(json!({
            "deleted": true,
            "integration_id": integration_id.to_string(),
            "operations_deleted": ops_deleted,
            "connections_deleted": conns_deleted,
            "message": "Integration and all associated connections/operations deleted"
        })))
    }

    /// Insert operations from the `operations` array param. Used by both create and update.
    async fn upsert_operations(&self, integration_id: Uuid, params: &JsonValue) -> usize {
        let mut ops_created = 0;
        if let Some(operations) = params.get("operations").and_then(|v| v.as_array()) {
            for op in operations {
                let op_id = op["operation_id"].as_str().unwrap_or("unknown");
                let op_name = op["name"].as_str().unwrap_or(op_id);
                let description = op.get("description").and_then(|v| v.as_str()).unwrap_or("");
                let http_method = op["http_method"].as_str().unwrap_or("GET");
                let path_template = op["path_template"].as_str().unwrap_or("/");
                let request_schema = op.get("request_schema").cloned().unwrap_or(json!({}));
                let response_schema = op.get("response_schema").cloned().unwrap_or(json!({}));
                let requires_confirmation = op
                    .get("requires_confirmation")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let is_destructive = op
                    .get("is_destructive")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let res = sqlx::query(
                    r#"
                    INSERT INTO integration_operations
                        (integration_id, operation_id, name, description, http_method,
                         path_template, request_schema, response_schema,
                         requires_confirmation, is_destructive, status, examples, metadata)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'active', '[]', '{}')
                    ON CONFLICT (integration_id, operation_id) DO UPDATE SET
                        name = EXCLUDED.name,
                        description = EXCLUDED.description,
                        http_method = EXCLUDED.http_method,
                        path_template = EXCLUDED.path_template,
                        request_schema = EXCLUDED.request_schema,
                        response_schema = EXCLUDED.response_schema,
                        requires_confirmation = EXCLUDED.requires_confirmation,
                        is_destructive = EXCLUDED.is_destructive,
                        updated_at = NOW()
                    "#,
                )
                .bind(integration_id)
                .bind(op_id)
                .bind(op_name)
                .bind(description)
                .bind(http_method)
                .bind(path_template)
                .bind(&request_schema)
                .bind(&response_schema)
                .bind(requires_confirmation)
                .bind(is_destructive)
                .execute(&self.db_pool)
                .await;

                if res.is_ok() {
                    ops_created += 1;
                }
            }
        }
        ops_created
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Create Connection Tool
// ═══════════════════════════════════════════════════════════════════════════

/// Creates a new integration connection with credentials
pub struct CreateConnectionTool {
    db_pool: PgPool,
}

impl CreateConnectionTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

/// Minimal row returned from credential INSERT
#[derive(Debug, sqlx::FromRow)]
struct CredentialIdRow {
    id: Uuid,
}

/// Minimal row returned from connection INSERT
#[derive(Debug, sqlx::FromRow)]
struct NewConnectionRow {
    id: Uuid,
    integration_id: Uuid,
    credential_id: Option<Uuid>,
    name: Option<String>,
    status: String,
    health: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
impl Tool for CreateConnectionTool {
    fn name(&self) -> &str {
        "create_connection"
    }

    fn description(&self) -> &str {
        "Create a new integration connection with authentication credentials"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "integration_id": {
                    "type": "string",
                    "description": "UUID of the integration to connect to"
                },
                "auth_type": {
                    "type": "string",
                    "description": "Authentication type",
                    "enum": ["api_key", "bearer_token", "basic_auth", "oauth2", "sso_key", "no_auth", "custom"]
                },
                "credentials": {
                    "type": "object",
                    "description": "Credentials data (e.g., {\"api_key\": \"sk_123\"} or {\"username\": \"user\", \"password\": \"pass\"}). Not required if vault_credential_id is provided."
                },
                "vault_credential_id": {
                    "type": "string",
                    "description": "UUID of a credential stored in the encrypted vault (from collect_credential tool). Use instead of plaintext credentials."
                },
                "name": {
                    "type": "string",
                    "description": "Friendly name for this connection"
                },
                "config": {
                    "type": "object",
                    "description": "Connection-specific configuration settings"
                }
            },
            "required": ["integration_id", "auth_type"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let integration_id = params["integration_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation("integration_id is required".to_string())
            })
            .and_then(|s| {
                Uuid::from_str(s).map_err(|_| {
                    amos_core::AmosError::Validation("Invalid integration_id UUID".to_string())
                })
            })?;

        let auth_type = params["auth_type"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("auth_type is required".to_string()))?
            .to_string();

        let vault_credential_id = params
            .get("vault_credential_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let credentials = if let Some(ref vault_id) = vault_credential_id {
            Uuid::from_str(vault_id).map_err(|_| {
                amos_core::AmosError::Validation("Invalid vault_credential_id UUID".to_string())
            })?;
            json!({ "vault_credential_id": vault_id })
        } else {
            params
                .get("credentials")
                .ok_or_else(|| {
                    amos_core::AmosError::Validation(
                        "Either credentials or vault_credential_id is required".to_string(),
                    )
                })?
                .clone()
        };

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let config = params.get("config").cloned().unwrap_or_else(|| json!({}));

        let credential: CredentialIdRow = sqlx::query_as(
            r#"
            INSERT INTO integration_credentials
                (integration_id, auth_type, credentials_data, status, metadata)
            VALUES ($1, $2, $3, 'active', '{}')
            RETURNING id
            "#,
        )
        .bind(integration_id)
        .bind(&auth_type)
        .bind(&credentials)
        .fetch_one(&self.db_pool)
        .await?;

        let connection: NewConnectionRow = sqlx::query_as(
            r#"
            INSERT INTO integration_connections
                (integration_id, credential_id, name, status, health, config, metadata)
            VALUES ($1, $2, $3, 'disconnected', 'unknown', $4, '{}')
            RETURNING id, integration_id, credential_id, name, status, health,
                      created_at, updated_at
            "#,
        )
        .bind(integration_id)
        .bind(credential.id)
        .bind(&name)
        .bind(&config)
        .fetch_one(&self.db_pool)
        .await?;

        Ok(ToolResult::success(json!({
            "connection_id": connection.id,
            "integration_id": connection.integration_id,
            "credential_id": connection.credential_id,
            "name": connection.name,
            "status": connection.status,
            "health": connection.health,
            "created_at": connection.created_at,
            "updated_at": connection.updated_at,
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Connection Tool
// ═══════════════════════════════════════════════════════════════════════════

pub struct TestConnectionTool {
    db_pool: PgPool,
    api_executor: Arc<ApiExecutor>,
}

impl TestConnectionTool {
    pub fn new(db_pool: PgPool, api_executor: Arc<ApiExecutor>) -> Self {
        Self {
            db_pool,
            api_executor,
        }
    }
}

#[async_trait]
impl Tool for TestConnectionTool {
    fn name(&self) -> &str {
        "test_connection"
    }

    fn description(&self) -> &str {
        "Test if an integration connection is working by executing a test API call"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "connection_id": {
                    "type": "string",
                    "description": "UUID of the connection to test"
                }
            },
            "required": ["connection_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let connection_id = params["connection_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation("connection_id is required".to_string())
            })
            .and_then(|s| {
                Uuid::from_str(s).map_err(|_| {
                    amos_core::AmosError::Validation("Invalid connection_id UUID".to_string())
                })
            })?;

        match self.api_executor.test_connection(connection_id).await {
            Ok(result) => {
                sqlx::query(
                    r#"
                    UPDATE integration_connections
                    SET status = 'connected',
                        health = 'healthy',
                        last_used_at = NOW(),
                        error_message = NULL,
                        consecutive_errors = 0
                    WHERE id = $1
                    "#,
                )
                .bind(connection_id)
                .execute(&self.db_pool)
                .await?;

                Ok(ToolResult::success(json!({
                    "success": true,
                    "status_code": result.status_code,
                    "duration_ms": result.duration_ms,
                    "message": "Connection test successful"
                })))
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                sqlx::query(
                    r#"
                    UPDATE integration_connections
                    SET status = 'error',
                        health = 'failing',
                        error_message = $2,
                        consecutive_errors = consecutive_errors + 1
                    WHERE id = $1
                    "#,
                )
                .bind(connection_id)
                .bind(&error_msg)
                .execute(&self.db_pool)
                .await?;

                Ok(ToolResult::error(format!("Connection test failed: {}", e)))
            }
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Execute Integration Action Tool
// ═══════════════════════════════════════════════════════════════════════════

pub struct ExecuteIntegrationActionTool {
    api_executor: Arc<ApiExecutor>,
}

impl ExecuteIntegrationActionTool {
    pub fn new(api_executor: Arc<ApiExecutor>) -> Self {
        Self { api_executor }
    }
}

#[async_trait]
impl Tool for ExecuteIntegrationActionTool {
    fn name(&self) -> &str {
        "execute_integration_action"
    }

    fn description(&self) -> &str {
        "Execute an API operation on an integration (e.g., create_contact, send_email, fetch_invoices)"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "connection_id": {
                    "type": "string",
                    "description": "UUID of the connection to use"
                },
                "operation_id": {
                    "type": "string",
                    "description": "ID of the operation to execute (e.g., 'create_contact', 'send_email')"
                },
                "params": {
                    "type": "object",
                    "description": "Parameters for the operation"
                }
            },
            "required": ["connection_id", "operation_id"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let connection_id = params["connection_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation("connection_id is required".to_string())
            })
            .and_then(|s| {
                Uuid::from_str(s).map_err(|_| {
                    amos_core::AmosError::Validation("Invalid connection_id UUID".to_string())
                })
            })?;

        let operation_id = params["operation_id"].as_str().ok_or_else(|| {
            amos_core::AmosError::Validation("operation_id is required".to_string())
        })?;

        let operation_params = params.get("params").cloned().unwrap_or_else(|| json!({}));

        match self
            .api_executor
            .execute(connection_id, operation_id, operation_params)
            .await
        {
            Ok(result) => Ok(ToolResult::success(json!({
                "success": true,
                "status_code": result.status_code,
                "body": result.body,
                "duration_ms": result.duration_ms,
                "operation_id": result.operation_id,
            }))),
            Err(e) => Ok(ToolResult::error(format!("Operation failed: {}", e))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Sync Integration Tool (merged: create_sync_config + trigger_sync)
// ═══════════════════════════════════════════════════════════════════════════

pub struct SyncIntegrationTool {
    db_pool: PgPool,
    etl_pipeline: Arc<EtlPipeline>,
}

impl SyncIntegrationTool {
    pub fn new(db_pool: PgPool, etl_pipeline: Arc<EtlPipeline>) -> Self {
        Self {
            db_pool,
            etl_pipeline,
        }
    }
}

/// Minimal row returned from sync config INSERT
#[derive(Debug, sqlx::FromRow)]
struct NewSyncConfigRow {
    id: Uuid,
    connection_id: Uuid,
    resource_type: String,
    target_collection: String,
    sync_mode: String,
    sync_direction: String,
    schedule_type: String,
    enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
impl Tool for SyncIntegrationTool {
    fn name(&self) -> &str {
        "sync_integration"
    }

    fn description(&self) -> &str {
        "Create an ETL sync configuration or trigger an existing sync. Use operation 'create' to set up data sync from an integration into a collection, or 'trigger' to manually run an existing sync config."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "trigger"],
                    "description": "Operation: 'create' a new sync config, or 'trigger' an existing one"
                },
                "sync_config_id": {
                    "type": "string",
                    "description": "UUID of existing sync config (required for trigger)"
                },
                "connection_id": {
                    "type": "string",
                    "description": "UUID of the connection to sync from (required for create)"
                },
                "resource_type": {
                    "type": "string",
                    "description": "Type of resource to sync (e.g., 'contacts', 'invoices')"
                },
                "target_collection": {
                    "type": "string",
                    "description": "Collection name to store synced data"
                },
                "fetch_operation_id": {
                    "type": "string",
                    "description": "Operation ID for fetching data (e.g., 'list_contacts')"
                },
                "field_mappings": {
                    "type": "object",
                    "description": "Map external fields to collection fields (e.g., {\"email\": \"contact_email\"})"
                },
                "sync_mode": {
                    "type": "string",
                    "enum": ["full", "incremental"],
                    "description": "Sync mode (default: 'full')"
                },
                "sync_direction": {
                    "type": "string",
                    "enum": ["inbound", "outbound", "bidirectional"],
                    "description": "Sync direction (default: 'inbound')"
                },
                "schedule_type": {
                    "type": "string",
                    "enum": ["manual", "scheduled", "realtime"],
                    "description": "Schedule type (default: 'manual')"
                },
                "schedule_cron": {
                    "type": "string",
                    "description": "Cron expression for scheduled syncs (e.g., '0 */6 * * *')"
                },
                "conflict_resolution": {
                    "type": "string",
                    "enum": ["external_wins", "internal_wins", "manual", "newest"],
                    "description": "Conflict resolution strategy (default: 'external_wins')"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let operation = params["operation"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("operation is required".to_string()))?;

        match operation {
            "create" => self.create_config(params).await,
            "trigger" => self.trigger_sync(params).await,
            _ => Ok(ToolResult::error(format!(
                "Unknown operation '{}'. Use: create, trigger",
                operation
            ))),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Integration
    }
}

impl SyncIntegrationTool {
    async fn create_config(&self, params: JsonValue) -> Result<ToolResult> {
        let connection_id = params["connection_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation("connection_id is required for create".to_string())
            })
            .and_then(|s| {
                Uuid::from_str(s).map_err(|_| {
                    amos_core::AmosError::Validation("Invalid connection_id UUID".to_string())
                })
            })?;

        let resource_type = params["resource_type"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation("resource_type is required".to_string())
            })?
            .to_string();

        let target_collection = params["target_collection"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation("target_collection is required".to_string())
            })?
            .to_string();

        let fetch_operation_id = params["fetch_operation_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation("fetch_operation_id is required".to_string())
            })?
            .to_string();

        let field_mappings = params
            .get("field_mappings")
            .ok_or_else(|| {
                amos_core::AmosError::Validation("field_mappings is required".to_string())
            })?
            .clone();

        let sync_mode = params
            .get("sync_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("full")
            .to_string();

        let sync_direction = params
            .get("sync_direction")
            .and_then(|v| v.as_str())
            .unwrap_or("inbound")
            .to_string();

        let schedule_type = params
            .get("schedule_type")
            .and_then(|v| v.as_str())
            .unwrap_or("manual")
            .to_string();

        let schedule_cron: Option<String> = params
            .get("schedule_cron")
            .and_then(|v| v.as_str())
            .map(String::from);

        let conflict_resolution = params
            .get("conflict_resolution")
            .and_then(|v| v.as_str())
            .unwrap_or("external_wins")
            .to_string();

        let empty_json = json!({});

        let sync_config: NewSyncConfigRow = sqlx::query_as(
            r#"
            INSERT INTO integration_sync_configs
                (connection_id, resource_type, target_collection, sync_mode, sync_direction,
                 field_mappings, conflict_resolution, schedule_type, schedule_cron,
                 fetch_operation_id, fetch_params, enabled, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, true, $12)
            RETURNING id, connection_id, resource_type, target_collection, sync_mode,
                      sync_direction, schedule_type, enabled, created_at, updated_at
            "#,
        )
        .bind(connection_id)
        .bind(&resource_type)
        .bind(&target_collection)
        .bind(&sync_mode)
        .bind(&sync_direction)
        .bind(&field_mappings)
        .bind(&conflict_resolution)
        .bind(&schedule_type)
        .bind(&schedule_cron)
        .bind(&fetch_operation_id)
        .bind(&empty_json)
        .bind(&empty_json)
        .fetch_one(&self.db_pool)
        .await?;

        Ok(ToolResult::success(json!({
            "sync_config_id": sync_config.id,
            "connection_id": sync_config.connection_id,
            "resource_type": sync_config.resource_type,
            "target_collection": sync_config.target_collection,
            "sync_mode": sync_config.sync_mode,
            "sync_direction": sync_config.sync_direction,
            "schedule_type": sync_config.schedule_type,
            "enabled": sync_config.enabled,
            "created_at": sync_config.created_at,
            "updated_at": sync_config.updated_at,
        })))
    }

    async fn trigger_sync(&self, params: JsonValue) -> Result<ToolResult> {
        let sync_config_id = params["sync_config_id"]
            .as_str()
            .ok_or_else(|| {
                amos_core::AmosError::Validation(
                    "sync_config_id is required for trigger".to_string(),
                )
            })
            .and_then(|s| {
                Uuid::from_str(s).map_err(|_| {
                    amos_core::AmosError::Validation("Invalid sync_config_id UUID".to_string())
                })
            })?;

        match self.etl_pipeline.run(sync_config_id).await {
            Ok(result) => Ok(ToolResult::success(json!({
                "success": result.status == "success" || result.status == "partial",
                "status": result.status,
                "extracted": result.extracted,
                "transformed": result.transformed,
                "loaded": result.loaded,
                "duration_ms": result.duration_ms,
                "errors": result.errors,
            }))),
            Err(e) => Ok(ToolResult::error(format!("Sync failed: {}", e))),
        }
    }
}
