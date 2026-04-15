//! Dynamic Schema System
//!
//! Runtime-defined collections and records. The AI agent creates collections
//! (analogous to database tables) and manages records (rows) through tool calls.
//! No migrations needed per customer request — schema is data, not DDL.
//!
//! ## Design
//!
//! - **Collections** define the shape of data: field names, types, constraints
//! - **Records** store actual data as JSONB, validated against their collection's fields
//! - Both are queried via PostgreSQL JSONB operators and GIN indexes

use crate::automations::{TriggerEvent, TriggerType};
use amos_core::{AmosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use tokio::sync::mpsc;
use uuid::Uuid;

// ── Types ────────────────────────────────────────────────────────────────

/// A collection defines the structure for a set of records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub fields: Vec<FieldDefinition>,
    pub settings: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Definition of a single field within a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Machine-readable name (snake_case slug)
    pub name: String,
    /// Human-readable label
    pub display_name: String,
    /// Data type
    pub field_type: FieldType,
    /// Whether the field must be present and non-null
    #[serde(default)]
    pub required: bool,
    /// Whether values must be unique across records
    #[serde(default)]
    pub unique: bool,
    /// Default value if not provided
    pub default_value: Option<JsonValue>,
    /// What this field is for
    pub description: Option<String>,
    /// Type-specific options:
    /// - enum: `{"choices": ["a", "b", "c"]}`
    /// - reference: `{"collection": "other_collection_name"}`
    /// - number/decimal: `{"min": 0, "max": 100}`
    #[serde(default = "default_options")]
    pub options: JsonValue,
}

fn default_options() -> JsonValue {
    json!({})
}

/// Supported field types for collection fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    RichText,
    Number,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Enum,
    Reference,
    Email,
    Url,
    Phone,
    Json,
}

/// A single data record within a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub collection_name: String,
    pub data: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Schema Engine ────────────────────────────────────────────────────────

/// Engine for CRUD operations on collections and records.
pub struct SchemaEngine {
    db_pool: PgPool,
    event_tx: Option<mpsc::Sender<TriggerEvent>>,
}

impl SchemaEngine {
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            event_tx: None,
        }
    }

    pub fn with_event_sender(db_pool: PgPool, event_tx: mpsc::Sender<TriggerEvent>) -> Self {
        Self {
            db_pool,
            event_tx: Some(event_tx),
        }
    }

    // ─── Collection operations ───────────────────────────────────────

    /// Create or update a collection definition (upsert by name).
    pub async fn define_collection(
        &self,
        name: &str,
        display_name: &str,
        description: Option<&str>,
        fields: Vec<FieldDefinition>,
    ) -> Result<Collection> {
        let fields_json = serde_json::to_value(&fields)
            .map_err(|e| AmosError::Internal(format!("Failed to serialize fields: {}", e)))?;

        let row = sqlx::query(
            r#"
            INSERT INTO collections (name, display_name, description, fields)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (name) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                description = COALESCE(EXCLUDED.description, collections.description),
                fields = EXCLUDED.fields,
                updated_at = NOW()
            RETURNING id, name, display_name, description, fields, settings, created_at, updated_at
            "#,
        )
        .bind(name)
        .bind(display_name)
        .bind(description)
        .bind(&fields_json)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to define collection: {}", e)))?;

        collection_from_row(&row)
    }

    /// Get a collection by its slug name.
    pub async fn get_collection(&self, name: &str) -> Result<Collection> {
        let row = sqlx::query(
            r#"SELECT id, name, display_name, description, fields, settings, created_at, updated_at
               FROM collections WHERE name = $1"#,
        )
        .bind(name)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to get collection: {}", e)))?
        .ok_or_else(|| AmosError::NotFound {
            entity: "Collection".to_string(),
            id: name.to_string(),
        })?;

        collection_from_row(&row)
    }

    /// List all collections.
    pub async fn list_collections(&self) -> Result<Vec<Collection>> {
        let rows = sqlx::query(
            r#"SELECT id, name, display_name, description, fields, settings, created_at, updated_at
               FROM collections ORDER BY name"#,
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to list collections: {}", e)))?;

        rows.iter().map(collection_from_row).collect()
    }

    /// Delete a collection and all its records.
    pub async fn delete_collection(&self, name: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM collections WHERE name = $1")
            .bind(name)
            .execute(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to delete collection: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AmosError::NotFound {
                entity: "Collection".to_string(),
                id: name.to_string(),
            });
        }

        Ok(())
    }

    // ─── Record operations ───────────────────────────────────────────

    /// Create a new record in a collection. Validates data against the collection's field defs.
    pub async fn create_record(
        &self,
        collection_name: &str,
        mut data: JsonValue,
    ) -> Result<Record> {
        let collection = self.get_collection(collection_name).await?;

        // Apply defaults for missing fields
        if let Some(obj) = data.as_object_mut() {
            for field in &collection.fields {
                if !obj.contains_key(&field.name) {
                    if let Some(default) = &field.default_value {
                        obj.insert(field.name.clone(), default.clone());
                    }
                }
            }
        }

        // Validate against field definitions
        validate_record_data(&collection.fields, &data)?;

        let row = sqlx::query(
            r#"INSERT INTO records (collection_id, data)
               VALUES ($1, $2)
               RETURNING id, collection_id, data, created_at, updated_at"#,
        )
        .bind(collection.id)
        .bind(&data)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to create record: {}", e)))?;

        let record = record_from_row(&row, collection_name)?;

        // Fire automation event (non-blocking channel send, drops if channel full)
        if let Some(tx) = &self.event_tx {
            let _ = tx.try_send(TriggerEvent {
                event_type: TriggerType::RecordCreated,
                collection: Some(collection_name.to_string()),
                record_id: Some(record.id),
                data: record.data.clone(),
            });
        }

        Ok(record)
    }

    /// Query records from a collection with JSONB containment filters.
    ///
    /// Filters use equality matching via PostgreSQL `@>` operator:
    /// `{"status": "active", "priority": "high"}` matches records containing those exact values.
    pub async fn query_records(
        &self,
        collection_name: &str,
        filters: Option<JsonValue>,
        sort_by: Option<&str>,
        sort_dir: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Record>, i64)> {
        let collection = self.get_collection(collection_name).await?;
        let filter_json = filters.unwrap_or_else(|| json!({}));

        // Count total matching
        let count_row = sqlx::query(
            "SELECT COUNT(*) as total FROM records WHERE collection_id = $1 AND data @> $2::jsonb",
        )
        .bind(collection.id)
        .bind(&filter_json)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to count records: {}", e)))?;

        let total: i64 = count_row.get("total");

        // Build ORDER BY — support created_at, updated_at, or validated JSONB field names.
        // Allowlist: only ASCII alphanumeric + underscore, max 64 chars.
        let order_expr = match sort_by {
            Some("updated_at") => "r.updated_at".to_string(),
            Some("created_at") | None => "r.created_at".to_string(),
            Some(field_name)
                if field_name.len() <= 64
                    && field_name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                format!("r.data->>'{}'", field_name)
            }
            Some(_) => "r.created_at".to_string(),
        };

        let dir = match sort_dir {
            Some(d) if d.eq_ignore_ascii_case("asc") => "ASC",
            _ => "DESC",
        };

        let query = format!(
            r#"SELECT r.id, r.collection_id, r.data, r.created_at, r.updated_at
               FROM records r
               WHERE r.collection_id = $1 AND r.data @> $2::jsonb
               ORDER BY {} {}
               LIMIT $3 OFFSET $4"#,
            order_expr, dir
        );

        let rows = sqlx::query(&query)
            .bind(collection.id)
            .bind(&filter_json)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to query records: {}", e)))?;

        let records: Vec<Record> = rows
            .iter()
            .map(|row| record_from_row(row, collection_name))
            .collect::<Result<Vec<_>>>()?;

        Ok((records, total))
    }

    /// Get a single record by ID.
    pub async fn get_record(&self, record_id: Uuid) -> Result<Record> {
        let row = sqlx::query(
            r#"SELECT r.id, r.collection_id, r.data, r.created_at, r.updated_at, c.name as collection_name
               FROM records r JOIN collections c ON r.collection_id = c.id
               WHERE r.id = $1"#,
        )
        .bind(record_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to get record: {}", e)))?
        .ok_or_else(|| AmosError::NotFound {
            entity: "Record".to_string(),
            id: record_id.to_string(),
        })?;

        let collection_name: String = row.get("collection_name");
        record_from_row(&row, &collection_name)
    }

    /// Update a record by merging new data (new keys win on conflict).
    pub async fn update_record(&self, record_id: Uuid, data: JsonValue) -> Result<Record> {
        // Look up the record + collection name in one query
        let existing = sqlx::query(
            r#"SELECT r.id, c.name as collection_name
               FROM records r JOIN collections c ON r.collection_id = c.id
               WHERE r.id = $1"#,
        )
        .bind(record_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to find record: {}", e)))?
        .ok_or_else(|| AmosError::NotFound {
            entity: "Record".to_string(),
            id: record_id.to_string(),
        })?;

        let collection_name: String = existing.get("collection_name");

        // Merge using JSONB || operator (right side wins on key conflicts)
        let row = sqlx::query(
            r#"UPDATE records SET data = data || $1::jsonb, updated_at = NOW()
               WHERE id = $2
               RETURNING id, collection_id, data, created_at, updated_at"#,
        )
        .bind(&data)
        .bind(record_id)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AmosError::Internal(format!("Failed to update record: {}", e)))?;

        let record = record_from_row(&row, &collection_name)?;

        // Fire automation event (non-blocking channel send, drops if channel full)
        if let Some(tx) = &self.event_tx {
            let _ = tx.try_send(TriggerEvent {
                event_type: TriggerType::RecordUpdated,
                collection: Some(collection_name.clone()),
                record_id: Some(record.id),
                data: record.data.clone(),
            });
        }

        Ok(record)
    }

    /// Delete a record by ID.
    pub async fn delete_record(&self, record_id: Uuid) -> Result<()> {
        // Fetch record info before deleting (for automation event)
        let record_info = if self.event_tx.is_some() {
            sqlx::query(
                r#"SELECT r.data, c.name as collection_name
                   FROM records r JOIN collections c ON r.collection_id = c.id
                   WHERE r.id = $1"#,
            )
            .bind(record_id)
            .fetch_optional(&self.db_pool)
            .await
            .ok()
            .flatten()
            .map(|row| {
                let data: JsonValue = row.get("data");
                let collection_name: String = row.get("collection_name");
                (collection_name, data)
            })
        } else {
            None
        };

        let result = sqlx::query("DELETE FROM records WHERE id = $1")
            .bind(record_id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| AmosError::Internal(format!("Failed to delete record: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AmosError::NotFound {
                entity: "Record".to_string(),
                id: record_id.to_string(),
            });
        }

        // Fire automation event (non-blocking channel send, drops if channel full)
        if let (Some(tx), Some((collection_name, data))) = (&self.event_tx, record_info) {
            let _ = tx.try_send(TriggerEvent {
                event_type: TriggerType::RecordDeleted,
                collection: Some(collection_name),
                record_id: Some(record_id),
                data,
            });
        }

        Ok(())
    }
}

// ── Validation ───────────────────────────────────────────────────────────

/// Validate record data against a collection's field definitions.
fn validate_record_data(fields: &[FieldDefinition], data: &JsonValue) -> Result<()> {
    let obj = data
        .as_object()
        .ok_or_else(|| AmosError::Validation("Record data must be a JSON object".to_string()))?;

    for field in fields {
        let value = obj.get(&field.name);

        // Required check
        if field.required && (value.is_none() || value.is_some_and(|v| v.is_null())) {
            return Err(AmosError::Validation(format!(
                "Field '{}' is required",
                field.display_name
            )));
        }

        // Type check (skip nulls and missing optional fields)
        if let Some(val) = value {
            if !val.is_null() {
                validate_field_value(
                    &field.name,
                    &field.display_name,
                    &field.field_type,
                    val,
                    &field.options,
                )?;
            }
        }
    }

    Ok(())
}

/// Validate a single field's value against its declared type.
fn validate_field_value(
    _name: &str,
    display_name: &str,
    field_type: &FieldType,
    value: &JsonValue,
    options: &JsonValue,
) -> Result<()> {
    match field_type {
        FieldType::Text
        | FieldType::RichText
        | FieldType::Email
        | FieldType::Url
        | FieldType::Phone => {
            if !value.is_string() {
                return Err(AmosError::Validation(format!(
                    "'{}' must be a string",
                    display_name
                )));
            }
        }
        FieldType::Number => {
            if !value.is_i64() && !value.is_u64() {
                return Err(AmosError::Validation(format!(
                    "'{}' must be an integer",
                    display_name
                )));
            }
        }
        FieldType::Decimal => {
            if !value.is_number() {
                return Err(AmosError::Validation(format!(
                    "'{}' must be a number",
                    display_name
                )));
            }
        }
        FieldType::Boolean => {
            if !value.is_boolean() {
                return Err(AmosError::Validation(format!(
                    "'{}' must be a boolean",
                    display_name
                )));
            }
        }
        FieldType::Date | FieldType::DateTime => {
            if !value.is_string() {
                return Err(AmosError::Validation(format!(
                    "'{}' must be a date string (ISO 8601)",
                    display_name
                )));
            }
        }
        FieldType::Enum => {
            let val_str = value.as_str().ok_or_else(|| {
                AmosError::Validation(format!("'{}' must be a string", display_name))
            })?;
            if let Some(choices) = options.get("choices").and_then(|c| c.as_array()) {
                let valid = choices.iter().any(|c| c.as_str() == Some(val_str));
                if !valid {
                    let valid_choices: Vec<&str> =
                        choices.iter().filter_map(|c| c.as_str()).collect();
                    return Err(AmosError::Validation(format!(
                        "'{}' must be one of: {}",
                        display_name,
                        valid_choices.join(", ")
                    )));
                }
            }
        }
        FieldType::Reference => {
            let s = value.as_str().ok_or_else(|| {
                AmosError::Validation(format!("'{}' must be a UUID string", display_name))
            })?;
            if Uuid::parse_str(s).is_err() {
                return Err(AmosError::Validation(format!(
                    "'{}' must be a valid UUID",
                    display_name
                )));
            }
        }
        FieldType::Json => {
            // Any JSON value is valid
        }
    }

    Ok(())
}

// ── Row helpers ──────────────────────────────────────────────────────────

fn collection_from_row(row: &sqlx::postgres::PgRow) -> Result<Collection> {
    let fields_json: JsonValue = row.get("fields");
    let fields: Vec<FieldDefinition> = serde_json::from_value(fields_json)
        .map_err(|e| AmosError::Internal(format!("Failed to parse field definitions: {}", e)))?;

    Ok(Collection {
        id: row.get("id"),
        name: row.get("name"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        fields,
        settings: row.get("settings"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn record_from_row(row: &sqlx::postgres::PgRow, collection_name: &str) -> Result<Record> {
    Ok(Record {
        id: row.get("id"),
        collection_id: row.get("collection_id"),
        collection_name: collection_name.to_string(),
        data: row.get("data"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text_field(name: &str, required: bool) -> FieldDefinition {
        FieldDefinition {
            name: name.to_string(),
            display_name: name.to_string(),
            field_type: FieldType::Text,
            required,
            unique: false,
            default_value: None,
            description: None,
            options: json!({}),
        }
    }

    fn field(name: &str, ft: FieldType, required: bool) -> FieldDefinition {
        FieldDefinition {
            name: name.to_string(),
            display_name: name.to_string(),
            field_type: ft,
            required,
            unique: false,
            default_value: None,
            description: None,
            options: json!({}),
        }
    }

    // ── validate_record_data ─────────────────────────────────────────

    #[test]
    fn valid_record_passes_validation() {
        let fields = vec![text_field("name", true), text_field("email", false)];
        let data = json!({"name": "Alice", "email": "alice@example.com"});
        assert!(validate_record_data(&fields, &data).is_ok());
    }

    #[test]
    fn missing_required_field_fails() {
        let fields = vec![text_field("name", true)];
        let data = json!({});
        let err = validate_record_data(&fields, &data).unwrap_err();
        assert!(err.to_string().contains("required"));
    }

    #[test]
    fn null_required_field_fails() {
        let fields = vec![text_field("name", true)];
        let data = json!({"name": null});
        let err = validate_record_data(&fields, &data).unwrap_err();
        assert!(err.to_string().contains("required"));
    }

    #[test]
    fn missing_optional_field_passes() {
        let fields = vec![text_field("name", true), text_field("bio", false)];
        let data = json!({"name": "Alice"});
        assert!(validate_record_data(&fields, &data).is_ok());
    }

    #[test]
    fn non_object_data_fails() {
        let fields = vec![text_field("name", true)];
        let data = json!("not an object");
        let err = validate_record_data(&fields, &data).unwrap_err();
        assert!(err.to_string().contains("JSON object"));
    }

    // ── validate_field_value type checks ─────────────────────────────

    #[test]
    fn text_field_rejects_number() {
        let err = validate_field_value("f", "Field", &FieldType::Text, &json!(42), &json!({}));
        assert!(err.is_err());
    }

    #[test]
    fn text_field_accepts_string() {
        assert!(
            validate_field_value("f", "Field", &FieldType::Text, &json!("hello"), &json!({}))
                .is_ok()
        );
    }

    #[test]
    fn number_field_rejects_string() {
        let err = validate_field_value("f", "Field", &FieldType::Number, &json!("abc"), &json!({}));
        assert!(err.is_err());
    }

    #[test]
    fn number_field_accepts_integer() {
        assert!(
            validate_field_value("f", "Field", &FieldType::Number, &json!(42), &json!({})).is_ok()
        );
    }

    #[test]
    fn decimal_field_accepts_float() {
        assert!(
            validate_field_value("f", "Field", &FieldType::Decimal, &json!(3.14), &json!({}))
                .is_ok()
        );
    }

    #[test]
    fn decimal_field_accepts_integer() {
        assert!(
            validate_field_value("f", "Field", &FieldType::Decimal, &json!(42), &json!({})).is_ok()
        );
    }

    #[test]
    fn boolean_field_rejects_string() {
        let err = validate_field_value(
            "f",
            "Field",
            &FieldType::Boolean,
            &json!("true"),
            &json!({}),
        );
        assert!(err.is_err());
    }

    #[test]
    fn boolean_field_accepts_bool() {
        assert!(
            validate_field_value("f", "Field", &FieldType::Boolean, &json!(true), &json!({}))
                .is_ok()
        );
    }

    #[test]
    fn email_field_rejects_number() {
        let err = validate_field_value("f", "Field", &FieldType::Email, &json!(42), &json!({}));
        assert!(err.is_err());
    }

    #[test]
    fn json_field_accepts_anything() {
        assert!(
            validate_field_value("f", "Field", &FieldType::Json, &json!({"a": 1}), &json!({}))
                .is_ok()
        );
        assert!(
            validate_field_value("f", "Field", &FieldType::Json, &json!([1, 2]), &json!({}))
                .is_ok()
        );
        assert!(
            validate_field_value("f", "Field", &FieldType::Json, &json!("str"), &json!({})).is_ok()
        );
    }

    // ── Enum validation ──────────────────────────────────────────────

    #[test]
    fn enum_accepts_valid_choice() {
        let options = json!({"choices": ["active", "inactive", "pending"]});
        assert!(
            validate_field_value("f", "Status", &FieldType::Enum, &json!("active"), &options)
                .is_ok()
        );
    }

    #[test]
    fn enum_rejects_invalid_choice() {
        let options = json!({"choices": ["active", "inactive"]});
        let err =
            validate_field_value("f", "Status", &FieldType::Enum, &json!("deleted"), &options);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("must be one of"));
    }

    #[test]
    fn enum_rejects_non_string() {
        let options = json!({"choices": ["a", "b"]});
        let err = validate_field_value("f", "Status", &FieldType::Enum, &json!(42), &options);
        assert!(err.is_err());
    }

    // ── Reference validation ─────────────────────────────────────────

    #[test]
    fn reference_accepts_valid_uuid() {
        let uuid = Uuid::new_v4().to_string();
        assert!(
            validate_field_value("f", "Ref", &FieldType::Reference, &json!(uuid), &json!({}))
                .is_ok()
        );
    }

    #[test]
    fn reference_rejects_invalid_uuid() {
        let err = validate_field_value(
            "f",
            "Ref",
            &FieldType::Reference,
            &json!("not-a-uuid"),
            &json!({}),
        );
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("valid UUID"));
    }

    #[test]
    fn reference_rejects_non_string() {
        let err = validate_field_value("f", "Ref", &FieldType::Reference, &json!(123), &json!({}));
        assert!(err.is_err());
    }

    // ── Combined record validation ───────────────────────────────────

    #[test]
    fn mixed_field_types_validated_together() {
        let fields = vec![
            text_field("name", true),
            field("age", FieldType::Number, true),
            field("active", FieldType::Boolean, false),
            FieldDefinition {
                name: "role".to_string(),
                display_name: "Role".to_string(),
                field_type: FieldType::Enum,
                required: true,
                unique: false,
                default_value: None,
                description: None,
                options: json!({"choices": ["admin", "user", "guest"]}),
            },
        ];

        // Valid
        let data = json!({"name": "Alice", "age": 30, "active": true, "role": "admin"});
        assert!(validate_record_data(&fields, &data).is_ok());

        // Wrong type for age
        let bad_data = json!({"name": "Alice", "age": "thirty", "role": "admin"});
        assert!(validate_record_data(&fields, &bad_data).is_err());

        // Invalid enum choice
        let bad_enum = json!({"name": "Alice", "age": 30, "role": "superadmin"});
        assert!(validate_record_data(&fields, &bad_enum).is_err());
    }

    // ── Serde roundtrip ──────────────────────────────────────────────

    #[test]
    fn field_type_serde_roundtrip() {
        let types = vec![
            FieldType::Text,
            FieldType::RichText,
            FieldType::Number,
            FieldType::Decimal,
            FieldType::Boolean,
            FieldType::Date,
            FieldType::DateTime,
            FieldType::Enum,
            FieldType::Reference,
            FieldType::Email,
            FieldType::Url,
            FieldType::Phone,
            FieldType::Json,
        ];

        for ft in types {
            let serialized = serde_json::to_string(&ft).unwrap();
            let deserialized: FieldType = serde_json::from_str(&serialized).unwrap();
            assert_eq!(ft, deserialized);
        }
    }

    #[test]
    fn field_definition_serde_roundtrip() {
        let field = FieldDefinition {
            name: "email".to_string(),
            display_name: "Email Address".to_string(),
            field_type: FieldType::Email,
            required: true,
            unique: true,
            default_value: None,
            description: Some("User's email".to_string()),
            options: json!({}),
        };

        let json = serde_json::to_value(&field).unwrap();
        let roundtripped: FieldDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(roundtripped.name, "email");
        assert_eq!(roundtripped.field_type, FieldType::Email);
        assert!(roundtripped.required);
        assert!(roundtripped.unique);
    }
}
