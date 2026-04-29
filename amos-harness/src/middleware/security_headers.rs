//! HTTP security headers middleware.
//!
//! Adds standard security headers to all responses:
//! - Strict-Transport-Security (HSTS)
//! - X-Content-Type-Options: nosniff
//! - X-Frame-Options: DENY (SAMEORIGIN for canvas/sites paths)
//! - Referrer-Policy: strict-origin-when-cross-origin
//! - X-XSS-Protection: 0 (disabled — CSP is the modern replacement)
//! - Permissions-Policy: restricts sensitive browser APIs
//! - Content-Security-Policy (canvas/app iframes only — SECURE-005)
//!
//! AMOS-SECURE-004, AMOS-SECURE-005

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Content-Security-Policy header for canvas and app-iframe paths.
///
/// This is the coarse-grained defense-in-depth CSP attached via HTTP
/// header. It does NOT carry a nonce — the per-response nonce lives in
/// a `<meta http-equiv="Content-Security-Policy">` tag emitted by
/// [`CanvasResponse::freeform`][1] and `buildCanvasDocument` in
/// `static/js/app.js`. Browsers enforce all active CSPs simultaneously,
/// so the effective policy is the **intersection** of this header and
/// the per-document meta tag — which means inline scripts only run when
/// they carry the matching nonce.
///
/// This header omits `'unsafe-inline'` entirely (SECURE-005 hardening
/// over the initial rollout): if a future handler returns canvas HTML
/// without the meta-tag nonce mechanism, the header still blocks inline
/// scripts rather than silently weakening the posture.
///
/// Allowances:
/// - `script-src` / `style-src`: `'self'` plus the Bootstrap / Lucide /
///   Chart.js CDNs used by canvas rendering. Nonce-bearing inline
///   scripts are allowed by the meta-tag CSP; this header simply
///   doesn't admit non-nonced inline scripts.
/// - `connect-src 'self'`: canvas code may call `/api/v1/*` but cannot
///   exfiltrate to arbitrary third-party origins.
/// - `img-src`: `'self' data: https:` to keep inline data URLs and
///   third-party images working.
/// - `object-src 'none'`, `base-uri 'self'`, `form-action 'self'`,
///   `frame-ancestors 'self'`: close the most common CSP bypass paths.
///
/// Emitted on every response served out of the canvas route namespace
/// (`/c/*` and `/api/v1/canvas*`).
///
/// [1]: crate::canvas::types::CanvasResponse::freeform
const CANVAS_CSP: &str = concat!(
    "default-src 'self'; ",
    "script-src 'self' https://cdn.jsdelivr.net https://unpkg.com; ",
    "style-src 'self' https://cdn.jsdelivr.net; ",
    "img-src 'self' data: https:; ",
    "font-src 'self' data: https://cdn.jsdelivr.net; ",
    "connect-src 'self'; ",
    "object-src 'none'; ",
    "base-uri 'self'; ",
    "form-action 'self'; ",
    "frame-ancestors 'self'",
);

/// Returns `true` if the request path serves canvas/app-iframe HTML.
///
/// Kept as a separate helper so the behavior is directly unit-testable and
/// so the same predicate can be reused for other canvas-specific headers.
pub(crate) fn is_canvas_path(path: &str) -> bool {
    path.starts_with("/c/") || path.starts_with("/api/v1/canvas")
}

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

    // Canvas/app-iframe CSP (SECURE-005): only emit on canvas paths so the
    // policy doesn't leak onto unrelated API responses.
    if is_canvas_path(&path) {
        headers.insert(
            "content-security-policy",
            HeaderValue::from_static(CANVAS_CSP),
        );
    }

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

    // ── SECURE-005: Content-Security-Policy for canvas/app iframes ────

    fn app_with_canvas_route() -> Router {
        Router::new()
            .route("/api/v1/test", get(ok_handler))
            .route("/c/my-canvas", get(ok_handler))
            .route("/api/v1/canvases/list", get(ok_handler))
            .route("/s/my-site", get(ok_handler))
            .layer(axum::middleware::from_fn(security_headers))
    }

    async fn get_response_with_canvas_app(uri: &str) -> axum::response::Response {
        app_with_canvas_route()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_csp_set_on_canvas_path() {
        let resp = get_response_with_canvas_app("/c/my-canvas").await;
        let csp = resp.headers().get("content-security-policy");
        assert!(csp.is_some(), "CSP header should be present on /c/ paths");
        let csp_str = csp.unwrap().to_str().unwrap();
        assert!(
            csp_str.contains("default-src 'self'"),
            "CSP missing default-src: {}",
            csp_str
        );
        assert!(
            csp_str.contains("object-src 'none'"),
            "CSP missing object-src 'none': {}",
            csp_str
        );
        assert!(
            csp_str.contains("base-uri 'self'"),
            "CSP missing base-uri: {}",
            csp_str
        );
        assert!(
            csp_str.contains("frame-ancestors 'self'"),
            "CSP missing frame-ancestors: {}",
            csp_str
        );
        assert!(
            csp_str.contains("connect-src 'self'"),
            "CSP missing connect-src: {}",
            csp_str
        );
    }

    #[tokio::test]
    async fn test_csp_set_on_canvas_api_path() {
        let resp = get_response_with_canvas_app("/api/v1/canvases/list").await;
        assert!(
            resp.headers().get("content-security-policy").is_some(),
            "CSP should be present on /api/v1/canvas* paths"
        );
    }

    #[tokio::test]
    async fn test_csp_absent_on_regular_api_path() {
        let resp = get_response_with_canvas_app("/api/v1/test").await;
        assert!(
            resp.headers().get("content-security-policy").is_none(),
            "CSP should NOT leak onto non-canvas API paths"
        );
    }

    #[tokio::test]
    async fn test_csp_absent_on_sites_path() {
        // /s/ (sites) has its own CSP embedded as a <meta> tag in sites.rs;
        // the middleware-level CSP is canvas-specific.
        let resp = get_response_with_canvas_app("/s/my-site").await;
        assert!(
            resp.headers().get("content-security-policy").is_none(),
            "sites manage their own CSP via meta tag"
        );
    }

    #[tokio::test]
    async fn test_csp_forbids_object_embed_scripts_from_elsewhere() {
        // Assert the specific directives that block common CSP-bypass
        // payloads.
        let resp = get_response_with_canvas_app("/c/my-canvas").await;
        let csp_str = resp
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        // No 'unsafe-eval' — blocks eval()/new Function()-based exploits
        assert!(
            !csp_str.contains("'unsafe-eval'"),
            "CSP must NOT allow unsafe-eval: {}",
            csp_str
        );
        // No wildcard script-src
        assert!(
            !csp_str.contains("script-src *"),
            "CSP must NOT allow wildcard script-src: {}",
            csp_str
        );
    }

    #[tokio::test]
    async fn test_csp_header_does_not_allow_unsafe_inline() {
        // Regression guard: the header-level CSP must NOT weaken back to
        // `'unsafe-inline'`. Nonces (emitted via meta-tag CSP by
        // `CanvasResponse::freeform` and `buildCanvasDocument`) are the
        // mechanism that allows inline scripts to run. If somebody
        // adds `'unsafe-inline'` back to the header, the meta-tag nonce
        // becomes pointless — the intersection would admit any inline
        // script.
        let resp = get_response_with_canvas_app("/c/my-canvas").await;
        let csp_str = resp
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            !csp_str.contains("'unsafe-inline'"),
            "canvas CSP header must NOT include 'unsafe-inline' (SECURE-005): {}",
            csp_str
        );
    }

    #[test]
    fn test_is_canvas_path_helper() {
        assert!(is_canvas_path("/c/my-canvas"));
        assert!(is_canvas_path("/c/"));
        assert!(is_canvas_path("/api/v1/canvas"));
        assert!(is_canvas_path("/api/v1/canvases/list"));

        assert!(!is_canvas_path("/api/v1/bounties"));
        assert!(!is_canvas_path("/s/my-site"));
        assert!(!is_canvas_path("/login"));
        assert!(!is_canvas_path("/"));
        assert!(!is_canvas_path(""));
    }
}
