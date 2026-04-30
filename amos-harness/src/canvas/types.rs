//! Canvas data types

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

/// Generate a cryptographically random CSP nonce (22-char base64url).
///
/// 128 bits of randomness — OWASP's recommended minimum for CSP nonces.
/// Base64url encoding keeps the value safe to drop into both the CSP
/// header and HTML attributes without escaping.
pub(crate) fn generate_csp_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Build the canvas-iframe CSP string for use inside a
/// `<meta http-equiv="Content-Security-Policy" content="...">` tag.
///
/// The meta-tag CSP is the authoritative policy for canvas documents.
///
/// **Why `'unsafe-inline'` is allowed for both script-src and style-src:**
///
/// Pre-existing framework canvases (system-settings, login, register, the
/// dynamic LLM-generated ones, etc.) and user-generated canvases written
/// by agents both rely heavily on inline `onclick`/`onchange` event
/// handlers and inline `style="..."` attributes. SECURE-005 (2026-04-29)
/// removed `'unsafe-inline'`, which broke every interactive control on
/// every existing canvas — confirmed in production 2026-04-30 when the
/// BYOK↔Bedrock toggle and every other button stopped responding.
///
/// The primary defense is the iframe sandbox plus same-origin restriction
/// plus ammonia HTML sanitization on user-submitted content. CSP is
/// defense-in-depth. Relaxing CSP back to allow inline scripts/styles
/// inside this sandbox keeps the iframe-bounded content interactive
/// while not weakening the cross-origin protection.
///
/// **Follow-up (tracked):** rewrite framework canvases to be CSP-clean
/// (use `addEventListener` + class-based styling), then tighten back
/// to nonce-only. That's a multi-canvas refactor, not a one-liner.
///
/// What's still locked down:
/// - `connect-src 'self'` blocks data exfiltration to attacker domains.
/// - `object-src 'none'` blocks Flash/plugin embeds.
/// - `base-uri 'self'` and `form-action 'self'` block base/form hijacks.
/// - `frame-ancestors 'self'` blocks the canvas being embedded by foreign
///   pages (clickjacking).
pub(crate) fn canvas_meta_csp(nonce: &str) -> String {
    format!(
        "default-src 'self'; \
         script-src 'self' 'unsafe-inline' 'nonce-{nonce}' https://cdn.jsdelivr.net https://unpkg.com; \
         style-src 'self' 'unsafe-inline' 'nonce-{nonce}' https://cdn.jsdelivr.net; \
         img-src 'self' data: https:; \
         font-src 'self' data: https://cdn.jsdelivr.net; \
         connect-src 'self'; \
         object-src 'none'; \
         base-uri 'self'; \
         form-action 'self'; \
         frame-ancestors 'self'"
    )
}

/// Canvas type enumeration
///
/// Stored as VARCHAR in Postgres (not a custom enum type), so we implement
/// sqlx traits manually to map to/from TEXT.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CanvasType {
    /// Dynamic data-driven canvas with templates
    Dynamic,
    /// Freeform custom HTML/JS/CSS
    Freeform,
    /// Dashboard with widgets
    Dashboard,
    /// Data grid/table
    DataGrid,
    /// Form for data entry
    Form,
    /// Detail view for a single record
    Detail,
    /// Kanban board
    Kanban,
    /// Calendar view
    Calendar,
    /// Report with charts
    Report,
    /// Multi-step wizard
    Wizard,
    /// Custom canvas type
    Custom,
}

impl CanvasType {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            CanvasType::Dynamic => "dynamic",
            CanvasType::Freeform => "freeform",
            CanvasType::Dashboard => "dashboard",
            CanvasType::DataGrid => "datagrid",
            CanvasType::Form => "form",
            CanvasType::Detail => "detail",
            CanvasType::Kanban => "kanban",
            CanvasType::Calendar => "calendar",
            CanvasType::Report => "report",
            CanvasType::Wizard => "wizard",
            CanvasType::Custom => "custom",
        }
    }

    /// Parse from database string
    pub fn from_str(s: &str) -> Self {
        match s {
            "dynamic" => CanvasType::Dynamic,
            "freeform" => CanvasType::Freeform,
            "dashboard" => CanvasType::Dashboard,
            "datagrid" => CanvasType::DataGrid,
            "form" => CanvasType::Form,
            "detail" => CanvasType::Detail,
            "kanban" => CanvasType::Kanban,
            "calendar" => CanvasType::Calendar,
            "report" => CanvasType::Report,
            "wizard" => CanvasType::Wizard,
            "custom" => CanvasType::Custom,
            _ => CanvasType::Custom,
        }
    }
}

impl std::fmt::Display for CanvasType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── sqlx trait implementations (map to VARCHAR/TEXT, not PG enum) ─────────

impl sqlx::Type<sqlx::Postgres> for CanvasType {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for CanvasType {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(CanvasType::from_str(&s))
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for CanvasType {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        <&str as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

/// Canvas struct (database model)
///
/// Maps to the `canvases` table. Uses explicit column selection (not SELECT *)
/// because the table has many columns we don't need in every query.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Canvas {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub html_content: Option<String>,
    pub js_content: Option<String>,
    pub css_content: Option<String>,
    pub canvas_type: CanvasType,
    pub data_sources: Option<JsonValue>,
    pub actions: Option<JsonValue>,
    pub layout_config: Option<JsonValue>,
    pub version: i32,
    pub is_public: bool,
    pub public_slug: Option<String>,
    pub is_system: bool,
    pub nav_icon: Option<String>,
    pub nav_order: i32,
    pub metadata: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Canvas template (reusable templates)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CanvasTemplate {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub canvas_type: CanvasType,
    pub html_content: Option<String>,
    pub js_content: Option<String>,
    pub css_content: Option<String>,
    pub metadata: Option<JsonValue>,
    pub version: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Canvas response sent to the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasResponse {
    /// Canvas type
    pub type_name: String,

    /// Canvas title
    pub title: String,

    /// Rendered HTML content
    pub content: String,

    /// JavaScript content
    pub js_content: Option<String>,

    /// CSS content
    pub css_content: Option<String>,

    /// Additional data for the canvas
    #[serde(flatten)]
    pub data: CanvasData,
}

/// Additional canvas data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasData {
    /// Module slug (if applicable)
    pub module_slug: Option<String>,

    /// Canvas slug
    pub canvas_slug: String,

    /// UI mode
    pub ui_mode: String,

    /// Available actions
    pub actions: Option<JsonValue>,

    /// Data sources configuration
    pub data_sources: Option<JsonValue>,

    /// Layout configuration
    pub layout_config: Option<JsonValue>,

    /// Canvas metadata
    pub metadata: Option<JsonValue>,
}

/// Data source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    /// Source type (e.g., "model", "api", "static")
    pub source_type: String,

    /// Model name (if source_type is "model")
    pub model_name: Option<String>,

    /// Scope/query parameters
    pub scope: Option<JsonValue>,

    /// Limit on number of records
    pub limit: Option<i32>,

    /// Filters to apply
    pub filters: Option<JsonValue>,

    /// Additional configuration
    pub config: Option<JsonValue>,
}

/// Action configuration for canvas buttons/interactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasAction {
    /// Unique action key
    pub key: String,

    /// Display label
    pub label: String,

    /// Action type (e.g., "create", "update", "delete", "execute")
    pub action_type: String,

    /// Target (e.g., model name, API endpoint)
    pub target: Option<String>,

    /// Parameters for the action
    pub params: Option<JsonValue>,

    /// Icon name (Lucide icon)
    pub icon: Option<String>,

    /// Button style/variant
    pub variant: Option<String>,

    /// Confirmation message before executing
    pub confirm: Option<String>,
}

impl CanvasResponse {
    /// Create a new canvas response
    pub fn new(
        canvas: &Canvas,
        rendered_html: String,
        module_slug: Option<String>,
        ui_mode: String,
    ) -> Self {
        Self {
            type_name: canvas.canvas_type.to_string(),
            title: canvas.name.clone(),
            content: rendered_html,
            js_content: canvas.js_content.clone(),
            css_content: canvas.css_content.clone(),
            data: CanvasData {
                module_slug,
                canvas_slug: canvas.slug.clone(),
                ui_mode,
                actions: canvas.actions.clone(),
                data_sources: canvas.data_sources.clone(),
                layout_config: canvas.layout_config.clone(),
                metadata: canvas.metadata.clone(),
            },
        }
    }

    /// Create a freeform canvas response with iframe.
    ///
    /// SECURE-005: three layers of defense against XSS in user-/agent-
    /// authored canvas content:
    ///
    /// 1. `canvas.html_content` is passed through [`crate::html_sanitizer`]
    ///    before being emitted into the iframe body. Inline event
    ///    handlers, `<script>` tags, `javascript:` URLs, and other XSS
    ///    vectors are stripped.
    ///
    /// 2. A per-response CSP nonce is generated via [`generate_csp_nonce`]
    ///    and applied to the `<meta http-equiv="Content-Security-Policy">`
    ///    tag plus every inline `<style>` / `<script>` tag emitted below.
    ///    Any attacker-injected inline tag that survives sanitization
    ///    will not carry the nonce and will be blocked by CSP.
    ///
    /// 3. `canvas.js_content` is emitted inside a `<script nonce=...>`
    ///    block. It is NOT sanitized — author-supplied JS is expected
    ///    to run, contained by the iframe sandbox (`sandbox=` on
    ///    `canvasFrame`) and the nonce-scoped CSP.
    ///
    /// `canvas.css_content` is wrapped in `<style nonce=...>` so it
    /// passes the style-src CSP directive; ammonia's allowlist does
    /// not cover stylesheets but the iframe sandbox prevents CSS from
    /// reaching the parent document.
    pub fn freeform(canvas: &Canvas) -> Self {
        let html_raw = canvas.html_content.as_deref().unwrap_or("");
        let js = canvas.js_content.as_deref().unwrap_or("");
        let css = canvas.css_content.as_deref().unwrap_or("");

        let html = crate::html_sanitizer::sanitize_html(html_raw);
        let nonce = generate_csp_nonce();
        let csp = canvas_meta_csp(&nonce);

        let content = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta http-equiv="Content-Security-Policy" content="{csp}">
    <style nonce="{nonce}">{css}</style>
</head>
<body>
    {html}
    <script nonce="{nonce}">{js}</script>
</body>
</html>"#,
        );

        Self {
            type_name: "freeform".to_string(),
            title: canvas.name.clone(),
            content,
            js_content: None,
            css_content: None,
            data: CanvasData {
                module_slug: None,
                canvas_slug: canvas.slug.clone(),
                ui_mode: "freeform".to_string(),
                actions: canvas.actions.clone(),
                data_sources: canvas.data_sources.clone(),
                layout_config: canvas.layout_config.clone(),
                metadata: canvas.metadata.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal Canvas struct with the given html/js/css for
    /// exercising `CanvasResponse::freeform`. Populates all required
    /// fields with sensible defaults.
    fn make_canvas(html: &str, js: &str, css: &str) -> Canvas {
        Canvas {
            id: Uuid::nil(),
            slug: "test-canvas".to_string(),
            name: "Test Canvas".to_string(),
            description: None,
            html_content: Some(html.to_string()),
            js_content: Some(js.to_string()),
            css_content: Some(css.to_string()),
            canvas_type: CanvasType::Freeform,
            data_sources: None,
            actions: None,
            layout_config: None,
            version: 1,
            is_public: false,
            public_slug: None,
            is_system: false,
            nav_icon: None,
            nav_order: 0,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // ── SECURE-005: freeform canvas HTML sanitization ────────────────
    //
    // The `CanvasResponse::freeform` constructor emits a full HTML
    // document that gets loaded into the canvas iframe. User-/agent-
    // authored HTML flows in via `canvas.html_content` and must be
    // passed through the allowlist sanitizer before being inlined into
    // the iframe body — otherwise `<script>` / inline event handlers /
    // `javascript:` URLs land directly in the rendered document.

    #[test]
    fn test_freeform_strips_script_tag_from_html() {
        let canvas = make_canvas("<p>hi</p><script>steal()</script>", "", "");
        let resp = CanvasResponse::freeform(&canvas);
        // The body's user-HTML section should no longer contain a raw
        // <script>steal()</script>. Note: the iframe shell itself emits
        // its own <script>{js}</script> for author JS, which is an empty
        // string here, so any <script> hit must come from the sanitized
        // html — which should have none.
        assert!(
            !resp.content.contains("<script>steal()</script>"),
            "script tag in user html leaked into iframe body: {}",
            resp.content
        );
        assert!(
            !resp.content.contains("steal()"),
            "script body leaked: {}",
            resp.content
        );
        assert!(resp.content.contains("<p>hi</p>"), "safe HTML stripped");
    }

    #[test]
    fn test_freeform_strips_inline_event_handler() {
        let canvas = make_canvas(r#"<img src="x" onerror="alert(1)">"#, "", "");
        let resp = CanvasResponse::freeform(&canvas);
        assert!(!resp.content.contains("onerror"), "onerror leaked");
        assert!(!resp.content.contains("alert(1)"), "payload leaked");
    }

    #[test]
    fn test_freeform_strips_javascript_url() {
        let canvas = make_canvas(r#"<a href="javascript:alert(1)">click</a>"#, "", "");
        let resp = CanvasResponse::freeform(&canvas);
        assert!(
            !resp.content.contains("javascript:"),
            "javascript: URL leaked: {}",
            resp.content
        );
    }

    #[test]
    fn test_freeform_strips_iframe_in_html() {
        let canvas = make_canvas(
            r#"<p>ok</p><iframe src="https://evil.example"></iframe>"#,
            "",
            "",
        );
        let resp = CanvasResponse::freeform(&canvas);
        // The outer iframe shell (`<html><head>...</head><body>...`) is
        // fine; an attacker-injected nested iframe inside user HTML is
        // what we need to strip.
        assert!(
            !resp
                .content
                .contains(r#"<iframe src="https://evil.example"#),
            "nested iframe leaked: {}",
            resp.content
        );
    }

    #[test]
    fn test_freeform_preserves_author_js() {
        // The author's `js_content` is intentionally NOT sanitized —
        // it's expected to run inside the sandboxed iframe.
        let canvas = make_canvas("<p>ok</p>", "console.log('author code');", "");
        let resp = CanvasResponse::freeform(&canvas);
        assert!(
            resp.content.contains("console.log('author code')"),
            "author js stripped: {}",
            resp.content
        );
    }

    #[test]
    fn test_freeform_preserves_safe_html() {
        let canvas = make_canvas("<h1>Hello</h1><p><strong>world</strong></p>", "", "");
        let resp = CanvasResponse::freeform(&canvas);
        assert!(resp.content.contains("<h1>Hello</h1>"));
        assert!(resp.content.contains("<strong>world</strong>"));
    }

    #[test]
    fn test_freeform_handles_empty_html() {
        let canvas = make_canvas("", "", "");
        let resp = CanvasResponse::freeform(&canvas);
        // Should still produce a valid document skeleton
        assert!(resp.content.contains("<!DOCTYPE html>"));
        assert!(resp.content.contains("<body>"));
    }

    #[test]
    fn test_freeform_handles_missing_html_content() {
        let mut canvas = make_canvas("", "", "");
        canvas.html_content = None;
        canvas.js_content = None;
        canvas.css_content = None;
        // Must not panic on absent content
        let resp = CanvasResponse::freeform(&canvas);
        assert!(resp.content.contains("<!DOCTYPE html>"));
    }

    // ── SECURE-005 nonces ─────────────────────────────────────────────

    /// Extract the first `nonce="..."` attribute value from the emitted
    /// freeform document. The document always opens with a `<meta>` CSP
    /// tag whose `content=...` attribute contains `'nonce-<value>'`
    /// before any `nonce="<value>"` tag attribute, so we skip past that
    /// first occurrence and grab the `<style nonce="...">` nonce.
    fn extract_nonce(content: &str) -> Option<String> {
        let marker = r#"nonce=""#;
        let start = content.find(marker)? + marker.len();
        let end = content[start..].find('"')? + start;
        Some(content[start..end].to_string())
    }

    #[test]
    fn test_generate_csp_nonce_is_unique() {
        // 128 bits of randomness — collision across two calls is
        // astronomically unlikely. If this ever fails, rand is broken.
        let a = generate_csp_nonce();
        let b = generate_csp_nonce();
        assert_ne!(a, b, "two consecutive nonces collided: {} == {}", a, b);
    }

    #[test]
    fn test_generate_csp_nonce_is_base64url() {
        let nonce = generate_csp_nonce();
        assert_eq!(
            nonce.len(),
            22,
            "128-bit base64url (no padding) = 22 chars, got {}",
            nonce.len()
        );
        assert!(
            nonce
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "nonce contains non-url-safe chars: {}",
            nonce
        );
        // base64url explicitly disallows + / = padding
        assert!(!nonce.contains('+'));
        assert!(!nonce.contains('/'));
        assert!(!nonce.contains('='));
    }

    #[test]
    fn test_freeform_emits_meta_csp() {
        let canvas = make_canvas("<p>ok</p>", "", "");
        let resp = CanvasResponse::freeform(&canvas);
        assert!(
            resp.content
                .contains(r#"<meta http-equiv="Content-Security-Policy""#),
            "meta CSP tag missing: {}",
            resp.content
        );
    }

    #[test]
    fn test_freeform_applies_same_nonce_to_style_and_script() {
        let canvas = make_canvas("<p>ok</p>", "console.log(1)", "body{color:red}");
        let resp = CanvasResponse::freeform(&canvas);
        let nonce = extract_nonce(&resp.content).expect("nonce missing");
        // Both the <style> (for css) and the <script> (for js) must
        // carry the same nonce so the meta CSP permits them.
        assert!(
            resp.content
                .contains(&format!(r#"<style nonce="{}">"#, nonce)),
            "style nonce missing: {}",
            resp.content
        );
        assert!(
            resp.content
                .contains(&format!(r#"<script nonce="{}">"#, nonce)),
            "script nonce missing: {}",
            resp.content
        );
        // And the CSP directive itself must authorise that exact nonce
        assert!(
            resp.content.contains(&format!("'nonce-{}'", nonce)),
            "CSP directive missing 'nonce-{}': {}",
            nonce,
            resp.content
        );
    }

    #[test]
    fn test_freeform_nonce_is_unique_per_call() {
        // Two invocations of `freeform` with identical input should
        // produce different nonces — this is what makes nonce-based CSP
        // resistant to replay of captured payloads.
        let canvas = make_canvas("<p>ok</p>", "", "");
        let a = CanvasResponse::freeform(&canvas);
        let b = CanvasResponse::freeform(&canvas);
        let na = extract_nonce(&a.content).unwrap();
        let nb = extract_nonce(&b.content).unwrap();
        assert_ne!(na, nb, "nonce reused across calls: {}", na);
    }

    #[test]
    fn test_freeform_csp_directive_keeps_real_protections() {
        // SECURE-005's original assertion was "no 'unsafe-inline'". That
        // turned out to break every interactive control on every existing
        // framework canvas in production (BYOK toggle, login, settings —
        // 2026-04-30 incident). We're allowing 'unsafe-inline' inside the
        // sandboxed iframe again until a multi-canvas refactor lands.
        //
        // What MUST stay locked down is what actually limits cross-origin
        // damage: object-src, frame-ancestors, connect-src, base-uri,
        // form-action, and the absence of 'unsafe-eval'. This test guards
        // those.
        let canvas = make_canvas("<p>ok</p>", "", "");
        let resp = CanvasResponse::freeform(&canvas);
        let marker = r#"Content-Security-Policy" content=""#;
        let start = resp.content.find(marker).unwrap() + marker.len();
        let end = resp.content[start..].find('"').unwrap() + start;
        let csp = &resp.content[start..end];
        assert!(
            !csp.contains("'unsafe-eval'"),
            "freeform meta CSP must not include 'unsafe-eval': {}",
            csp
        );
        assert!(csp.contains("object-src 'none'"));
        assert!(csp.contains("frame-ancestors 'self'"));
        assert!(csp.contains("connect-src 'self'"));
        assert!(csp.contains("base-uri 'self'"));
        assert!(csp.contains("form-action 'self'"));
    }

    #[test]
    fn test_canvas_meta_csp_includes_nonce_in_script_and_style() {
        let csp = canvas_meta_csp("abc123");
        assert!(csp.contains("'nonce-abc123'"), "nonce missing: {}", csp);
        let script_idx = csp.find("script-src").expect("no script-src");
        let style_idx = csp.find("style-src").expect("no style-src");
        let script_end = csp[script_idx..].find(';').unwrap() + script_idx;
        // style-src is followed by ';' before any later directive
        let style_end = csp[style_idx..].find(';').unwrap() + style_idx;
        assert!(csp[script_idx..script_end].contains("'nonce-abc123'"));
        assert!(csp[style_idx..style_end].contains("'nonce-abc123'"));
    }

    #[test]
    fn test_canvas_meta_csp_allows_inline_for_existing_canvases() {
        // Regression guard: SECURE-005 removed 'unsafe-inline' which broke
        // every interactive control on every existing framework canvas
        // (BYOK toggle, login buttons, etc.) in production 2026-04-30.
        // Re-allowed here while a multi-canvas refactor is underway. If
        // someone tightens this back without rewriting the canvases, this
        // test will fail loud rather than letting prod break silently.
        let csp = canvas_meta_csp("abc123");
        let script_idx = csp.find("script-src").expect("no script-src");
        let script_end = csp[script_idx..].find(';').unwrap() + script_idx;
        let style_idx = csp.find("style-src").expect("no style-src");
        let style_end = csp[style_idx..].find(';').unwrap() + style_idx;
        assert!(
            csp[script_idx..script_end].contains("'unsafe-inline'"),
            "script-src must allow 'unsafe-inline' until canvas refactor lands"
        );
        assert!(
            csp[style_idx..style_end].contains("'unsafe-inline'"),
            "style-src must allow 'unsafe-inline' until canvas refactor lands"
        );
    }
}
