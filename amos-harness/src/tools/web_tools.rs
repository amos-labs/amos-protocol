//! Web access tools

use super::{Tool, ToolCategory, ToolResult};
use amos_core::{AppConfig, Result};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::net::IpAddr;
use std::sync::Arc;

/// Check if an IP address is in a private/internal range
fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Validate a URL is safe to fetch (prevents SSRF)
async fn validate_url_safe(raw_url: &str) -> std::result::Result<(), String> {
    let parsed = url::Url::parse(raw_url).map_err(|e| format!("Invalid URL: {}", e))?;

    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(format!("URL scheme '{}' not allowed", s)),
    }

    let host = parsed.host_str().ok_or("URL has no host")?;
    let blocked_hosts = ["localhost", "0.0.0.0", "[::]", "[::1]"];
    if blocked_hosts.contains(&host) || host.ends_with(".local") || host.ends_with(".internal") {
        return Err(format!("Host '{}' is not allowed", host));
    }

    // Resolve DNS and check for private IPs
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addr_str = format!("{}:{}", host, port);
    match tokio::net::lookup_host(&addr_str).await {
        Ok(addrs) => {
            for addr in addrs {
                if is_blocked_ip(addr.ip()) {
                    return Err(format!("URL resolves to blocked IP: {}", addr.ip()));
                }
            }
        }
        Err(e) => return Err(format!("DNS resolution failed: {}", e)),
    }

    Ok(())
}

/// Search the web
pub struct WebSearchTool {
    config: Arc<AppConfig>,
}

impl WebSearchTool {
    pub fn new(config: Arc<AppConfig>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information using a search engine"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Number of results to return",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("query is required".to_string()))?;

        let num_results = params
            .get("num_results")
            .and_then(|v| v.as_i64())
            .unwrap_or(5)
            .min(10) as usize;

        // Read Brave API key from environment
        let api_key = std::env::var("BRAVE_API_KEY").map_err(|_| {
            amos_core::AmosError::Internal(
                "Web search unavailable: BRAVE_API_KEY not configured".to_string(),
            )
        })?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| {
                amos_core::AmosError::Internal(format!("Failed to build HTTP client: {e}"))
            })?;
        let response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", &num_results.to_string())])
            .send()
            .await
            .map_err(|e| {
                amos_core::AmosError::Internal(format!("Brave Search request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(amos_core::AmosError::Internal(format!(
                "Brave Search API error {status}: {body}"
            )));
        }

        let search_result: JsonValue = response.json().await.map_err(|e| {
            amos_core::AmosError::Internal(format!("Failed to parse Brave Search response: {e}"))
        })?;

        let results: Vec<JsonValue> = search_result
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|r| {
                        let snippet = r.get("description").and_then(|d| d.as_str()).unwrap_or("");
                        // Truncate snippets to keep search results lightweight
                        let snippet = if snippet.len() > 300 {
                            &snippet[..300]
                        } else {
                            snippet
                        };
                        json!({
                            "title": r.get("title").and_then(|t| t.as_str()).unwrap_or(""),
                            "url": r.get("url").and_then(|u| u.as_str()).unwrap_or(""),
                            "snippet": snippet,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ToolResult::success(json!({
            "query": query,
            "results": results,
            "count": results.len()
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Web
    }
}

/// Fetch and parse a web page
pub struct ViewWebPageTool;

impl Default for ViewWebPageTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewWebPageTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ViewWebPageTool {
    fn name(&self) -> &str {
        "view_web_page"
    }

    fn description(&self) -> &str {
        "Fetch web pages or call external APIs. Supports GET/POST/PUT/PATCH/DELETE with custom headers and body. Use extract_format 'raw' for API JSON responses. Never use bash curl — use this tool instead."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"],
                    "description": "HTTP method (default: GET)",
                    "default": "GET"
                },
                "headers": {
                    "type": "object",
                    "description": "Custom HTTP headers as key-value pairs (e.g. {\"Authorization\": \"Bearer token\", \"Content-Type\": \"application/json\"})"
                },
                "body": {
                    "type": "string",
                    "description": "Request body (for POST/PUT/PATCH). Send JSON as a string."
                },
                "extract_format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html", "raw"],
                    "description": "Format to extract content in. Use 'raw' for API JSON responses.",
                    "default": "text"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| amos_core::AmosError::Validation("url is required".to_string()))?;

        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();

        let extract_format = params
            .get("extract_format")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        // Validate URL to prevent SSRF
        validate_url_safe(url)
            .await
            .map_err(|e| amos_core::AmosError::Validation(format!("URL blocked: {}", e)))?;

        // Fetch the web page — disable automatic redirects so we can validate each hop
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Mozilla/5.0 (compatible; AMOS/1.0; +https://amoslabs.com)")
            .build()
            .map_err(|e| {
                amos_core::AmosError::Internal(format!("Failed to build HTTP client: {}", e))
            })?;

        let mut request = match method.as_str() {
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "PATCH" => client.patch(url),
            "DELETE" => client.delete(url),
            _ => client.get(url),
        };

        // Apply custom headers
        if let Some(headers) = params.get("headers").and_then(|v| v.as_object()) {
            for (key, value) in headers {
                if let Some(val) = value.as_str() {
                    request = request.header(key.as_str(), val);
                }
            }
        }

        // Apply request body
        if let Some(body) = params.get("body").and_then(|v| v.as_str()) {
            request = request.body(body.to_string());
        }

        let mut response: reqwest::Response = request.send().await.map_err(|e| {
            amos_core::AmosError::Internal(format!("External: Failed to fetch URL: {}", e))
        })?;

        // Manually follow redirects with SSRF validation on each hop.
        // This prevents TOCTOU attacks where the redirect target is an internal IP.
        let max_redirects = 5;
        for _ in 0..max_redirects {
            if !response.status().is_redirection() {
                break;
            }
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            let Some(redirect_url) = location else {
                break;
            };
            // Resolve relative URLs against the current response URL
            let resolved = response
                .url()
                .join(&redirect_url)
                .map_err(|e| {
                    amos_core::AmosError::Validation(format!("Invalid redirect URL: {}", e))
                })?
                .to_string();
            // Validate the redirect target BEFORE following it
            validate_url_safe(&resolved).await.map_err(|e| {
                amos_core::AmosError::Validation(format!("Redirect target blocked: {}", e))
            })?;
            response = client.get(&resolved).send().await.map_err(|e| {
                amos_core::AmosError::Internal(format!(
                    "External: Failed to follow redirect: {}",
                    e
                ))
            })?;
        }

        let status_code = response.status().as_u16();
        let html = response.text().await.map_err(|e| {
            amos_core::AmosError::Internal(format!("External: Failed to read response body: {}", e))
        })?;

        // Extract content based on format
        let content = match extract_format {
            "html" => html.clone(),
            "raw" => html.clone(), // Return raw response (useful for API JSON)
            "markdown" => {
                // TODO: Convert HTML to markdown
                // For now, just strip tags
                strip_html_tags(&html)
            }
            _ => {
                // Extract text
                strip_html_tags(&html)
            }
        };

        // Cap content at 12,000 chars to prevent context window overflow.
        // The agent loop has a 15K backstop, but we truncate here so the
        // tool itself is well-behaved and the LLM gets a clean message.
        const MAX_PAGE_CHARS: usize = 12_000;
        let content = if content.len() > MAX_PAGE_CHARS {
            format!(
                "{}\n\n[... content truncated — page was {} chars, showing first {}]",
                &content[..MAX_PAGE_CHARS],
                content.len(),
                MAX_PAGE_CHARS
            )
        } else {
            content
        };

        Ok(ToolResult::success(json!({
            "url": url,
            "status_code": status_code,
            "content": content,
            "format": extract_format
        })))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Web
    }
}

/// Simple HTML tag stripper
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags() {
        let html = "<p>Hello <strong>world</strong>!</p>";
        let text = strip_html_tags(html);
        assert_eq!(text, "Hello world!");
    }
}
