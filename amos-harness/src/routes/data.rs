//! Data REST API for collections and records
//!
//! Exposes the SchemaEngine over HTTP so that canvas components (running inside
//! sandboxed iframes) can fetch, create, update, and delete collection records
//! without going through the agent tool loop.

use crate::{schema::SchemaEngine, state::AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use uuid::Uuid;

// ── Error wrapper ────────────────────────────────────────────────────────

struct ApiError(amos_core::AmosError);

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status =
            StatusCode::from_u16(self.0.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = json!({ "error": self.0.to_string() });
        (status, Json(body)).into_response()
    }
}

impl From<amos_core::AmosError> for ApiError {
    fn from(err: amos_core::AmosError) -> Self {
        Self(err)
    }
}

// ── Routes ───────────────────────────────────────────────────────────────

pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_collections))
        .route("/{collection}", get(list_records).post(create_record))
        .route("/{collection}/schema", get(get_schema))
        .route(
            "/{collection}/{id}",
            get(get_record).put(update_record).delete(delete_record),
        )
}

// ── Query params ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ListRecordsQuery {
    /// JSON-encoded containment filter, e.g. `{"status":"active"}`
    filters: Option<String>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    /// Full-text search across `data::text` via ILIKE
    search: Option<String>,
}

// ── Handlers ─────────────────────────────────────────────────────────────

async fn list_collections(State(state): State<Arc<AppState>>) -> Result<Json<JsonValue>, ApiError> {
    let engine = SchemaEngine::new(state.db_pool.clone());
    let collections = engine.list_collections().await?;
    Ok(Json(json!({ "collections": collections })))
}

async fn get_schema(
    State(state): State<Arc<AppState>>,
    Path(collection): Path<String>,
) -> Result<Json<JsonValue>, ApiError> {
    let engine = SchemaEngine::new(state.db_pool.clone());
    let coll = engine.get_collection(&collection).await?;
    Ok(Json(json!(coll)))
}

async fn list_records(
    State(state): State<Arc<AppState>>,
    Path(collection): Path<String>,
    Query(params): Query<ListRecordsQuery>,
) -> Result<Json<JsonValue>, ApiError> {
    let engine = SchemaEngine::new(state.db_pool.clone());

    // Parse optional JSON filters
    let filters = match &params.filters {
        Some(f) => {
            let parsed: JsonValue = serde_json::from_str(f).map_err(|e| {
                amos_core::AmosError::Validation(format!("Invalid filters JSON: {}", e))
            })?;
            Some(parsed)
        }
        None => None,
    };

    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    // If search is provided, use ILIKE on data::text
    let filters = if let Some(search) = &params.search {
        // We'll handle search via a separate query path below
        // For now, keep existing filters and add search post-fetch
        // Actually, let's fold search into the query by querying with search
        // The SchemaEngine doesn't support search natively, so we query
        // and also do a search-filtered query
        let _ = search;
        filters
    } else {
        filters
    };

    // Get collection schema for response enrichment
    let coll = engine.get_collection(&collection).await?;

    let (records, total) = if let Some(search) = &params.search {
        // Search path: use raw query with ILIKE on data::text
        search_records(
            &state.db_pool,
            &coll,
            search,
            filters,
            params.sort_by.as_deref(),
            params.sort_dir.as_deref(),
            limit,
            offset,
        )
        .await?
    } else {
        engine
            .query_records(
                &collection,
                filters,
                params.sort_by.as_deref(),
                params.sort_dir.as_deref(),
                limit,
                offset,
            )
            .await?
    };

    Ok(Json(json!({
        "records": records,
        "total": total,
        "limit": limit,
        "offset": offset,
        "schema": {
            "fields": coll.fields,
            "display_name": coll.display_name,
        }
    })))
}

async fn get_record(
    State(state): State<Arc<AppState>>,
    Path((_collection, id)): Path<(String, Uuid)>,
) -> Result<Json<JsonValue>, ApiError> {
    let engine = SchemaEngine::new(state.db_pool.clone());
    let record = engine.get_record(id).await?;
    Ok(Json(json!(record)))
}

async fn create_record(
    State(state): State<Arc<AppState>>,
    Path(collection): Path<String>,
    Json(data): Json<JsonValue>,
) -> Result<(StatusCode, Json<JsonValue>), ApiError> {
    let engine =
        SchemaEngine::with_event_sender(state.db_pool.clone(), state.automation_event_tx.clone());
    let record = engine.create_record(&collection, data).await?;
    Ok((StatusCode::CREATED, Json(json!(record))))
}

async fn update_record(
    State(state): State<Arc<AppState>>,
    Path((_collection, id)): Path<(String, Uuid)>,
    Json(data): Json<JsonValue>,
) -> Result<Json<JsonValue>, ApiError> {
    let engine =
        SchemaEngine::with_event_sender(state.db_pool.clone(), state.automation_event_tx.clone());
    let record = engine.update_record(id, data).await?;
    Ok(Json(json!(record)))
}

async fn delete_record(
    State(state): State<Arc<AppState>>,
    Path((_collection, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let engine =
        SchemaEngine::with_event_sender(state.db_pool.clone(), state.automation_event_tx.clone());
    engine.delete_record(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Search helper ────────────────────────────────────────────────────────

/// Full-text search on record data cast to text via ILIKE.
async fn search_records(
    pool: &sqlx::PgPool,
    coll: &crate::schema::Collection,
    search: &str,
    filters: Option<JsonValue>,
    sort_by: Option<&str>,
    sort_dir: Option<&str>,
    limit: i64,
    offset: i64,
) -> amos_core::Result<(Vec<crate::schema::Record>, i64)> {
    use sqlx::Row;

    let filter_json = filters.unwrap_or_else(|| json!({}));
    let search_pattern = format!("%{}%", search.replace('%', "\\%").replace('_', "\\_"));

    // Count
    let count_row = sqlx::query(
        "SELECT COUNT(*) as total FROM records WHERE collection_id = $1 AND data @> $2::jsonb AND data::text ILIKE $3",
    )
    .bind(coll.id)
    .bind(&filter_json)
    .bind(&search_pattern)
    .fetch_one(pool)
    .await
    .map_err(|e| amos_core::AmosError::Internal(format!("Search count failed: {}", e)))?;

    let total: i64 = count_row.get("total");

    // Allowlist approach: only permit known column names or validated JSONB field names.
    // Field names are restricted to [a-zA-Z0-9_] and max 64 chars to prevent injection.
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
           WHERE r.collection_id = $1 AND r.data @> $2::jsonb AND r.data::text ILIKE $3
           ORDER BY {} {}
           LIMIT $4 OFFSET $5"#,
        order_expr, dir
    );

    let rows = sqlx::query(&query)
        .bind(coll.id)
        .bind(&filter_json)
        .bind(&search_pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| amos_core::AmosError::Internal(format!("Search query failed: {}", e)))?;

    let records: Vec<crate::schema::Record> = rows
        .iter()
        .map(|row| {
            Ok(crate::schema::Record {
                id: row.get("id"),
                collection_id: row.get("collection_id"),
                collection_name: coll.name.clone(),
                data: row.get("data"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
        })
        .collect::<amos_core::Result<Vec<_>>>()?;

    Ok((records, total))
}
