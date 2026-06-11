//! HTTP middleware for error handling and authentication.

use amos_core::AmosError;
use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
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
        // Log the full error server-side; respond with a client-safe message
        // so SQL fragments, paths, and upstream URLs never leak to clients.
        if status_code.is_server_error() {
            tracing::error!(error = %self.0, "relay request failed");
        }
        let message = self.0.client_message();

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
    let method = req.method().clone();

    // Skip auth for health, public read-only, and webhook endpoints (use HMAC auth)
    if path == "/health"
        || path.starts_with("/api/v1/harnesses/connect")
        || path.starts_with("/api/v1/agents/register")
        || path.starts_with("/api/v1/pool/")
        || path.starts_with("/api/v1/webhooks/")
    {
        return Ok(next.run(req).await);
    }

    // Public read-only access to bounty board and agent directory (marketplace)
    if method == Method::GET
        && (path.starts_with("/api/v1/bounties") || path.starts_with("/api/v1/agents"))
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

    // Two valid auth shapes:
    //   - Harness API key:  hash and compare to relay_harnesses.api_key_hash
    //   - Agent UUID:        compare raw token to relay_agents.id::text
    // The previous query bound only the hash for both branches, so agent-UUID
    // auth was effectively broken — no agent ID will ever equal its own SHA256.
    let token_hash = hash_api_key(token);

    let is_valid = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM relay_harnesses
              WHERE api_key_hash = $1 AND status = 'active'
            UNION ALL
            SELECT 1 FROM relay_agents
              WHERE id::text = $2 AND status = 'active'
        )
        "#,
    )
    .bind(&token_hash)
    .bind(token)
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

/// X-Request-ID middleware.
///
/// Generates a UUID v4 at request entry, attaches it to the request extension
/// (so handlers can read via `Extension<RequestId>` if needed), echoes it in
/// the `X-Request-ID` response header, and joins it into a tracing span so
/// log lines from different layers can be correlated when an agent makes a
/// relay call.
///
/// If the client supplies an `X-Request-ID` header on the way in, we honor it
/// (lets harnesses propagate trace IDs end-to-end). Otherwise we mint a fresh
/// UUID v4. Bounded length + character set so a malicious caller can't pollute
/// logs with arbitrary content.
pub async fn request_id(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{header::HeaderValue, HeaderName};

    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Attach to request extensions for handler access.
    req.extensions_mut().insert(RequestId(request_id.clone()));

    // Span the rest of the request with the id so structured logs can be joined.
    let span = tracing::info_span!("request", request_id = %request_id);
    let mut response = {
        let _enter = span.enter();
        next.run(req).await
    };

    // Echo on the response.
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-request-id"), value);
    }
    response
}

/// Newtype for request id, exposed as an Axum extension. Handlers can use
/// `axum::extract::Extension<RequestId>` to read it.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

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

    // ── X-Request-ID middleware ───────────────────────────────────────

    #[tokio::test]
    async fn request_id_middleware_mints_uuid_when_absent() {
        use axum::body::Body;
        use axum::extract::Request;
        use axum::http::Method;
        use axum::middleware::from_fn;
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        async fn echo() -> &'static str {
            "ok"
        }
        let app = Router::new()
            .route("/", get(echo))
            .layer(from_fn(request_id));
        let req = Request::builder()
            .method(Method::GET)
            .uri("/")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let id = resp
            .headers()
            .get("x-request-id")
            .expect("response missing x-request-id")
            .to_str()
            .unwrap();
        // UUIDv4 is 36 chars (8-4-4-4-12)
        assert_eq!(id.len(), 36);
        assert!(id.contains('-'));
    }

    #[tokio::test]
    async fn request_id_middleware_honors_caller_supplied_id() {
        use axum::body::Body;
        use axum::extract::Request;
        use axum::http::{HeaderValue, Method};
        use axum::middleware::from_fn;
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        async fn echo() -> &'static str {
            "ok"
        }
        let app = Router::new()
            .route("/", get(echo))
            .layer(from_fn(request_id));
        let mut req = Request::builder()
            .method(Method::GET)
            .uri("/")
            .body(Body::empty())
            .unwrap();
        req.headers_mut()
            .insert("x-request-id", HeaderValue::from_static("trace-abc-123"));
        let resp = app.oneshot(req).await.unwrap();
        let id = resp
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(id, "trace-abc-123");
    }

    #[tokio::test]
    async fn request_id_middleware_rejects_oversized_caller_id() {
        use axum::body::Body;
        use axum::extract::Request;
        use axum::http::{HeaderValue, Method};
        use axum::middleware::from_fn;
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        async fn echo() -> &'static str {
            "ok"
        }
        let app = Router::new()
            .route("/", get(echo))
            .layer(from_fn(request_id));
        let mut req = Request::builder()
            .method(Method::GET)
            .uri("/")
            .body(Body::empty())
            .unwrap();
        // 100-char id — over the 64 cap, should be replaced with a fresh UUID.
        req.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_static("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let resp = app.oneshot(req).await.unwrap();
        let id = resp
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert_ne!(id.len(), 100);
        assert_eq!(id.len(), 36);
    }
}
