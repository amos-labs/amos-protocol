//! App tools — create interactive, multi-view applications backed by canvases.
//!
//! An "app" is a freeform canvas where `canvas.metadata.app_config` stores
//! structured view configs. The harness renders the app HTML server-side
//! from those configs, using AMOS components (DataTable, KanbanBoard, etc.).

use super::{Tool, ToolCategory, ToolResult};
use crate::canvas::{CanvasEngine, CanvasType, CanvasUpdate};
use amos_core::{AmosError, AppConfig, Result};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::Arc;

// ── CreateApp ─────────────────────────────────────────────────────────────

/// Create a multi-view interactive application backed by a freeform canvas.
pub struct CreateAppTool {
    db_pool: PgPool,
}

impl CreateAppTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for CreateAppTool {
    fn name(&self) -> &str {
        "create_app"
    }

    fn description(&self) -> &str {
        "Create an interactive multi-view application (CRM, dashboard, project tracker, etc.) \
         backed by AMOS collections. Each view uses an AMOS component: data_table, kanban, \
         dashboard, form, chart, or custom. Always create the underlying collections first, \
         then call this tool with the view configs."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "App name (e.g. 'Acme CRM')"
                },
                "slug": {
                    "type": "string",
                    "description": "URL slug (lowercase, hyphens). Example: 'acme-crm'"
                },
                "description": {
                    "type": "string",
                    "description": "What this app does"
                },
                "icon": {
                    "type": "string",
                    "description": "Lucide icon name for the app (e.g. 'users', 'bar-chart-3', 'kanban')"
                },
                "theme": {
                    "type": "object",
                    "description": "Theme config: {\"primary\": \"#4f46e5\", \"sidebar_bg\": \"#1e1b4b\", \"sidebar_text\": \"#e0e7ff\"}",
                    "properties": {
                        "primary": { "type": "string" },
                        "sidebar_bg": { "type": "string" },
                        "sidebar_text": { "type": "string" }
                    }
                },
                "views": {
                    "type": "array",
                    "description": "Array of view configurations",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "View display name" },
                            "slug": { "type": "string", "description": "View URL slug" },
                            "icon": { "type": "string", "description": "Lucide icon name" },
                            "view_type": {
                                "type": "string",
                                "enum": ["data_table", "dashboard", "kanban", "form", "chart", "custom"],
                                "description": "Component type for this view"
                            },
                            "is_default": { "type": "boolean", "description": "Show this view on load" },
                            "component_config": {
                                "type": "object",
                                "description": "Config passed to the AMOS component. For data_table: {collection, columns, page_size}. For kanban: {collection, group_field, card_title_field}. For dashboard: {widgets: [{type, collection, ...}]}. For chart: {collection, chart_type, x_field, y_field}. For form: {collection, fields}. For custom: {html, css, js}."
                            }
                        },
                        "required": ["name", "slug", "view_type", "component_config"]
                    }
                }
            },
            "required": ["name", "slug", "views"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| AmosError::Validation("name is required".to_string()))?;

        let slug = params["slug"]
            .as_str()
            .ok_or_else(|| AmosError::Validation("slug is required".to_string()))?;

        let description = params.get("description").and_then(|v| v.as_str());
        let icon = params
            .get("icon")
            .and_then(|v| v.as_str())
            .unwrap_or("layout-grid");
        let theme = params.get("theme").cloned().unwrap_or_else(|| {
            json!({
                "primary": "#4f46e5",
                "sidebar_bg": "#1e1b4b",
                "sidebar_text": "#e0e7ff"
            })
        });

        let views = params["views"]
            .as_array()
            .ok_or_else(|| AmosError::Validation("views array is required".to_string()))?;

        if views.is_empty() {
            return Err(AmosError::Validation(
                "At least one view is required".to_string(),
            ));
        }

        // Build the app_config that goes into canvas metadata
        let app_config = json!({
            "slug": slug,
            "icon": icon,
            "theme": theme,
            "views": views,
        });

        // Render the app HTML
        let (html, js, css) = render_app_html(name, &app_config);

        // Store as a freeform canvas with app_config in metadata
        let metadata = json!({ "app_config": app_config });

        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config);

        // Use slug as name hint for canvas slug generation
        let canvas = engine
            .create_canvas_with_metadata(
                format!("App: {}", name),
                description.map(String::from),
                CanvasType::Freeform,
                html,
                Some(js),
                Some(css),
                None,
                None,
                None,
                Some(metadata),
            )
            .await?;

        Ok(ToolResult::success_with_metadata(
            json!({
                "canvas_id": canvas.id.to_string(),
                "slug": canvas.slug,
                "name": name,
                "view_count": views.len(),
                "message": format!("App '{}' created with {} views. Preview loading.", name, views.len())
            }),
            json!({
                "__canvas_action": "preview_app",
                "canvas_id": canvas.id.to_string(),
                "slug": canvas.slug,
                "app_name": name
            }),
        ))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Apps
    }
}

// ── UpdateAppView ─────────────────────────────────────────────────────────

/// Update or add a view in an existing app.
pub struct UpdateAppViewTool {
    db_pool: PgPool,
}

impl UpdateAppViewTool {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl Tool for UpdateAppViewTool {
    fn name(&self) -> &str {
        "update_app_view"
    }

    fn description(&self) -> &str {
        "Update or add a view in an existing app. If a view with the given slug exists, it is \
         updated. Otherwise a new view is added."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "app_slug": {
                    "type": "string",
                    "description": "Canvas slug of the app to update"
                },
                "view_slug": {
                    "type": "string",
                    "description": "Slug of the view to update or create"
                },
                "name": {
                    "type": "string",
                    "description": "View display name (required for new views)"
                },
                "icon": {
                    "type": "string",
                    "description": "Lucide icon name"
                },
                "view_type": {
                    "type": "string",
                    "enum": ["data_table", "dashboard", "kanban", "form", "chart", "custom"],
                    "description": "Component type (required for new views)"
                },
                "is_default": {
                    "type": "boolean",
                    "description": "Set this view as the default"
                },
                "component_config": {
                    "type": "object",
                    "description": "Updated component config"
                }
            },
            "required": ["app_slug", "view_slug"]
        })
    }

    async fn execute(&self, params: JsonValue) -> Result<ToolResult> {
        let app_slug = params["app_slug"]
            .as_str()
            .ok_or_else(|| AmosError::Validation("app_slug is required".to_string()))?;

        let view_slug = params["view_slug"]
            .as_str()
            .ok_or_else(|| AmosError::Validation("view_slug is required".to_string()))?;

        let config = Arc::new(AppConfig::load()?);
        let engine = CanvasEngine::new(self.db_pool.clone(), config);

        // Load existing canvas
        let canvas = engine.get_canvas_by_slug(app_slug).await?;

        // Parse app_config from metadata
        let metadata = canvas.metadata.as_ref().ok_or_else(|| {
            AmosError::Validation("Canvas has no app_config metadata".to_string())
        })?;

        let mut app_config = metadata
            .get("app_config")
            .cloned()
            .ok_or_else(|| AmosError::Validation("Canvas is not an app".to_string()))?;

        let views = app_config
            .get_mut("views")
            .and_then(|v| v.as_array_mut())
            .ok_or_else(|| AmosError::Internal("app_config.views is not an array".to_string()))?;

        // Find existing view or create new entry
        let existing_idx = views
            .iter()
            .position(|v| v.get("slug").and_then(|s| s.as_str()) == Some(view_slug));

        if let Some(idx) = existing_idx {
            // Update existing view fields
            let view = &mut views[idx];
            if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
                view["name"] = json!(name);
            }
            if let Some(icon) = params.get("icon").and_then(|v| v.as_str()) {
                view["icon"] = json!(icon);
            }
            if let Some(vt) = params.get("view_type").and_then(|v| v.as_str()) {
                view["view_type"] = json!(vt);
            }
            if let Some(is_default) = params.get("is_default").and_then(|v| v.as_bool()) {
                view["is_default"] = json!(is_default);
            }
            if let Some(cc) = params.get("component_config") {
                view["component_config"] = cc.clone();
            }
        } else {
            // Add new view — require name and view_type
            let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                AmosError::Validation("name is required for new views".to_string())
            })?;
            let view_type = params
                .get("view_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AmosError::Validation("view_type is required for new views".to_string())
                })?;

            let mut new_view = json!({
                "name": name,
                "slug": view_slug,
                "view_type": view_type,
                "component_config": params.get("component_config").cloned().unwrap_or(json!({}))
            });
            if let Some(icon) = params.get("icon").and_then(|v| v.as_str()) {
                new_view["icon"] = json!(icon);
            }
            if let Some(is_default) = params.get("is_default").and_then(|v| v.as_bool()) {
                new_view["is_default"] = json!(is_default);
            }
            views.push(new_view);
        }

        // Extract app name from canvas name (strip "App: " prefix)
        let app_name = canvas.name.strip_prefix("App: ").unwrap_or(&canvas.name);

        // Re-render
        let (html, js, css) = render_app_html(app_name, &app_config);

        let new_metadata = json!({ "app_config": app_config });

        let updates = CanvasUpdate {
            html_content: Some(html),
            js_content: Some(js),
            css_content: Some(css),
            metadata: Some(new_metadata),
            ..Default::default()
        };

        let updated = engine.update_canvas(canvas.id, updates).await?;

        let action = if existing_idx.is_some() {
            "updated"
        } else {
            "added"
        };

        Ok(ToolResult::success_with_metadata(
            json!({
                "canvas_id": updated.id.to_string(),
                "slug": updated.slug,
                "view_slug": view_slug,
                "action": action,
                "message": format!("View '{}' {} in app", view_slug, action)
            }),
            json!({
                "__canvas_action": "preview_app",
                "canvas_id": updated.id.to_string(),
                "slug": updated.slug,
                "app_name": app_name
            }),
        ))
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Apps
    }
}

// ── render_app_html ───────────────────────────────────────────────────────

/// Render a complete app HTML document from an app_config.
///
/// Returns (html_content, js_content, css_content).
fn render_app_html(app_name: &str, app_config: &JsonValue) -> (String, String, String) {
    let icon = app_config
        .get("icon")
        .and_then(|v| v.as_str())
        .unwrap_or("layout-grid");

    let theme = app_config.get("theme").cloned().unwrap_or(json!({}));
    let primary = theme
        .get("primary")
        .and_then(|v| v.as_str())
        .unwrap_or("#4f46e5");
    let sidebar_bg = theme
        .get("sidebar_bg")
        .and_then(|v| v.as_str())
        .unwrap_or("#1e1b4b");
    let sidebar_text = theme
        .get("sidebar_text")
        .and_then(|v| v.as_str())
        .unwrap_or("#e0e7ff");

    let views = app_config
        .get("views")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Build sidebar nav items
    let mut nav_items = String::new();
    for view in &views {
        let v_name = view.get("name").and_then(|v| v.as_str()).unwrap_or("View");
        let v_slug = view.get("slug").and_then(|v| v.as_str()).unwrap_or("view");
        let v_icon = view
            .get("icon")
            .and_then(|v| v.as_str())
            .unwrap_or("file-text");
        let is_default = view
            .get("is_default")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let active_class = if is_default { " active" } else { "" };
        nav_items.push_str(&format!(
            "<a href=\"#\" class=\"nav-link{active_class}\" data-view=\"{v_slug}\" onclick=\"switchView('{v_slug}'); return false;\">\n    <i data-lucide=\"{v_icon}\"></i>\n    <span>{v_name}</span>\n</a>\n"
        ));
    }

    // If no view is marked default, mark the first one
    let default_slug = views
        .iter()
        .find(|v| {
            v.get("is_default")
                .and_then(|d| d.as_bool())
                .unwrap_or(false)
        })
        .or(views.first())
        .and_then(|v| v.get("slug"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Build view containers
    let mut view_divs = String::new();
    let mut init_scripts = String::new();

    for view in &views {
        let v_slug = view.get("slug").and_then(|v| v.as_str()).unwrap_or("view");
        let v_type = view
            .get("view_type")
            .and_then(|v| v.as_str())
            .unwrap_or("custom");
        let config = view.get("component_config").cloned().unwrap_or(json!({}));
        let hidden = if v_slug == default_slug {
            ""
        } else {
            " hidden"
        };

        view_divs.push_str(&format!(
            r#"<div id="view-{v_slug}" class="view-container{hidden}"></div>
"#
        ));

        let config_str = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());

        match v_type {
            "data_table" => {
                init_scripts.push_str(&format!(
                    r#"viewComponents['{v_slug}'] = new AMOS.DataTable(document.getElementById('view-{v_slug}'), {config_str});
"#
                ));
            }
            "kanban" => {
                init_scripts.push_str(&format!(
                    r#"viewComponents['{v_slug}'] = new AMOS.KanbanBoard(document.getElementById('view-{v_slug}'), {config_str});
"#
                ));
            }
            "dashboard" => {
                // Dashboard: render a grid of MetricCard + Chart widgets
                init_scripts.push_str(&format!(
                    r#"(function() {{
    const container = document.getElementById('view-{v_slug}');
    const cfg = {config_str};
    const widgets = cfg.widgets || [];
    container.innerHTML = '<div class="dashboard-grid"></div>';
    const grid = container.querySelector('.dashboard-grid');
    widgets.forEach(function(w, i) {{
        const cell = document.createElement('div');
        cell.className = 'dashboard-widget';
        grid.appendChild(cell);
        if (w.type === 'metric') {{
            new AMOS.MetricCard(cell, w);
        }} else if (w.type === 'chart') {{
            new AMOS.Chart(cell, w);
        }}
    }});
}})();
"#
                ));
            }
            "form" => {
                init_scripts.push_str(&format!(
                    r#"viewComponents['{v_slug}'] = new AMOS.FormBuilder(document.getElementById('view-{v_slug}'), {config_str});
"#
                ));
            }
            "chart" => {
                init_scripts.push_str(&format!(
                    r#"viewComponents['{v_slug}'] = new AMOS.Chart(document.getElementById('view-{v_slug}'), {config_str});
"#
                ));
            }
            "custom" => {
                let custom_html = config.get("html").and_then(|v| v.as_str()).unwrap_or("");
                let custom_js = config.get("js").and_then(|v| v.as_str()).unwrap_or("");
                init_scripts.push_str(&format!(
                    r#"document.getElementById('view-{v_slug}').innerHTML = {custom_html_json};
{custom_js}
"#,
                    custom_html_json =
                        serde_json::to_string(custom_html).unwrap_or_else(|_| "\"\"".to_string()),
                ));
            }
            _ => {}
        }
    }

    let html = format!(
        r#"<div class="app-layout">
    <aside class="app-sidebar" id="appSidebar">
        <div class="sidebar-header">
            <i data-lucide="{icon}"></i>
            <h1>{app_name}</h1>
            <button class="sidebar-close" onclick="toggleSidebar()">
                <i data-lucide="x"></i>
            </button>
        </div>
        <nav class="sidebar-nav">
{nav_items}        </nav>
    </aside>
    <main class="app-main">
        <header class="app-topbar">
            <button class="hamburger" onclick="toggleSidebar()">
                <i data-lucide="menu"></i>
            </button>
            <h2 id="viewTitle">{app_name}</h2>
        </header>
        <div class="app-content" id="appContent">
{view_divs}        </div>
    </main>
</div>"#
    );

    let js = format!(
        r#"// App initialization
const viewComponents = {{}};

function switchView(slug) {{
    document.querySelectorAll('.view-container').forEach(el => el.classList.add('hidden'));
    const target = document.getElementById('view-' + slug);
    if (target) target.classList.remove('hidden');

    document.querySelectorAll('.nav-link').forEach(el => el.classList.remove('active'));
    const navLink = document.querySelector('.nav-link[data-view="' + slug + '"]');
    if (navLink) navLink.classList.add('active');

    // Refresh the component if it has a refresh method
    if (viewComponents[slug] && viewComponents[slug].refresh) {{
        viewComponents[slug].refresh();
    }}
}}

function toggleSidebar() {{
    document.getElementById('appSidebar').classList.toggle('collapsed');
}}

// Initialize components
{init_scripts}

// Initialize Lucide icons
if (typeof lucide !== 'undefined') lucide.createIcons();
"#
    );

    let css = format!(
        r#":root {{
    --app-primary: {primary};
    --app-sidebar-bg: {sidebar_bg};
    --app-sidebar-text: {sidebar_text};
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #f8fafc; }}
.app-layout {{ display: flex; height: 100vh; overflow: hidden; }}
.app-sidebar {{
    width: 240px; min-width: 240px; background: var(--app-sidebar-bg); color: var(--app-sidebar-text);
    display: flex; flex-direction: column; transition: margin-left 0.2s;
}}
.app-sidebar.collapsed {{ margin-left: -240px; }}
.sidebar-header {{
    padding: 16px; display: flex; align-items: center; gap: 10px; border-bottom: 1px solid rgba(255,255,255,0.1);
}}
.sidebar-header h1 {{ font-size: 16px; font-weight: 600; flex: 1; }}
.sidebar-header i {{ width: 20px; height: 20px; }}
.sidebar-close {{ display: none; background: none; border: none; color: inherit; cursor: pointer; }}
.sidebar-nav {{ flex: 1; padding: 8px; overflow-y: auto; }}
.nav-link {{
    display: flex; align-items: center; gap: 10px; padding: 10px 12px; border-radius: 8px;
    color: var(--app-sidebar-text); text-decoration: none; font-size: 14px; transition: background 0.15s;
}}
.nav-link:hover {{ background: rgba(255,255,255,0.1); }}
.nav-link.active {{ background: var(--app-primary); color: #fff; }}
.nav-link i {{ width: 18px; height: 18px; }}
.app-main {{ flex: 1; display: flex; flex-direction: column; overflow: hidden; }}
.app-topbar {{
    padding: 12px 20px; display: flex; align-items: center; gap: 12px;
    border-bottom: 1px solid #e2e8f0; background: #fff;
}}
.app-topbar h2 {{ font-size: 18px; font-weight: 600; color: #1e293b; }}
.hamburger {{ display: none; background: none; border: none; cursor: pointer; padding: 4px; color: #475569; }}
.app-content {{ flex: 1; padding: 20px; overflow-y: auto; }}
.view-container {{ height: 100%; }}
.hidden {{ display: none !important; }}

/* Dashboard grid */
.dashboard-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 16px; }}
.dashboard-widget {{ background: #fff; border-radius: 12px; padding: 16px; box-shadow: 0 1px 3px rgba(0,0,0,0.08); }}

/* Responsive */
@media (max-width: 768px) {{
    .app-sidebar {{ position: fixed; z-index: 50; height: 100vh; }}
    .app-sidebar.collapsed {{ margin-left: -240px; }}
    .sidebar-close {{ display: block; }}
    .hamburger {{ display: block; }}
}}"#
    );

    (html, js, css)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_app_html_basic() {
        let config = json!({
            "icon": "users",
            "theme": { "primary": "#dc2626" },
            "views": [
                {
                    "name": "Contacts",
                    "slug": "contacts",
                    "icon": "contact",
                    "view_type": "data_table",
                    "component_config": { "collection": "contacts", "page_size": 25 }
                },
                {
                    "name": "Pipeline",
                    "slug": "pipeline",
                    "icon": "kanban",
                    "view_type": "kanban",
                    "is_default": true,
                    "component_config": { "collection": "deals", "group_field": "stage" }
                }
            ]
        });

        let (html, js, css) = render_app_html("Test CRM", &config);

        assert!(html.contains("Test CRM"));
        assert!(html.contains("view-contacts"));
        assert!(html.contains("view-pipeline"));
        assert!(js.contains("AMOS.DataTable"));
        assert!(js.contains("AMOS.KanbanBoard"));
        assert!(css.contains("#dc2626"));
    }

    #[test]
    fn test_render_app_html_dashboard_view() {
        let config = json!({
            "views": [
                {
                    "name": "Overview",
                    "slug": "overview",
                    "view_type": "dashboard",
                    "component_config": {
                        "widgets": [
                            { "type": "metric", "collection": "deals", "aggregation": "count", "label": "Total Deals" }
                        ]
                    }
                }
            ]
        });

        let (html, js, _css) = render_app_html("Dashboard App", &config);
        assert!(html.contains("view-overview"));
        assert!(js.contains("AMOS.MetricCard"));
    }

    #[test]
    fn test_render_app_html_defaults() {
        let config = json!({ "views": [] });
        let (html, _js, css) = render_app_html("Empty App", &config);
        assert!(html.contains("Empty App"));
        // Default theme colors
        assert!(css.contains("#4f46e5"));
        assert!(css.contains("#1e1b4b"));
    }
}
