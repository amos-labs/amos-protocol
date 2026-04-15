//! HTTP middleware for error handling and authentication.

use amos_core::AmosError;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// Error response wrapper to avoid orphan rule violations.
pub struct ErrorResponse(pub AmosError);

/// Convert ErrorResponse to HTTP response.
impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status_code =
            StatusCode::from_u16(self.0.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let message = self.0.to_string();

        let body = Json(json!({
            "error": message,
            "status": status_code.as_u16(),
        }));

        (status_code, body).into_response()
    }
}

/// API key authentication middleware for relay endpoints.
///
/// Extracts Bearer token from Authorization header, hashes it with SHA-256,
/// and checks against `api_key_hash` in the `relay_harnesses` or `relay_agents` table.
///
/// Skips authentication for health check and public discovery endpoints.
pub async fn api_key_auth(
    State(db): State<PgPool>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path().to_string();

    // Skip auth for health and public read-only endpoints
    if path == "/health"
        || path.starts_with("/api/v1/harnesses/connect")
        || path.starts_with("/api/v1/agents/register")
        || path.starts_with("/api/v1/pool/")
    {
        return Ok(next.run(req).await);
    }

    // Extract Bearer token
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Hash the token for comparison
    let token_hash = hash_api_key(token);

    // Check against harnesses first, then agents
    let is_valid = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM relay_harnesses WHERE api_key_hash = $1 AND status = 'active'
            UNION ALL
            SELECT 1 FROM relay_agents WHERE id::text = $1
        )
        "#,
    )
    .bind(&token_hash)
    .fetch_one(&db)
    .await
    .unwrap_or(false);

    if !is_valid {
        tracing::warn!(
            path = %path,
            "API key authentication failed — rejecting request"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

/// Hash an API key with SHA-256 for secure storage/comparison.
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a random API key for harness/agent registration.
pub fn generate_api_key(prefix: &str) -> String {
    use rand::RngCore;
    let mut random_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    format!("{}_{}", prefix, hex::encode(random_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Error response mapping ─────────────────────────────────────────

    #[test]
    fn test_error_mapping_not_found() {
        let err = AmosError::NotFound {
            entity: "resource".to_string(),
            id: "123".to_string(),
        };
        let response = ErrorResponse(err).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_error_mapping_validation() {
        let err = AmosError::Validation("invalid input".to_string());
        let response = ErrorResponse(err).into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_error_mapping_unauthorized() {
        let err = AmosError::Unauthorized("invalid credentials".to_string());
        let response = ErrorResponse(err).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_error_mapping_internal() {
        let err = AmosError::Internal("something broke".to_string());
        let response = ErrorResponse(err).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── API key generation ─────────────────────────────────────────────

    #[test]
    fn test_api_key_generation_format() {
        let key = generate_api_key("relay");
        assert!(key.starts_with("relay_"));
        assert_eq!(key.len(), 6 + 64); // "relay_" + 64 hex chars
    }

    #[test]
    fn test_api_key_generation_different_prefixes() {
        let key1 = generate_api_key("harness");
        let key2 = generate_api_key("agent");
        assert!(key1.starts_with("harness_"));
        assert!(key2.starts_with("agent_"));
    }

    #[test]
    fn test_api_key_generation_uniqueness() {
        let key1 = generate_api_key("relay");
        let key2 = generate_api_key("relay");
        // Two generated keys should be different (overwhelming probability)
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_api_key_generation_hex_chars() {
        let key = generate_api_key("test");
        let hex_part = &key[5..]; // Skip "test_"
                                  // Should only contain valid hex characters
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── API key hashing ────────────────────────────────────────────────

    #[test]
    fn test_api_key_hashing_deterministic() {
        let hash1 = hash_api_key("test_key");
        let hash2 = hash_api_key("test_key");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_api_key_hashing_length() {
        let hash = hash_api_key("test_key");
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex chars
    }

    #[test]
    fn test_api_key_hashing_different_keys() {
        let hash1 = hash_api_key("key_one");
        let hash2 = hash_api_key("key_two");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_api_key_hashing_hex_output() {
        let hash = hash_api_key("anything");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_api_key_roundtrip() {
        // Generate a key, hash it, verify the hash is consistent
        let key = generate_api_key("eap");
        let hash1 = hash_api_key(&key);
        let hash2 = hash_api_key(&key);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }
}
