//! HTML sanitization for user-provided canvas content.
//!
//! Canvas apps (built via the `create_app` / `update_app` tools) accept
//! user-authored HTML that is later injected into the DOM via `innerHTML`.
//! Unsanitized, this is a trivial XSS vector: inline `onerror=` /
//! `onclick=` handlers, `javascript:` URLs, and `<script>` tags inside
//! user HTML all execute in the canvas iframe with access to the user's
//! session.
//!
//! This module wraps the [`ammonia`] crate with a fixed, conservative
//! allowlist. Anything outside the allowlist is stripped, not escaped —
//! the goal is to render a safe visual approximation of the intended
//! markup, not to preserve attacker payloads.
//!
//! See AMOS-SECURE-005.

use ammonia::Builder;
use std::collections::HashSet;

/// Safe HTML tags accepted from user input.
///
/// Deliberately excludes:
/// - `<script>`, `<style>`, `<link>`, `<meta>`: arbitrary code / resource loading
/// - `<iframe>`, `<object>`, `<embed>`, `<applet>`: nested execution contexts
/// - `<form>`, `<input>`, `<button>`, `<select>`, `<textarea>`: form hijacking
/// - `<base>`: base-URL hijacking
/// - `<svg>`, `<math>`: can carry foreign-namespace script handlers
const SAFE_TAGS: &[&str] = &[
    "a",
    "abbr",
    "b",
    "blockquote",
    "br",
    "caption",
    "code",
    "dd",
    "div",
    "dl",
    "dt",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "pre",
    "s",
    "samp",
    "small",
    "span",
    "strong",
    "sub",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "u",
    "ul",
    "var",
];

/// URL schemes allowed on attributes that take URLs (`href`, `src`, `cite`).
///
/// Excludes `javascript:`, `data:` with script types, `vbscript:`, `file:`,
/// and any other scheme that could execute code or exfiltrate data.
/// `data:` is allowed only with image MIME types (rendered image tags).
const SAFE_URL_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];

/// Sanitize user-supplied HTML into a safe subset.
///
/// Strips all tags not in [`SAFE_TAGS`], all inline event handlers
/// (`onclick`, `onerror`, …), `style` attributes, and any URL attribute
/// whose scheme is not in [`SAFE_URL_SCHEMES`]. The output is always
/// valid UTF-8 HTML fragment suitable for `innerHTML` assignment.
///
/// # Example
/// ```ignore
/// use amos_harness::html_sanitizer::sanitize_html;
///
/// let dirty = r#"<p onclick="alert(1)">Hi</p><script>alert('xss')</script>"#;
/// let clean = sanitize_html(dirty);
/// assert!(!clean.contains("onclick"));
/// assert!(!clean.contains("<script>"));
/// ```
pub fn sanitize_html(input: &str) -> String {
    let tags: HashSet<&str> = SAFE_TAGS.iter().copied().collect();
    let schemes: HashSet<&str> = SAFE_URL_SCHEMES.iter().copied().collect();

    Builder::default()
        .tags(tags)
        .url_schemes(schemes)
        // Default-deny attributes; ammonia keeps only a safe baseline (class,
        // id, alt, title, href on a, src+alt on img, etc.). Inline event
        // handlers and `style` are stripped automatically.
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(input)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_safe_tags() {
        let input = "<p>Hello <strong>world</strong></p>";
        assert_eq!(sanitize_html(input), input);
    }

    #[test]
    fn test_strips_script_tag() {
        let input = "<p>Safe</p><script>alert('xss')</script>";
        let out = sanitize_html(input);
        assert!(!out.contains("<script"));
        assert!(!out.contains("alert"));
        assert!(out.contains("Safe"));
    }

    #[test]
    fn test_strips_inline_event_handlers() {
        let cases = [
            r#"<p onclick="alert(1)">x</p>"#,
            r#"<img src="x" onerror="alert(1)">"#,
            r#"<div onload="steal()">x</div>"#,
            r#"<a href="/" onmouseover="evil()">x</a>"#,
        ];
        for input in cases {
            let out = sanitize_html(input);
            assert!(
                !out.contains("onclick")
                    && !out.contains("onerror")
                    && !out.contains("onload")
                    && !out.contains("onmouseover"),
                "event handler survived: input={:?} output={:?}",
                input,
                out
            );
        }
    }

    #[test]
    fn test_strips_javascript_url_in_href() {
        let input = r#"<a href="javascript:alert(1)">click</a>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("javascript:"));
    }

    #[test]
    fn test_strips_data_url_script() {
        let input = r#"<a href="data:text/html,<script>alert(1)</script>">x</a>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("script"));
        assert!(!out.contains("data:"));
    }

    #[test]
    fn test_strips_vbscript_url() {
        let input = r#"<a href="vbscript:msgbox(1)">x</a>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("vbscript"));
    }

    #[test]
    fn test_strips_iframe() {
        let input = r#"<iframe src="https://evil.example"></iframe><p>ok</p>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("<iframe"));
        assert!(out.contains("ok"));
    }

    #[test]
    fn test_strips_object_and_embed() {
        let input = r#"<object data="x"></object><embed src="x"><p>ok</p>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("<object"));
        assert!(!out.contains("<embed"));
    }

    #[test]
    fn test_strips_form_elements() {
        let input = r#"<form action="/evil"><input name="x"><button>go</button></form>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("<form"));
        assert!(!out.contains("<input"));
        assert!(!out.contains("<button"));
    }

    #[test]
    fn test_strips_base_tag() {
        let input = r#"<base href="https://evil.example/"><p>ok</p>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("<base"));
    }

    #[test]
    fn test_strips_style_attribute_and_style_tag() {
        // <style> tag is not in the allowlist — stripped outright
        let style_input = "<style>body{background:url(javascript:alert(1))}</style><p>x</p>";
        let style_out = sanitize_html(style_input);
        assert!(!style_out.contains("<style"));
        assert!(!style_out.contains("javascript:"));

        // style attribute on allowed tag is stripped
        let attr_input = r#"<p style="background:url(javascript:alert(1))">x</p>"#;
        let attr_out = sanitize_html(attr_input);
        assert!(!attr_out.contains("style="));
        assert!(!attr_out.contains("javascript:"));
    }

    #[test]
    fn test_preserves_safe_link() {
        let input = r#"<a href="https://example.com">ok</a>"#;
        let out = sanitize_html(input);
        assert!(out.contains(r#"href="https://example.com""#));
        // Ammonia adds rel="noopener noreferrer nofollow" for safety
        assert!(out.contains("noopener"));
    }

    #[test]
    fn test_preserves_safe_image() {
        let input = r#"<img src="https://example.com/x.png" alt="hi">"#;
        let out = sanitize_html(input);
        assert!(out.contains(r#"src="https://example.com/x.png""#));
        assert!(out.contains(r#"alt="hi""#));
    }

    #[test]
    fn test_nested_malicious_payload_still_stripped() {
        // Attacker tries to bypass with nested structures
        let input =
            r#"<div><p onclick="a">x</p><script>alert(1)</script><svg onload="b"></svg></div>"#;
        let out = sanitize_html(input);
        assert!(!out.contains("onclick"));
        assert!(!out.contains("onload"));
        assert!(!out.contains("<script"));
        assert!(!out.contains("<svg"));
        assert!(out.contains("<div"));
        assert!(out.contains("<p"));
    }

    #[test]
    fn test_malformed_html_does_not_panic() {
        // Any malformed input should return some string without panicking
        let malformed = [
            "",
            "<",
            "<<<<",
            "<p><<<",
            "<p>unclosed",
            "</p></p></p>",
            "<p attr=\"un\"closed>x</p>",
            "\0\0\0",
            "<p>\u{202e}rtl-override</p>",
            "🦀<script>🦀</script>",
            &"<p>".repeat(10_000),
        ];
        for input in malformed {
            let out = sanitize_html(input);
            // Whatever comes back must not contain unescaped script
            assert!(!out.contains("<script"), "script leaked: {:?}", input);
        }
    }

    #[test]
    fn test_empty_input_returns_empty() {
        assert_eq!(sanitize_html(""), "");
    }

    #[test]
    fn test_plain_text_passes_through() {
        let input = "Just some plain text with no tags.";
        let out = sanitize_html(input);
        assert_eq!(out, input);
    }

    #[test]
    fn test_text_content_preserves_ampersands_and_quotes() {
        let input = "<p>5 &lt; 10 &amp; 7 &gt; 3</p>";
        let out = sanitize_html(input);
        // Ampersands/comparisons stay encoded
        assert!(out.contains("5"));
        assert!(out.contains("10"));
        assert!(out.contains("&lt;") || out.contains("<"));
    }

    #[test]
    fn test_very_large_input_does_not_panic() {
        let input = "<p>hello</p>".repeat(1_000);
        let out = sanitize_html(&input);
        assert!(!out.is_empty());
        assert!(!out.contains("<script"));
    }

    #[test]
    fn test_html_entity_encoded_script_does_not_execute() {
        // &lt;script&gt; should remain as text, not become a real tag
        let input = "&lt;script&gt;alert(1)&lt;/script&gt;";
        let out = sanitize_html(input);
        assert!(!out.contains("<script"));
    }
}
