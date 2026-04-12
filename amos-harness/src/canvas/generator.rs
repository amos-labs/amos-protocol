//! AI-powered canvas generation
//!
//! Uses LLMs to generate custom canvases based on user requirements

use super::{templates, types::CanvasType};
use crate::bedrock::BedrockClient;
use amos_core::{
    types::{ContentBlock, Message, Role},
    AmosError, Result,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// The model ID used for canvas generation (Haiku: fast and cheap)
const GENERATION_MODEL: &str = "us.anthropic.claude-3-5-haiku-20241022-v1:0";

/// Request for canvas generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateCanvasRequest {
    /// Module definition (fields, actions, etc.)
    pub module_definition: Option<JsonValue>,

    /// Desired canvas type
    pub canvas_type: CanvasType,

    /// Description of what the canvas should do
    pub description: String,

    /// Additional requirements or constraints
    pub requirements: Option<Vec<String>>,

    /// Sample data for context
    pub sample_data: Option<JsonValue>,
}

/// Generated canvas content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCanvas {
    /// HTML content
    pub html_content: String,

    /// JavaScript content
    pub js_content: Option<String>,

    /// CSS content
    pub css_content: Option<String>,

    /// Data sources configuration
    pub data_sources: Option<JsonValue>,

    /// Actions configuration
    pub actions: Option<JsonValue>,

    /// Validation warnings
    pub warnings: Vec<String>,
}

/// Generate a canvas using AI
///
/// Calls the LLM to produce custom HTML/JS/CSS for the given schema and description.
/// Falls back to static templates if the LLM call fails or returns unparseable content.
pub async fn generate_canvas(
    request: GenerateCanvasRequest,
    bedrock: Option<&BedrockClient>,
) -> Result<GeneratedCanvas> {
    // If we have a Bedrock client, try the LLM
    if let Some(client) = bedrock {
        match generate_with_llm(client, &request).await {
            Ok(canvas) => {
                tracing::info!(
                    "AI canvas generation succeeded for type={}, warnings={}",
                    request.canvas_type,
                    canvas.warnings.len()
                );
                return Ok(canvas);
            }
            Err(e) => {
                tracing::warn!(
                    "AI canvas generation failed, falling back to static template: {}",
                    e
                );
            }
        }
    } else {
        tracing::warn!("No Bedrock client available, using static template");
    }

    // Fall back to static templates
    use_static_template(&request)
}

/// Actually call the LLM and parse the response
async fn generate_with_llm(
    client: &BedrockClient,
    request: &GenerateCanvasRequest,
) -> Result<GeneratedCanvas> {
    let prompt = build_generation_prompt(request)?;

    let system_prompt = r#"You are an expert frontend developer. You generate production-quality HTML, JavaScript, and CSS for web application canvases.

Rules:
- Use Bootstrap 5 for layout and components (it is already loaded)
- Use Lucide icons via `lucide.createIcons()` (it is already loaded)
- Make everything responsive and mobile-friendly
- Use semantic HTML5 with proper accessibility attributes
- All interactive elements must have working event handlers
- Communicate with parent window using postMessage:
  window.parent.postMessage({ type: 'canvas-action', action: 'name', data: {} }, '*');
- Do NOT include <html>, <head>, or <body> tags — only the inner content
- Do NOT include Bootstrap or Lucide <script>/<link> tags — they are already loaded

## PREFERRED: AMOS Component Library

The `AMOS` component library is pre-loaded in every canvas. Use it instead of writing raw HTML tables, forms, charts, etc. Components auto-fetch data from the `/api/v1/data/{collection}` REST API.

### Available Components

**AMOS.MetricCard(el, opts)** — Single stat card
  Options: collection, label, aggregate ('count'|'sum'|'avg'|'min'|'max'), field, filters, format ('number'|'currency'|'percent'), icon (Lucide name), color (Bootstrap color)

**AMOS.DataTable(el, opts)** — Sortable, paginated table with CRUD
  Options: collection, columns (array of field names or {field, label, sortable}), actions (['edit','delete','view']), searchable (bool), sortable (bool), pageSize (default 25), filters, createButton (bool or label string)

**AMOS.FormBuilder(el, opts)** — Auto-generated form from collection schema
  Options: collection, recordId (for edit mode), fields (subset array), layout ('vertical'|'horizontal'|'two-column'), onSubmit (callback), onCancel (callback)

**AMOS.Chart(el, opts)** — Chart.js wrapper (Chart.js 4 is pre-loaded)
  Options: collection, type ('bar'|'line'|'pie'|'doughnut'), labelField, valueField, aggregate ('count'|'sum'|'avg'), title, data ({labels,values} static override), colors, filters

**AMOS.KanbanBoard(el, opts)** — Drag-and-drop board grouped by enum field
  Options: collection, groupBy, cardTitle, cardSubtitle, cardFields (array of extra fields), filters

**AMOS.FilterBar(el, opts)** — Filter controls driving other components
  Options: collection, fields (array of field names), targets (array of component instances)

### Data Helpers

- `AMOS.fetchData(collection, {filters, sort_by, sort_dir, limit, offset, search})` — fetch records
- `AMOS.fetchSchema(collection)` — fetch collection schema
- `AMOS.createRecord(collection, data)` / `AMOS.updateRecord(collection, id, data)` / `AMOS.deleteRecord(collection, id)`

### Example: Dashboard with metrics + chart + table (~15 lines)

```html
<div class="container-fluid py-3">
  <div class="row g-3 mb-3" id="metrics"></div>
  <div class="row g-3">
    <div class="col-md-5"><div id="chart" style="height:300px"></div></div>
    <div class="col-md-7" id="table"></div>
  </div>
</div>
```

```javascript
// Metrics row
const metricsEl = document.getElementById('metrics');
['count', 'sum', 'avg'].forEach((agg, i) => {
  const col = document.createElement('div');
  col.className = 'col-md-4';
  metricsEl.appendChild(col);
  new AMOS.MetricCard(col, { collection: 'orders', label: agg === 'count' ? 'Total Orders' : agg === 'sum' ? 'Revenue' : 'Avg Value', aggregate: agg, field: 'amount', format: agg === 'count' ? 'number' : 'currency', icon: 'shopping-cart', color: ['primary','success','info'][i] });
});
// Chart
new AMOS.Chart(document.getElementById('chart'), { collection: 'orders', type: 'bar', labelField: 'status', valueField: 'amount', aggregate: 'sum', title: 'Revenue by Status' });
// Table
new AMOS.DataTable(document.getElementById('table'), { collection: 'orders', searchable: true, actions: ['edit', 'delete'], createButton: 'New Order' });
```

Use AMOS components whenever the canvas involves collection data. Fall back to raw HTML only for purely static or non-data content.

Output your response in exactly this format with markdown code blocks:

### HTML
```html
<your html here>
```

### JavaScript
```javascript
<your javascript here>
```

### CSS
```css
<your css here>
```"#;

    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text { text: prompt }],
        tool_use_id: None,
        timestamp: Utc::now(),
    }];

    // No tools needed for canvas generation
    let tools: Vec<serde_json::Value> = vec![];

    let (response, usage) = client
        .converse(GENERATION_MODEL, system_prompt, &messages, &tools)
        .await?;

    tracing::info!(
        "Canvas generation LLM call: {} input tokens, {} output tokens",
        usage.input_tokens,
        usage.output_tokens
    );

    // Extract text from response
    let response_text = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    if response_text.is_empty() {
        return Err(AmosError::Internal(
            "LLM returned empty response for canvas generation".to_string(),
        ));
    }

    // Parse the generated content
    parse_generated_content(&response_text)
}

/// Build the prompt for canvas generation
fn build_generation_prompt(request: &GenerateCanvasRequest) -> Result<String> {
    use crate::prompt_guard;

    let canvas_type_str = request.canvas_type.as_str();

    let module_context = if let Some(module_def) = &request.module_definition {
        let json_str = serde_json::to_string_pretty(module_def).unwrap_or_default();
        format!(
            "\n\n## Module Definition (schema)\n\n{}",
            prompt_guard::wrap_user_data("module_definition", &json_str)
        )
    } else {
        String::new()
    };

    let sample_data_context = if let Some(sample_data) = &request.sample_data {
        let json_str = serde_json::to_string_pretty(sample_data).unwrap_or_default();
        format!(
            "\n\n## Sample Data\n\n{}",
            prompt_guard::wrap_user_data("sample_data", &json_str)
        )
    } else {
        String::new()
    };

    let requirements_context = if let Some(requirements) = &request.requirements {
        let items = requirements
            .iter()
            .enumerate()
            .map(|(i, r)| format!("{}. {}", i + 1, r))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\n## Additional Requirements\n\n{}",
            prompt_guard::wrap_user_data("requirements", &items)
        )
    } else {
        String::new()
    };

    // Sanitize user description (cap at 4000 chars)
    let description =
        prompt_guard::sanitize("canvas_description", &request.description, 4000);

    let prompt = format!(
        r#"{boundary_instruction}

Generate a **{canvas_type}** canvas for a web application.

## Description

{description}
{module_context}{sample_data_context}{requirements_context}

## Canvas Type Details

The canvas type is "{canvas_type}" which means:
- dashboard: Multi-widget overview with stats cards, charts, and summary tables
- kanban: Drag-and-drop board with columns representing statuses/stages
- datagrid: Sortable, filterable table with CRUD actions
- calendar: Month/week/day calendar view with events
- form: Data entry form with validation
- detail: Single-record detail view with related data
- report: Charts and analytics with date range filters
- wizard: Multi-step form with progress indicator
- freeform: Custom layout, no constraints

Generate complete, functional HTML/JS/CSS that is ready to use. Include realistic placeholder data if no sample data was provided. All buttons and interactions must work."#,
        boundary_instruction = prompt_guard::DATA_BOUNDARY_INSTRUCTION,
        canvas_type = canvas_type_str,
        description = description,
        module_context = module_context,
        sample_data_context = sample_data_context,
        requirements_context = requirements_context,
    );

    Ok(prompt)
}

/// Fall back to static template when AI generation is not available
fn use_static_template(request: &GenerateCanvasRequest) -> Result<GeneratedCanvas> {
    let template_key = match request.canvas_type {
        CanvasType::Dynamic => "list",
        CanvasType::Freeform => "freeform",
        CanvasType::Dashboard => "dashboard",
        CanvasType::DataGrid => "list",
        CanvasType::Form => "form",
        CanvasType::Detail => "detail",
        CanvasType::Kanban => "kanban",
        CanvasType::Calendar => "calendar",
        CanvasType::Report => "dashboard",
        CanvasType::Wizard => "form",
        CanvasType::Custom => "freeform",
    };

    let template = templates::get_template(template_key).ok_or_else(|| {
        AmosError::Internal(format!("No template found for key: {}", template_key))
    })?;

    Ok(GeneratedCanvas {
        html_content: template.html_content.unwrap_or_default(),
        js_content: template.js_content,
        css_content: template.css_content,
        data_sources: None,
        actions: None,
        warnings: vec!["Using static template (AI generation not available)".to_string()],
    })
}

/// Validate generated canvas content
pub fn validate_canvas(generated: &GeneratedCanvas) -> Vec<String> {
    let mut warnings = Vec::new();

    // Check for unwired buttons (buttons without event handlers)
    if let Some(js) = &generated.js_content {
        let button_count = generated.html_content.matches("<button").count()
            + generated.html_content.matches("data-action").count();

        let event_listener_count = js.matches("addEventListener").count()
            + js.matches("onclick").count()
            + js.matches("postMessage").count();

        if button_count > 0 && event_listener_count == 0 {
            warnings.push("Warning: Found buttons but no event handlers".to_string());
        }
    }

    // Check for dead links
    let link_count = generated.html_content.matches("href=\"#\"").count();
    if link_count > 0 {
        warnings.push(format!(
            "Warning: Found {} placeholder links (href=\"#\")",
            link_count
        ));
    }

    // Check for missing required elements
    if !generated.html_content.contains("container") {
        warnings.push("Warning: No Bootstrap container element found".to_string());
    }

    // Check for accessibility issues
    if generated.html_content.contains("<img") && !generated.html_content.contains("alt=") {
        warnings.push("Warning: Images without alt attributes (accessibility issue)".to_string());
    }

    warnings
}

/// Parse generated content from LLM response
pub fn parse_generated_content(llm_response: &str) -> Result<GeneratedCanvas> {
    // Extract HTML, JS, and CSS sections from the response
    let html_content = extract_section(llm_response, "HTML")
        .or_else(|| extract_section(llm_response, "html"))
        .ok_or_else(|| AmosError::Validation("No HTML section found in response".to_string()))?;

    let js_content = extract_section(llm_response, "JavaScript")
        .or_else(|| extract_section(llm_response, "javascript"))
        .or_else(|| extract_section(llm_response, "JS"))
        .or_else(|| extract_section(llm_response, "js"));

    let css_content =
        extract_section(llm_response, "CSS").or_else(|| extract_section(llm_response, "css"));

    let generated = GeneratedCanvas {
        html_content,
        js_content,
        css_content,
        data_sources: None,
        actions: None,
        warnings: Vec::new(),
    };

    // Validate the generated content
    let warnings = validate_canvas(&generated);

    Ok(GeneratedCanvas {
        warnings,
        ..generated
    })
}

/// Extract a section from markdown-style response
fn extract_section(text: &str, section_name: &str) -> Option<String> {
    // Look for markdown code blocks with the section name
    let markers = vec![
        format!("### {}", section_name),
        format!("## {}", section_name),
        format!("# {}", section_name),
    ];

    for marker in markers {
        if let Some(start_pos) = text.find(&marker) {
            let after_marker = &text[start_pos + marker.len()..];

            // Find the start of the code block
            if let Some(code_start) = after_marker.find("```") {
                let after_code_start = &after_marker[code_start + 3..];

                // Skip language identifier if present
                let content_start = after_code_start.find('\n').map(|pos| pos + 1).unwrap_or(0);

                let content = &after_code_start[content_start..];

                // Find the end of the code block
                if let Some(code_end) = content.find("```") {
                    return Some(content[..code_end].trim().to_string());
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_section() {
        let response = r#"
Here is the canvas:

### HTML
```html
<div>Hello World</div>
```

### JavaScript
```javascript
console.log('test');
```
"#;

        let html = extract_section(response, "HTML").unwrap();
        assert_eq!(html, "<div>Hello World</div>");

        let js = extract_section(response, "JavaScript").unwrap();
        assert_eq!(js, "console.log('test');");
    }

    #[test]
    fn test_validate_canvas() {
        let canvas = GeneratedCanvas {
            html_content: "<button>Click me</button>".to_string(),
            js_content: Some("// no event handlers".to_string()),
            css_content: None,
            data_sources: None,
            actions: None,
            warnings: Vec::new(),
        };

        let warnings = validate_canvas(&canvas);
        assert!(warnings.iter().any(|w| w.contains("no event handlers")));
    }
}
