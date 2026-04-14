//! File upload API routes
//!
//! Provides multipart file upload, listing, metadata retrieval,
//! file serving, and deletion.

use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// Query params for listing uploads
#[derive(Debug, Deserialize)]
pub struct ListUploadsQuery {
    pub session_id: Option<String>,
    pub context: Option<String>,
    pub limit: Option<i64>,
}

/// Build upload routes
pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(upload_file))
        .route("/", get(list_uploads))
        .route("/{id}", get(get_upload))
        .route("/{id}", delete(delete_upload))
        .route("/{id}/file", get(serve_file))
}

/// Upload a file via multipart form data
///
/// Fields:
/// - `file` (required): the file binary
/// - `session_id` (optional): associate with a chat session
/// - `context` (optional): "chat", "document", "media" (default: "chat")
pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut original_filename = String::from("upload");
    let mut content_type = String::from("application/octet-stream");
    let mut session_id: Option<Uuid> = None;
    let mut context = String::from("chat");

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                if let Some(fname) = field.file_name() {
                    original_filename = fname.to_string();
                }
                if let Some(ct) = field.content_type() {
                    content_type = ct.to_string();
                }
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| {
                            tracing::error!("Failed to read upload: {e}");
                            (
                                StatusCode::BAD_REQUEST,
                                Json(json!({"error": "Failed to read file data"})),
                            )
                        })?
                        .to_vec(),
                );
            }
            "session_id" => {
                let text = field.text().await.unwrap_or_default();
                session_id = Uuid::parse_str(&text).ok();
            }
            "context" => {
                context = field.text().await.unwrap_or_else(|_| "chat".into());
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    let data = file_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "No file field in upload"})),
        )
    })?;

    // Enforce max upload size (20 MB)
    const MAX_SIZE: usize = 20 * 1024 * 1024;
    if data.len() > MAX_SIZE {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error": "File exceeds 20 MB limit"})),
        ));
    }

    let size_bytes = data.len() as i64;
    let id = Uuid::new_v4();

    // Derive extension from original filename
    let ext = original_filename
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 10)
        .unwrap_or("bin");
    let storage_key = format!("{}.{}", id, ext);

    // Upload to storage backend
    state
        .storage
        .upload(&storage_key, &data, &content_type)
        .await
        .map_err(|e| {
            tracing::error!("Storage upload failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Storage upload failed"})),
            )
        })?;

    // Persist metadata
    sqlx::query(
        r#"
        INSERT INTO uploads (id, filename, original_filename, content_type, size_bytes, storage_key, upload_context, session_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(id)
    .bind(&storage_key)
    .bind(&original_filename)
    .bind(&content_type)
    .bind(size_bytes)
    .bind(&storage_key)
    .bind(&context)
    .bind(session_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to save upload metadata: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to save upload record"})),
        )
    })?;

    tracing::info!(
        "Uploaded {} ({}, {} bytes) -> {}",
        original_filename,
        content_type,
        size_bytes,
        storage_key
    );

    Ok(Json(json!({
        "id": id,
        "filename": original_filename,
        "content_type": content_type,
        "size_bytes": size_bytes,
        "url": format!("/api/v1/uploads/{}/file", id),
    })))
}

/// List uploads, optionally filtered by session or context
pub async fn list_uploads(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListUploadsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let limit = params.limit.unwrap_or(50);

    // Build the query dynamically depending on filters
    let rows: Vec<(
        Uuid,
        String,
        String,
        i64,
        String,
        chrono::DateTime<chrono::Utc>,
    )> = if let Some(ref sid) = params.session_id {
        let uuid = Uuid::parse_str(sid).map_err(|_| StatusCode::BAD_REQUEST)?;
        sqlx::query_as(
            r#"
                SELECT id, original_filename, content_type, size_bytes, upload_context, created_at
                FROM uploads
                WHERE session_id = $1
                ORDER BY created_at DESC
                LIMIT $2
                "#,
        )
        .bind(uuid)
        .bind(limit)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        sqlx::query_as(
            r#"
                SELECT id, original_filename, content_type, size_bytes, upload_context, created_at
                FROM uploads
                ORDER BY created_at DESC
                LIMIT $1
                "#,
        )
        .bind(limit)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let uploads: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, filename, ct, size, ctx, created)| {
            json!({
                "id": id,
                "filename": filename,
                "content_type": ct,
                "size_bytes": size,
                "context": ctx,
                "url": format!("/api/v1/uploads/{}/file", id),
                "created_at": created,
            })
        })
        .collect();

    Ok(Json(json!({ "uploads": uploads })))
}

/// Row type for upload metadata queries
#[derive(Debug, sqlx::FromRow)]
struct UploadRow {
    pub id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_key: String,
    pub upload_context: String,
    pub session_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Get upload metadata by ID
pub async fn get_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let row = sqlx::query_as::<_, UploadRow>(
        "SELECT id, original_filename, content_type, size_bytes, storage_key, upload_context, session_id, created_at FROM uploads WHERE id = $1",
    )
    .bind(uuid)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!({
        "id": row.id,
        "filename": row.original_filename,
        "content_type": row.content_type,
        "size_bytes": row.size_bytes,
        "context": row.upload_context,
        "session_id": row.session_id,
        "url": format!("/api/v1/uploads/{}/file", row.id),
        "created_at": row.created_at,
    })))
}

/// Delete an upload (file + DB record)
pub async fn delete_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Get the storage key first
    let row: Option<(String,)> = sqlx::query_as("SELECT storage_key FROM uploads WHERE id = $1")
        .bind(uuid)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (storage_key,) = row.ok_or(StatusCode::NOT_FOUND)?;

    // Delete from storage
    if let Err(e) = state.storage.delete(&storage_key).await {
        tracing::warn!("Failed to delete file from storage: {e}");
    }

    // Delete DB record
    sqlx::query("DELETE FROM uploads WHERE id = $1")
        .bind(uuid)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({"status": "deleted", "id": id})))
}

/// Serve the actual file content (for inline display, downloads)
pub async fn serve_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT storage_key, content_type, original_filename FROM uploads WHERE id = $1",
    )
    .bind(uuid)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (storage_key, content_type, original_filename) = row.ok_or(StatusCode::NOT_FOUND)?;

    let data = state
        .storage
        .read(&storage_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let safe_filename = sanitize_content_disposition_filename(&original_filename);
    let headers = [
        (header::CONTENT_TYPE, content_type),
        (
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", safe_filename),
        ),
        (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
    ];

    Ok((headers, Body::from(data)))
}

/// Sanitize a filename for use in Content-Disposition header.
/// Strips characters that could enable header injection.
fn sanitize_content_disposition_filename(filename: &str) -> String {
    let sanitized: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_' || *c == ' ')
        .collect();
    sanitized.trim().to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper for loading attachment data into the agent
// ═══════════════════════════════════════════════════════════════════════════

/// Load an upload's file data and metadata, returning (content_type, filename, data).
/// Used by the agent chat route to convert attachments to content blocks.
pub async fn load_upload_data(
    pool: &sqlx::PgPool,
    storage: &crate::storage::StorageClient,
    upload_id: Uuid,
) -> Result<(String, String, Vec<u8>), StatusCode> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT storage_key, content_type, original_filename FROM uploads WHERE id = $1",
    )
    .bind(upload_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (storage_key, content_type, filename) = row.ok_or(StatusCode::NOT_FOUND)?;

    let data = storage
        .read(&storage_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((content_type, filename, data))
}
