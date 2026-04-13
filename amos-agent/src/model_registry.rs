//! Model registry for smart model selection.
//!
//! Provides Haiku → Sonnet → Opus routing based on task complexity,
//! with support for custom model overrides (BYOK / self-hosted).

use serde::{Deserialize, Serialize};
use tracing::info;

/// Model tier for routing decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    /// Fast, cheap model for simple tasks (e.g., Haiku)
    Fast,
    /// Default balanced model (e.g., Sonnet)
    Default,
    /// Most capable model for complex tasks (e.g., Opus)
    Complex,
}

/// A registered model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub tier: ModelTier,
    pub provider: String, // "bedrock" or "openai"
    pub display_name: String,
    pub max_tokens: u64,
}

/// Model registry with smart routing
pub struct ModelRegistry {
    models: Vec<ModelInfo>,
    default_model_id: String,
}

impl ModelRegistry {
    /// Create a new registry with default Anthropic API models.
    pub fn new_anthropic() -> Self {
        let models = vec![
            ModelInfo {
                model_id: "claude-haiku-4-5".to_string(),
                tier: ModelTier::Fast,
                provider: "anthropic".to_string(),
                display_name: "Claude Haiku 4.5".to_string(),
                max_tokens: 4096,
            },
            ModelInfo {
                model_id: "claude-sonnet-4-6".to_string(),
                tier: ModelTier::Default,
                provider: "anthropic".to_string(),
                display_name: "Claude Sonnet 4.6".to_string(),
                max_tokens: 16384,
            },
            ModelInfo {
                model_id: "claude-opus-4-6".to_string(),
                tier: ModelTier::Complex,
                provider: "anthropic".to_string(),
                display_name: "Claude Opus 4.6".to_string(),
                max_tokens: 16384,
            },
        ];
        let default_model_id = models[1].model_id.clone(); // Sonnet as default
        Self {
            models,
            default_model_id,
        }
    }

    /// Create a new registry with default Bedrock models.
    pub fn new_bedrock() -> Self {
        let models = vec![
            ModelInfo {
                model_id: "us.anthropic.claude-3-5-haiku-20241022-v1:0".to_string(),
                tier: ModelTier::Fast,
                provider: "bedrock".to_string(),
                display_name: "Claude 3.5 Haiku".to_string(),
                max_tokens: 4096,
            },
            ModelInfo {
                model_id: "us.anthropic.claude-sonnet-4-6".to_string(),
                tier: ModelTier::Default,
                provider: "bedrock".to_string(),
                display_name: "Claude Sonnet 4.6".to_string(),
                max_tokens: 16384,
            },
            ModelInfo {
                model_id: "us.anthropic.claude-opus-4-6-v1".to_string(),
                tier: ModelTier::Complex,
                provider: "bedrock".to_string(),
                display_name: "Claude Opus 4.6".to_string(),
                max_tokens: 16384,
            },
        ];
        let default_model_id = models[1].model_id.clone(); // Sonnet as default
        Self {
            models,
            default_model_id,
        }
    }

    /// Create a registry with a single custom model.
    pub fn new_custom(model_id: String, provider: String) -> Self {
        let model = ModelInfo {
            model_id: model_id.clone(),
            tier: ModelTier::Default,
            provider,
            display_name: model_id.clone(),
            max_tokens: 16384,
        };
        Self {
            models: vec![model],
            default_model_id: model_id,
        }
    }

    /// Get the default model ID.
    pub fn default_model(&self) -> &str {
        &self.default_model_id
    }

    /// Select a model by tier. Falls back to default if tier not available.
    pub fn select_by_tier(&self, tier: ModelTier) -> &ModelInfo {
        self.models
            .iter()
            .find(|m| m.tier == tier)
            .unwrap_or_else(|| {
                self.models
                    .iter()
                    .find(|m| m.model_id == self.default_model_id)
                    .unwrap_or(&self.models[0])
            })
    }

    /// Select model based on task complexity heuristics.
    /// - Short simple queries → Fast (Haiku)
    /// - Normal conversation → Default (Sonnet)
    /// - Complex multi-step / code generation → Complex (Opus)
    pub fn select_for_message(&self, message: &str, tool_count: usize) -> &ModelInfo {
        let len = message.len();
        let tier = if len < 100 && tool_count == 0 {
            ModelTier::Fast
        } else if len > 2000
            || tool_count > 10
            || message.contains("analyze")
            || message.contains("architect")
        {
            ModelTier::Complex
        } else {
            ModelTier::Default
        };
        info!(tier = ?tier, "Selected model tier for message ({} chars, {} tools)", len, tool_count);
        self.select_by_tier(tier)
    }

    /// List all registered models.
    pub fn list(&self) -> &[ModelInfo] {
        &self.models
    }

    /// Override the default model.
    pub fn set_default(&mut self, model_id: &str) {
        if self.models.iter().any(|m| m.model_id == model_id) {
            self.default_model_id = model_id.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_registry() {
        let reg = ModelRegistry::new_anthropic();
        assert_eq!(reg.list().len(), 3);
        assert!(reg.default_model().contains("sonnet"));
        assert_eq!(reg.list()[0].provider, "anthropic");
    }

    #[test]
    fn test_bedrock_registry() {
        let reg = ModelRegistry::new_bedrock();
        assert_eq!(reg.list().len(), 3);
        assert!(reg.default_model().contains("sonnet"));
        assert_eq!(reg.list()[0].provider, "bedrock");
    }

    #[test]
    fn test_tier_selection() {
        let reg = ModelRegistry::new_anthropic();
        let fast = reg.select_by_tier(ModelTier::Fast);
        assert!(fast.model_id.contains("haiku"));
        let complex = reg.select_by_tier(ModelTier::Complex);
        assert!(complex.model_id.contains("opus"));
    }

    #[test]
    fn test_message_heuristic() {
        let reg = ModelRegistry::new_anthropic();
        let short = reg.select_for_message("hi", 0);
        assert_eq!(short.tier, ModelTier::Fast);
        let normal = reg.select_for_message("Help me build a website for my business", 3);
        assert_eq!(normal.tier, ModelTier::Default);
    }

    #[test]
    fn test_custom_registry() {
        let reg = ModelRegistry::new_custom("gpt-4".to_string(), "openai".to_string());
        assert_eq!(reg.list().len(), 1);
        assert_eq!(reg.default_model(), "gpt-4");
    }
}
