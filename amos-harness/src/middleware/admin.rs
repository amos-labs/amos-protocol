//! Admin API key authentication extractor.
//!
//! Validates the `X-Admin-Key` header against `AMOS__ADMIN__API_KEY`.
//! If the env var is unset or empty, the admin API is disabled and all
//! requests are rejected with 401 (matches the platform's admin pattern).

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};
use serde_json::json;

/// Extractor for endpoints that require the admin API key.
///
/// Behavior:
/// - Reads `AMOS__ADMIN__API_KEY` from the environment.
/// - If the env var is empty/unset → 401 with `admin_disabled`.
/// - If the `X-Admin-Key` header is missing or doesn't match → 401 with `admin_invalid_key`.
/// - Otherwise the extractor resolves, and the handler runs.
#[derive(Debug)]
pub struct AdminAuth;

impl<S: Send + Sync> FromRequestParts<S> for AdminAuth {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let configured_key = std::env::var("AMOS__ADMIN__API_KEY").unwrap_or_default();

        if configured_key.is_empty() {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Admin API is disabled (AMOS__ADMIN__API_KEY not configured)",
                    "code": "admin_disabled"
                })),
            ));
        }

        let provided = parts
            .headers
            .get("X-Admin-Key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if provided != configured_key {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Invalid admin API key",
                    "code": "admin_invalid_key"
                })),
            ));
        }

        Ok(AdminAuth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Method, Request};
    use std::sync::Mutex;

    // AMOS__ADMIN__API_KEY is process-global, so every test that touches it
    // has to serialize through this mutex or they'll race against each other
    // under the default multi-threaded test runner.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn build_parts(key_header: Option<&str>) -> Parts {
        let mut req = Request::builder().method(Method::POST).uri("/pause");
        if let Some(k) = key_header {
            req = req.header("X-Admin-Key", k);
        }
        req.body(()).unwrap().into_parts().0
    }

    struct DummyState;

    #[tokio::test(flavor = "current_thread")]
    #[allow(clippy::await_holding_lock)] // Intentional — serializing env-var access across tests.
    async fn admin_auth_covers_all_paths() {
        let _guard = ENV_LOCK.lock().unwrap();

        // 1. Env var unset → admin_disabled regardless of header.
        std::env::remove_var("AMOS__ADMIN__API_KEY");
        let mut parts = build_parts(Some("anything"));
        let err = AdminAuth::from_request_parts(&mut parts, &DummyState)
            .await
            .expect_err("expected 401 when env var unset");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1["code"], "admin_disabled");

        // 2. Env var set, missing header → admin_invalid_key.
        std::env::set_var("AMOS__ADMIN__API_KEY", "secret-key-abc");
        let mut parts = build_parts(None);
        let err = AdminAuth::from_request_parts(&mut parts, &DummyState)
            .await
            .expect_err("expected 401 when header missing");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1["code"], "admin_invalid_key");

        // 3. Env var set, wrong header → admin_invalid_key.
        let mut parts = build_parts(Some("wrong-key"));
        let err = AdminAuth::from_request_parts(&mut parts, &DummyState)
            .await
            .expect_err("expected 401 when key wrong");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1["code"], "admin_invalid_key");

        // 4. Matching header → extractor resolves.
        let mut parts = build_parts(Some("secret-key-abc"));
        let res = AdminAuth::from_request_parts(&mut parts, &DummyState).await;
        assert!(
            res.is_ok(),
            "expected AdminAuth to resolve when key matches"
        );

        std::env::remove_var("AMOS__ADMIN__API_KEY");
    }
}
