//! HTTP security headers middleware.
//!
//! Adds standard security headers to all responses:
//! - Strict-Transport-Security (HSTS)
//! - X-Content-Type-Options: nosniff
//! - X-Frame-Options: DENY (SAMEORIGIN for canvas/sites paths)
//! - Referrer-Policy: strict-origin-when-cross-origin
//! - X-XSS-Protection: 0 (disabled — CSP is the modern replacement)
//! - Permissions-Policy: restricts sensitive browser APIs
//!
//! AMOS-SECURE-004

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Security headers middleware.
///
/// Canvas and sites paths get `X-Frame-Options: SAMEORIGIN` (they render in
/// iframes). All other paths get `DENY`.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();

    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    // HSTS: enforce HTTPS for 1 year, include subdomains
    headers.insert(
        "strict-transport-security",
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );

    // Prevent MIME-type sniffing
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );

    // Clickjacking protection: SAMEORIGIN for embeddable paths, DENY for everything else
    let frame_options =
        if path.starts_with("/c/") || path.starts_with("/api/v1/canvas") || path.starts_with("/s/")
        {
            "SAMEORIGIN"
        } else {
            "DENY"
        };
    headers.insert("x-frame-options", HeaderValue::from_static(frame_options));

    // Referrer policy: send origin on cross-origin, full URL on same-origin
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    // Disable legacy XSS filter (CSP is the replacement; the filter can cause issues)
    headers.insert("x-xss-protection", HeaderValue::from_static("0"));

    // Restrict browser APIs the harness doesn't need
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static(
            "camera=(), microphone=(), geolocation=(), payment=(), usb=(), magnetometer=()",
        ),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, routing::get, Router};
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn app() -> Router {
        Router::new()
            .route("/api/v1/test", get(ok_handler))
            .route("/c/my-canvas", get(ok_handler))
            .route("/s/my-site", get(ok_handler))
            .layer(axum::middleware::from_fn(security_headers))
    }

    async fn get_response(uri: &str) -> axum::response::Response {
        app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_hsts_header() {
        let resp = get_response("/api/v1/test").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000; includeSubDomains"
        );
    }

    #[tokio::test]
    async fn test_nosniff_header() {
        let resp = get_response("/api/v1/test").await;
        assert_eq!(
            resp.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
    }

    #[tokio::test]
    async fn test_frame_options_deny_for_api() {
        let resp = get_response("/api/v1/test").await;
        assert_eq!(resp.headers().get("x-frame-options").unwrap(), "DENY");
    }

    #[tokio::test]
    async fn test_frame_options_sameorigin_for_canvas() {
        let resp = get_response("/c/my-canvas").await;
        assert_eq!(resp.headers().get("x-frame-options").unwrap(), "SAMEORIGIN");
    }

    #[tokio::test]
    async fn test_frame_options_sameorigin_for_sites() {
        let resp = get_response("/s/my-site").await;
        assert_eq!(resp.headers().get("x-frame-options").unwrap(), "SAMEORIGIN");
    }

    #[tokio::test]
    async fn test_referrer_policy() {
        let resp = get_response("/api/v1/test").await;
        assert_eq!(
            resp.headers().get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
    }

    #[tokio::test]
    async fn test_xss_protection_disabled() {
        let resp = get_response("/api/v1/test").await;
        assert_eq!(resp.headers().get("x-xss-protection").unwrap(), "0");
    }

    #[tokio::test]
    async fn test_permissions_policy() {
        let resp = get_response("/api/v1/test").await;
        let pp = resp
            .headers()
            .get("permissions-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(pp.contains("camera=()"));
        assert!(pp.contains("microphone=()"));
        assert!(pp.contains("geolocation=()"));
    }
}
