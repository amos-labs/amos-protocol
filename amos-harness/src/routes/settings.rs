//! Harness settings routes — model selection, provider mode, etc.
//!
//! Endpoints:
//!   - `GET  /api/v1/settings` — Get all settings + available models
//!   - `PUT  /api/v1/settings` — Update one or more settings
//!
//! Billing model:
//!   - Hosted harnesses have a working Bedrock client (via ECS task role).
//!     This means shared Bedrock is ON by default — the customer went through
//!     Stripe checkout to get a harness, so billing is already established.
//!   - Every token used via shared Bedrock is tracked per-model and reported to
//!     the platform via activity sync, where cost is calculated at Bedrock
//!     pricing + 3% markup and billed via Stripe metered billing.
//!   - Users can switch to BYOK mode in settings (bring their own API key).
//!   - Self-hosted harnesses without Bedrock creds — BYOK only. Can also
//!     explicitly opt out via `AMOS__SHARED_BEDROCK__DISABLED=true`.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new().route("/", get(get_settings).put(update_settings))
}

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
struct SettingsResponse {
    /// Current provider mode: "shared_bedrock" or "byok"
    llm_provider_mode: String,
    /// Current model ID (Bedrock model ID for shared, provider-specific for BYOK)
    llm_model: String,
    /// Whether shared Bedrock is available (false for self-hosted)
    shared_bedrock_available: bool,
    /// Available models for shared Bedrock (with pricing shown to user)
    available_models: Vec<ModelInfo>,
}

/// Prices are per 1M tokens, include the customer-facing 3% markup over Bedrock base.
///
/// Anthropic's pricing schedule has five dimensions:
///   - standard (input, output)
///   - batch    (input, output) — 50% discount; some models don't offer batch
///   - cache write (5-minute TTL)
///   - cache write (1-hour TTL)
///   - cache read (cheapest; hit on prompt cache)
///
/// `None` means the model doesn't offer that tier (e.g. Opus 4.7 has no batch).
#[derive(Debug, Serialize)]
struct ModelInfo {
    id: &'static str,
    display_name: &'static str,
    tier: &'static str,
    input_price_per_mtok: f64,
    output_price_per_mtok: f64,
    batch_input_price_per_mtok: Option<f64>,
    batch_output_price_per_mtok: Option<f64>,
    cache_write_5m_price_per_mtok: f64,
    cache_write_1h_price_per_mtok: f64,
    cache_read_price_per_mtok: f64,
}

#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    llm_provider_mode: Option<String>,
    llm_model: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Constants — pricing shown to customer includes 3% markup
// ═══════════════════════════════════════════════════════════════════════════

// Pattern across Anthropic tiers: cache-write-5m = 1.25× base input,
// cache-write-1h = 2× base input, cache-read = 0.10× base input,
// batch = 0.50× base (input and output). All values below are Bedrock base × 1.03.
const AVAILABLE_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "us.anthropic.claude-haiku-4-5-20251001-v1:0",
        display_name: "Claude Haiku 4.5",
        tier: "fast",
        // AWS Bedrock pricing 2026-05-02: base $1.00 / $5.00, repriced
        // up from $0.80 / $4.00 since the catalog was last touched.
        input_price_per_mtok: 1.03,
        output_price_per_mtok: 5.15,
        batch_input_price_per_mtok: Some(0.515),
        batch_output_price_per_mtok: Some(2.575),
        cache_write_5m_price_per_mtok: 1.2875,
        cache_write_1h_price_per_mtok: 2.06,
        cache_read_price_per_mtok: 0.103,
    },
    ModelInfo {
        id: "us.anthropic.claude-sonnet-4-6",
        display_name: "Claude Sonnet 4.6",
        tier: "balanced",
        // Base: $3.00 / $15.00
        input_price_per_mtok: 3.09,
        output_price_per_mtok: 15.45,
        batch_input_price_per_mtok: Some(1.545),
        batch_output_price_per_mtok: Some(7.725),
        cache_write_5m_price_per_mtok: 3.8625,
        cache_write_1h_price_per_mtok: 6.18,
        cache_read_price_per_mtok: 0.309,
    },
    ModelInfo {
        id: "us.anthropic.claude-opus-4-6-v1",
        display_name: "Claude Opus 4.6",
        tier: "powerful",
        // AWS Bedrock pricing 2026-05-02: base $5.00 / $25.00. Anthropic
        // dropped Opus 4.6 to match 4.7 — was $15.00 / $75.00 at launch.
        // Customers were being overcharged 3x until this fix.
        input_price_per_mtok: 5.15,
        output_price_per_mtok: 25.75,
        batch_input_price_per_mtok: Some(2.575),
        batch_output_price_per_mtok: Some(12.875),
        cache_write_5m_price_per_mtok: 6.4375,
        cache_write_1h_price_per_mtok: 10.30,
        cache_read_price_per_mtok: 0.515,
    },
    ModelInfo {
        id: "us.anthropic.claude-opus-4-7",
        display_name: "Claude Opus 4.7",
        tier: "powerful",
        // Base: $5.00 / $25.00 — batch not offered for this model
        input_price_per_mtok: 5.15,
        output_price_per_mtok: 25.75,
        batch_input_price_per_mtok: None,
        batch_output_price_per_mtok: None,
        cache_write_5m_price_per_mtok: 6.4375,
        cache_write_1h_price_per_mtok: 10.30,
        cache_read_price_per_mtok: 0.515,
    },
];

const DEFAULT_MODEL: &str = "us.anthropic.claude-sonnet-4-6";

/// Public accessor for the shared-Bedrock model catalog.
///
/// The single source of truth for "which Claude inference-profile IDs is this
/// release allowed to use." Consumed by:
///   * the `/api/v1/settings` handler (customer-facing dropdown)
///   * the CI `bedrock-probe` binary (live-fire release gate — probes every
///     ID against Bedrock before the image can ship)
///
/// Keeping both on the same list means a new model is auto-probed and a
/// renamed/removed inference profile blocks the release before customers see it.
pub fn catalog_model_ids() -> impl Iterator<Item = &'static str> {
    AVAILABLE_MODELS.iter().map(|m| m.id)
}

// ═══════════════════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════════════════

async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SettingsResponse>, StatusCode> {
    // Source of truth: did BedrockClient initialize at server startup?
    // The prior `SHARED_BEDROCK_ENABLED` env-var check was set on the wrong
    // container in production and silently hid the toggle from customers
    // (Rick + Jana: 2026-04-30 incident).
    let shared_bedrock_available = state.shared_bedrock_available;

    let provider_mode = get_setting(&state, "llm_provider_mode")
        .await
        .unwrap_or_else(|| {
            if shared_bedrock_available {
                "shared_bedrock".to_string()
            } else {
                "byok".to_string()
            }
        });

    let model = get_setting(&state, "llm_model")
        .await
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    Ok(Json(SettingsResponse {
        llm_provider_mode: provider_mode,
        llm_model: model,
        shared_bedrock_available,
        available_models: AVAILABLE_MODELS
            .iter()
            .map(|m| ModelInfo {
                id: m.id,
                display_name: m.display_name,
                tier: m.tier,
                input_price_per_mtok: m.input_price_per_mtok,
                output_price_per_mtok: m.output_price_per_mtok,
                batch_input_price_per_mtok: m.batch_input_price_per_mtok,
                batch_output_price_per_mtok: m.batch_output_price_per_mtok,
                cache_write_5m_price_per_mtok: m.cache_write_5m_price_per_mtok,
                cache_write_1h_price_per_mtok: m.cache_write_1h_price_per_mtok,
                cache_read_price_per_mtok: m.cache_read_price_per_mtok,
            })
            .collect(),
    }))
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let shared_bedrock_available = state.shared_bedrock_available;

    if let Some(mode) = &req.llm_provider_mode {
        if mode != "shared_bedrock" && mode != "byok" {
            return Err(StatusCode::BAD_REQUEST);
        }
        if mode == "shared_bedrock" && !shared_bedrock_available {
            return Err(StatusCode::BAD_REQUEST);
        }
        set_setting(&state, "llm_provider_mode", mode).await?;
    }

    if let Some(model) = &req.llm_model {
        // Validate model ID is in our allowed list
        let valid = AVAILABLE_MODELS.iter().any(|m| m.id == model);
        if !valid {
            return Err(StatusCode::BAD_REQUEST);
        }
        set_setting(&state, "llm_model", model).await?;
    }

    Ok(Json(serde_json::json!({ "updated": true })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Read a setting from the harness_settings table.
pub(crate) async fn get_setting(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, serde_json::Value>("SELECT value FROM harness_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(&state.db_pool)
        .await
        .ok()
        .flatten()
        .and_then(|v| {
            // Values are stored as JSON strings (e.g. "\"shared_bedrock\"")
            v.as_str().map(|s| s.to_string())
        })
}

/// Write a setting to the harness_settings table.
async fn set_setting(state: &AppState, key: &str, value: &str) -> Result<(), StatusCode> {
    sqlx::query(
        "INSERT INTO harness_settings (key, value, updated_at) VALUES ($1, $2, NOW())
         ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()",
    )
    .bind(key)
    .bind(serde_json::json!(value))
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to update setting {}: {}", key, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Regression tests — catalog invariants
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches the real Bedrock inference-profile / foundation-model ID grammar.
    /// Rejects rogue suffixes like `-v1` where Anthropic's actual profile has none
    /// (the Opus 4.7 bug that broke prod on 2026-04-20).
    fn is_valid_bedrock_model_id(id: &str) -> bool {
        // Optional geo prefix (us./eu./jp./global.), then "anthropic.claude-<tier>-<version>",
        // then optional "-YYYYMMDD-v<n>:<n>" or "-v<n>" tail.
        let re = regex::Regex::new(
            r"^((us|eu|jp|global)\.)?anthropic\.claude-(haiku|sonnet|opus)-\d+-\d+(-\d{8})?(-v\d+(:\d+)?)?$",
        )
        .expect("regex should compile");
        re.is_match(id)
    }

    #[test]
    fn model_catalog_is_non_empty() {
        assert!(!AVAILABLE_MODELS.is_empty(), "catalog must have ≥1 model");
    }

    #[test]
    fn default_model_is_in_catalog() {
        assert!(
            AVAILABLE_MODELS.iter().any(|m| m.id == DEFAULT_MODEL),
            "DEFAULT_MODEL '{}' not in AVAILABLE_MODELS",
            DEFAULT_MODEL
        );
    }

    #[test]
    fn every_model_id_matches_bedrock_grammar() {
        for m in AVAILABLE_MODELS {
            assert!(
                is_valid_bedrock_model_id(m.id),
                "model id '{}' fails Bedrock grammar — \
                 either missing/extra '-v1' suffix or wrong date segment",
                m.id
            );
        }
    }

    #[test]
    fn every_model_has_distinct_id_and_display_name() {
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for m in AVAILABLE_MODELS {
            assert!(ids.insert(m.id), "duplicate model id: {}", m.id);
            assert!(
                names.insert(m.display_name),
                "duplicate display name: {}",
                m.display_name
            );
        }
    }

    #[test]
    fn every_tier_is_known() {
        for m in AVAILABLE_MODELS {
            assert!(
                matches!(m.tier, "fast" | "balanced" | "powerful"),
                "model '{}' has unknown tier '{}'",
                m.id,
                m.tier
            );
        }
    }

    #[test]
    fn pricing_tiers_are_ordered_sanely() {
        for m in AVAILABLE_MODELS {
            assert!(
                m.input_price_per_mtok > 0.0,
                "{}: input price must be > 0",
                m.id
            );
            assert!(
                m.output_price_per_mtok > m.input_price_per_mtok,
                "{}: output ({}) should be > input ({})",
                m.id,
                m.output_price_per_mtok,
                m.input_price_per_mtok
            );
            assert!(
                m.cache_read_price_per_mtok < m.input_price_per_mtok,
                "{}: cache read ({}) should be < input ({})",
                m.id,
                m.cache_read_price_per_mtok,
                m.input_price_per_mtok
            );
            assert!(
                m.cache_write_5m_price_per_mtok > m.input_price_per_mtok,
                "{}: 5m cache write ({}) should be > input ({})",
                m.id,
                m.cache_write_5m_price_per_mtok,
                m.input_price_per_mtok
            );
            assert!(
                m.cache_write_1h_price_per_mtok > m.cache_write_5m_price_per_mtok,
                "{}: 1h cache write ({}) should be > 5m cache write ({})",
                m.id,
                m.cache_write_1h_price_per_mtok,
                m.cache_write_5m_price_per_mtok
            );
        }
    }

    #[test]
    fn anthropic_tier_ratios_are_close_to_schedule() {
        // Anthropic's public pattern (validated against Sonnet 4.6 and Opus 4.7):
        //   cache_write_5m = 1.25× input, cache_write_1h = 2× input,
        //   cache_read = 0.10× input, batch = 0.5× base.
        // Allow 2% slop for rounding at the third decimal.
        for m in AVAILABLE_MODELS {
            let r5m = m.cache_write_5m_price_per_mtok / m.input_price_per_mtok;
            let r1h = m.cache_write_1h_price_per_mtok / m.input_price_per_mtok;
            let rread = m.cache_read_price_per_mtok / m.input_price_per_mtok;
            assert!(
                (1.23..=1.27).contains(&r5m),
                "{}: cache_write_5m ratio {} not ≈ 1.25",
                m.id,
                r5m
            );
            assert!(
                (1.98..=2.02).contains(&r1h),
                "{}: cache_write_1h ratio {} not ≈ 2.0",
                m.id,
                r1h
            );
            assert!(
                (0.09..=0.11).contains(&rread),
                "{}: cache_read ratio {} not ≈ 0.10",
                m.id,
                rread
            );
            if let (Some(bi), Some(bo)) =
                (m.batch_input_price_per_mtok, m.batch_output_price_per_mtok)
            {
                let rbi = bi / m.input_price_per_mtok;
                let rbo = bo / m.output_price_per_mtok;
                assert!(
                    (0.49..=0.51).contains(&rbi),
                    "{}: batch input ratio {} not ≈ 0.50",
                    m.id,
                    rbi
                );
                assert!(
                    (0.49..=0.51).contains(&rbo),
                    "{}: batch output ratio {} not ≈ 0.50",
                    m.id,
                    rbo
                );
            }
        }
    }

    #[test]
    fn bad_model_ids_fail_grammar() {
        // The exact Opus 4.7 regression that broke prod.
        assert!(!is_valid_bedrock_model_id("us.anthropic.claude-opus-4-7-v"));
        // Missing model family.
        assert!(!is_valid_bedrock_model_id("us.anthropic.claude-4-7"));
        // Wrong vendor.
        assert!(!is_valid_bedrock_model_id("us.openai.gpt-5"));
        // Empty.
        assert!(!is_valid_bedrock_model_id(""));
    }
}
