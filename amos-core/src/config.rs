//! Application configuration loaded from env vars, files, and defaults.
//!
//! Uses the [`config`] crate to layer: defaults < config file < env vars.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

/// Deployment mode: managed (AMOS cloud) or self-hosted (customer hardware).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum DeploymentMode {
    /// AMOS manages the harness via Docker API (default).
    #[default]
    Managed,
    /// Customer runs harness on their own infrastructure.
    SelfHosted,
}

/// Root configuration for the AMOS Rust core.
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub redis: RedisConfig,
    #[serde(default)]
    pub solana: SolanaConfig,
    #[serde(default)]
    pub bedrock: BedrockConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    /// Deployment mode: managed cloud or self-hosted.
    #[serde(default)]
    pub deployment: DeploymentConfig,
    /// Platform sync settings (harness→platform communication).
    #[serde(default)]
    pub platform: PlatformConfig,
    /// Custom model providers (for sovereign AI / self-hosted Qwen).
    #[serde(default)]
    pub custom_models: CustomModelsConfig,
    /// Authentication and authorization settings.
    #[serde(default)]
    pub auth: AuthConfig,
    /// Relay connection settings (harness→relay communication).
    #[serde(default)]
    pub relay: RelayConfig,
    /// Embedding service settings (OpenAI-compatible API for vector embeddings).
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    /// Fleet settings (autonomous bounty agent management).
    #[serde(default)]
    pub fleet: FleetConfig,
    /// Email delivery settings (AWS SES).
    #[serde(default)]
    pub email: EmailConfig,
    /// Twilio credentials (WhatsApp messaging).
    #[serde(default)]
    pub twilio: TwilioConfig,
    /// Discord default webhook URL.
    #[serde(default)]
    pub discord: DiscordConfig,
    /// OAuth2 flow settings.
    #[serde(default)]
    pub oauth: OAuthConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
    /// Base URL of the existing Rails app (for hybrid proxying).
    #[serde(default = "default_rails_url")]
    pub rails_url: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            grpc_port: default_grpc_port(),
            rails_url: default_rails_url(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: SecretString,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SolanaConfig {
    #[serde(default = "default_solana_rpc")]
    pub rpc_url: String,
    #[serde(default = "default_solana_ws")]
    pub ws_url: String,
    #[serde(default = "default_treasury_program")]
    pub treasury_program_id: String,
    #[serde(default = "default_governance_program")]
    pub governance_program_id: String,
    #[serde(default = "default_bounty_program")]
    pub bounty_program_id: String,
    /// Path to the oracle keypair JSON file (Solana CLI format).
    /// Required for signing bounty settlement transactions.
    #[serde(default)]
    pub oracle_keypair_path: Option<String>,
    /// AMOS SPL token mint address.
    #[serde(default = "default_mint_address")]
    pub mint_address: Option<String>,
    /// Treasury token account that holds distribution tokens.
    #[serde(default = "default_treasury_token_account")]
    pub treasury_token_account: Option<String>,
}

impl Default for SolanaConfig {
    fn default() -> Self {
        Self {
            rpc_url: default_solana_rpc(),
            ws_url: default_solana_ws(),
            treasury_program_id: default_treasury_program(),
            governance_program_id: default_governance_program(),
            bounty_program_id: default_bounty_program(),
            oracle_keypair_path: None,
            mint_address: default_mint_address(),
            treasury_token_account: default_treasury_token_account(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct BedrockConfig {
    #[serde(default = "default_aws_region")]
    pub aws_region: String,
    pub aws_access_key_id: Option<SecretString>,
    pub aws_secret_access_key: Option<SecretString>,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default = "default_chat_model")]
    pub chat_model: String,
    #[serde(default = "default_voice_model")]
    pub voice_model: String,
}

impl Default for BedrockConfig {
    fn default() -> Self {
        Self {
            aws_region: default_aws_region(),
            aws_access_key_id: None,
            aws_secret_access_key: None,
            default_model: default_model(),
            chat_model: default_chat_model(),
            voice_model: default_voice_model(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    /// Maximum iterations for the V3 agent loop before forced stop.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    /// Maximum context tokens before compaction.
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
    /// Token budget per autonomous loop cycle.
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            max_context_tokens: default_max_context_tokens(),
            token_budget: default_token_budget(),
        }
    }
}

/// Deployment and licensing configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct DeploymentConfig {
    /// Deployment mode: "managed" or "self_hosted".
    #[serde(default)]
    pub mode: DeploymentMode,
    /// License key for self-hosted deployments (validated against platform).
    pub license_key: Option<SecretString>,
    /// Harness version (set at build time, used for update checks).
    #[serde(default = "default_harness_version")]
    pub harness_version: String,
    /// Auto-update: pull new versions automatically (self-hosted only).
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
}

impl Default for DeploymentConfig {
    fn default() -> Self {
        Self {
            mode: DeploymentMode::default(),
            license_key: None,
            harness_version: default_harness_version(),
            auto_update: default_auto_update(),
        }
    }
}

/// Platform sync configuration (how harness talks to the central platform).
#[derive(Debug, Deserialize, Clone)]
pub struct PlatformConfig {
    /// Platform API URL (e.g., "https://api.amos.ai").
    #[serde(default = "default_platform_url")]
    pub url: String,
    /// API key for authenticating with the platform.
    pub api_key: Option<SecretString>,
    /// Heartbeat interval in seconds (how often harness pings platform).
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
    /// Config sync interval in seconds (how often to pull config updates).
    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,
    /// Activity report interval in seconds (how often to push usage data).
    #[serde(default = "default_activity_interval")]
    pub activity_report_interval_secs: u64,
    /// Whether to report usage/telemetry to platform (can be disabled for air-gapped).
    #[serde(default = "default_telemetry_enabled")]
    pub telemetry_enabled: bool,
}

impl Default for PlatformConfig {
    fn default() -> Self {
        Self {
            url: default_platform_url(),
            api_key: None,
            heartbeat_interval_secs: default_heartbeat_interval(),
            sync_interval_secs: default_sync_interval(),
            activity_report_interval_secs: default_activity_interval(),
            telemetry_enabled: default_telemetry_enabled(),
        }
    }
}

/// Configuration for customer-provisioned AI models (sovereign AI).
#[derive(Debug, Deserialize, Clone, Default)]
pub struct CustomModelsConfig {
    /// Whether custom model support is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// List of custom model providers.
    #[serde(default)]
    pub providers: Vec<CustomModelProvider>,
}

/// A custom model provider (OpenAI-compatible API endpoint).
///
/// Supports self-hosted models via vLLM, TGI, Ollama, or any OpenAI-compatible server.
/// Intended for Qwen models (Qwen3-Next, Qwen 3.5) but works with any model
/// that exposes the OpenAI chat completions API.
#[derive(Debug, Deserialize, Clone)]
pub struct CustomModelProvider {
    /// Unique name for this provider (e.g., "qwen-local", "sovereign-qwen").
    pub name: String,
    /// Display name shown in UI (e.g., "Qwen3-Next 80B (Self-Hosted)").
    pub display_name: String,
    /// Base URL of the OpenAI-compatible API (e.g., "http://gpu-server:8000/v1").
    pub api_base: String,
    /// API key for the custom endpoint (optional, some local servers don't need one).
    pub api_key: Option<SecretString>,
    /// Model ID to send in API requests (e.g., "Qwen/Qwen3-Next-80B").
    pub model_id: String,
    /// Context window size in tokens.
    #[serde(default = "default_custom_context_window")]
    pub context_window: usize,
    /// Tier for model routing (1=fast/cheap, 2=balanced, 3=capable).
    #[serde(default = "default_custom_tier")]
    pub tier: u8,
    /// Cost per 1k input tokens (0.0 if customer owns the hardware).
    #[serde(default)]
    pub cost_per_1k_input: f64,
    /// Cost per 1k output tokens (0.0 if customer owns the hardware).
    #[serde(default)]
    pub cost_per_1k_output: f64,
    /// Whether this is a customer-owned model (no compute markup in billing).
    #[serde(default)]
    pub customer_owned: bool,
}

/// Authentication and authorization settings.
#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    /// JWT signing secret. MUST be set in production via AMOS__AUTH__JWT_SECRET.
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: SecretString,
    /// Access token lifetime in seconds (default: 3600 = 1 hour).
    #[serde(default = "default_access_token_expiry")]
    pub access_token_expiry_secs: u64,
    /// Refresh token lifetime in seconds (default: 604800 = 7 days).
    #[serde(default = "default_refresh_token_expiry")]
    pub refresh_token_expiry_secs: u64,
    /// Base domain for subdomain routing (e.g. "amos.ai").
    #[serde(default = "default_base_domain")]
    pub base_domain: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: default_jwt_secret(),
            access_token_expiry_secs: default_access_token_expiry(),
            refresh_token_expiry_secs: default_refresh_token_expiry(),
            base_domain: default_base_domain(),
        }
    }
}

/// Relay connection configuration (how harness talks to the AMOS Network Relay).
#[derive(Debug, Deserialize, Clone)]
pub struct RelayConfig {
    /// Relay API URL (e.g., "https://relay.amos.ai").
    #[serde(default = "default_relay_url")]
    pub url: String,
    /// API key for authenticating with the relay.
    pub api_key: Option<SecretString>,
    /// Whether relay integration is enabled.
    #[serde(default = "default_relay_enabled")]
    pub enabled: bool,
    /// Heartbeat interval in seconds (how often harness pings relay).
    #[serde(default = "default_relay_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
    /// Bounty sync interval in seconds (how often to check for new bounties).
    #[serde(default = "default_relay_bounty_sync_interval")]
    pub bounty_sync_interval_secs: u64,
    /// Reputation report interval in seconds (how often to push reputation data).
    #[serde(default = "default_relay_reputation_interval")]
    pub reputation_report_interval_secs: u64,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            url: default_relay_url(),
            api_key: None,
            enabled: default_relay_enabled(),
            heartbeat_interval_secs: default_relay_heartbeat_interval(),
            bounty_sync_interval_secs: default_relay_bounty_sync_interval(),
            reputation_report_interval_secs: default_relay_reputation_interval(),
        }
    }
}

/// Embedding service configuration (OpenAI-compatible API).
///
/// Used for semantic search in memory/knowledge base. AMOS owns the API key
/// and passes cost to customers. Users don't configure anything.
#[derive(Debug, Deserialize, Clone)]
pub struct EmbeddingConfig {
    /// API key for the embedding service. If not set, embeddings are disabled.
    pub api_key: Option<SecretString>,
    /// Model to use for embeddings.
    #[serde(default = "default_embedding_model")]
    pub model: String,
    /// Base URL for the OpenAI-compatible API.
    #[serde(default = "default_embedding_api_base")]
    pub api_base: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            model: default_embedding_model(),
            api_base: default_embedding_api_base(),
        }
    }
}

/// Email delivery settings (AWS SES v2).
///
/// If `from_address` is not set, email delivery is disabled and
/// `SendNotification` actions with `channel: "email"` become a warning log.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct EmailConfig {
    /// Verified SES sender address. Required to enable email delivery.
    /// Env: `AMOS__EMAIL__FROM_ADDRESS`
    #[serde(default)]
    pub from_address: Option<String>,

    /// Default reply-to address. Optional.
    /// Env: `AMOS__EMAIL__REPLY_TO`
    #[serde(default)]
    pub reply_to: Option<String>,

    /// AWS region for SES. Defaults to the main AWS_REGION.
    /// Env: `AMOS__EMAIL__REGION`
    #[serde(default)]
    pub region: Option<String>,
}

/// Twilio credentials (used for WhatsApp messaging).
///
/// All three fields are required to enable WhatsApp delivery. If any are
/// missing the `send_whatsapp` tool returns an error.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct TwilioConfig {
    /// Twilio Account SID.
    /// Env: `AMOS__TWILIO__ACCOUNT_SID`
    #[serde(default)]
    pub account_sid: Option<String>,

    /// Twilio Auth Token.
    /// Env: `AMOS__TWILIO__AUTH_TOKEN`
    #[serde(default)]
    pub auth_token: Option<SecretString>,

    /// Twilio WhatsApp-enabled From number (e.g. "whatsapp:+14155238886").
    /// Env: `AMOS__TWILIO__FROM_NUMBER`
    #[serde(default)]
    pub from_number: Option<String>,
}

/// Discord default webhook URL (optional — callers can also supply one per
/// message). If set, `send_discord` uses this URL when no `webhook_url`
/// parameter is provided.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct DiscordConfig {
    /// Env: `AMOS__DISCORD__DEFAULT_WEBHOOK_URL`
    #[serde(default)]
    pub default_webhook_url: Option<String>,
}

/// OAuth2 flow configuration.
///
/// `redirect_base_url` is the public URL of this harness that upstream
/// providers redirect back to after consent. Example:
/// `https://harness.amoslabs.com` → callback will be
/// `https://harness.amoslabs.com/api/v1/oauth/callback`.
#[derive(Debug, Deserialize, Clone)]
pub struct OAuthConfig {
    /// Env: `AMOS__OAUTH__REDIRECT_BASE_URL`
    #[serde(default = "default_oauth_redirect_base")]
    pub redirect_base_url: String,
}

fn default_oauth_redirect_base() -> String {
    "http://localhost:3000".to_string()
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            redirect_base_url: default_oauth_redirect_base(),
        }
    }
}

/// Fleet configuration for autonomous bounty agent management.
#[derive(Debug, Deserialize, Clone)]
pub struct FleetConfig {
    /// Whether the autonomous fleet is enabled.
    #[serde(default = "default_fleet_enabled")]
    pub enabled: bool,
    /// Maximum number of concurrent autonomous agents.
    #[serde(default = "default_fleet_max_agents")]
    pub max_agents: u32,
    /// Polling interval in seconds (how often idle agents check for bounties).
    #[serde(default = "default_fleet_polling_interval")]
    pub polling_interval_secs: u64,
    /// Maximum backoff in seconds when no bounties are available.
    #[serde(default = "default_fleet_backoff_max")]
    pub backoff_max_secs: u64,
    /// Whether to auto-scale agents based on bounty queue depth.
    #[serde(default = "default_fleet_auto_scale")]
    pub auto_scale: bool,
    /// Minimum fit score (0.0-1.0) for an agent to claim a bounty.
    #[serde(default = "default_fleet_min_fit_score")]
    pub min_fit_score: f64,
    /// Path to AGENT_CONTEXT.md for protocol parameter injection.
    #[serde(default = "default_fleet_agent_context_path")]
    pub agent_context_path: String,
    /// Local open-source model configuration for cost-free bounty execution.
    #[serde(default)]
    pub local_model: LocalModelConfig,
    /// Initial fleet composition deployed on startup (JSON array).
    /// Example: `[{"profile":"research","count":2},{"profile":"general","count":1}]`
    #[serde(default)]
    pub initial_fleet: Vec<InitialFleetEntry>,
    /// Interval in seconds for the health check loop (default: 60).
    #[serde(default = "default_fleet_health_check_interval")]
    pub health_check_interval_secs: u64,
    /// Interval in seconds for automatic rebalancing (default: 1800 = 30 min).
    #[serde(default = "default_fleet_rebalance_interval")]
    pub rebalance_interval_secs: u64,
    /// Max seconds to wait for bounty verification before timing out (default: 86400 = 24h).
    #[serde(default = "default_fleet_verification_timeout")]
    pub verification_timeout_secs: u64,
}

/// An entry in the initial fleet composition.
#[derive(Debug, Deserialize, Clone)]
pub struct InitialFleetEntry {
    pub profile: String,
    pub count: u32,
}

fn default_fleet_health_check_interval() -> u64 {
    60
}
fn default_fleet_rebalance_interval() -> u64 {
    1800
}
fn default_fleet_verification_timeout() -> u64 {
    86400
}

impl Default for FleetConfig {
    fn default() -> Self {
        Self {
            enabled: default_fleet_enabled(),
            max_agents: default_fleet_max_agents(),
            polling_interval_secs: default_fleet_polling_interval(),
            backoff_max_secs: default_fleet_backoff_max(),
            auto_scale: default_fleet_auto_scale(),
            min_fit_score: default_fleet_min_fit_score(),
            agent_context_path: default_fleet_agent_context_path(),
            local_model: LocalModelConfig::default(),
            initial_fleet: Vec::new(),
            health_check_interval_secs: default_fleet_health_check_interval(),
            rebalance_interval_secs: default_fleet_rebalance_interval(),
            verification_timeout_secs: default_fleet_verification_timeout(),
        }
    }
}

impl FleetConfig {
    /// Whether a local model is configured and available for fleet routing.
    pub fn has_local_model(&self) -> bool {
        self.local_model.enabled && !self.local_model.api_base.is_empty()
    }
}

/// Local open-source model configuration (Ollama, vLLM, or any OpenAI-compatible server).
///
/// When enabled, fleet agents route low-value bounties to the local model instead
/// of the cloud provider (Bedrock), reducing API costs to zero for routine work.
///
/// Env vars: `AMOS__FLEET__LOCAL_MODEL__ENABLED`, `AMOS__FLEET__LOCAL_MODEL__API_BASE`, etc.
#[derive(Debug, Deserialize, Clone)]
pub struct LocalModelConfig {
    /// Whether local model routing is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Provider type (e.g., "ollama", "vllm", "openai").
    #[serde(default = "default_local_model_provider")]
    pub provider: String,
    /// Base URL for the OpenAI-compatible API endpoint.
    #[serde(default = "default_local_model_api_base")]
    pub api_base: String,
    /// Model ID to use (e.g., "llama3.2:3b", "qwen2.5:7b", "mistral:7b").
    #[serde(default = "default_local_model_id")]
    pub model_id: String,
    /// Bounty reward threshold: bounties at or below this token value use the local model.
    /// Bounties above this value use the cloud model (Bedrock).
    #[serde(default = "default_local_model_cost_threshold")]
    pub cost_threshold: u64,
}

impl Default for LocalModelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_local_model_provider(),
            api_base: default_local_model_api_base(),
            model_id: default_local_model_id(),
            cost_threshold: default_local_model_cost_threshold(),
        }
    }
}

// ── Defaults ─────────────────────────────────────────────────────────────

fn default_host() -> String {
    "0.0.0.0".into()
}
fn default_port() -> u16 {
    3000
}
fn default_grpc_port() -> u16 {
    4001
}
fn default_rails_url() -> String {
    "http://localhost:5001".into()
}
fn default_pool_size() -> u32 {
    20
}
fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".into()
}
fn default_solana_rpc() -> String {
    "https://api.mainnet-beta.solana.com".into()
}
fn default_solana_ws() -> String {
    "wss://api.mainnet-beta.solana.com".into()
}
fn default_treasury_program() -> String {
    "8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s".into()
}
fn default_governance_program() -> String {
    "245xpoWLEAAPmUQxMSBDqQw5qnGfqt5roi5enuFG9fZZ".into()
}
fn default_bounty_program() -> String {
    "4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq".into()
}
fn default_mint_address() -> Option<String> {
    Some("5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ".into())
}
fn default_treasury_token_account() -> Option<String> {
    Some("9xDVHuW4kiUYH5NPDLFfKhpxLQ31N6bqMrvj4EJ57z2B".into())
}
fn default_aws_region() -> String {
    "us-west-2".into()
}
fn default_model() -> String {
    "us.anthropic.claude-sonnet-4-20250514-v1:0".into()
}
fn default_chat_model() -> String {
    "us.anthropic.claude-sonnet-4-20250514-v1:0".into()
}
fn default_voice_model() -> String {
    "us.anthropic.claude-3-5-haiku-20241022-v1:0".into()
}
fn default_max_iterations() -> usize {
    25
}
fn default_max_context_tokens() -> usize {
    200_000
}
fn default_token_budget() -> usize {
    30_000
}
fn default_harness_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}
fn default_auto_update() -> bool {
    true
}
fn default_platform_url() -> String {
    "http://localhost:4000".into()
}
fn default_heartbeat_interval() -> u64 {
    30
}
fn default_sync_interval() -> u64 {
    300
}
fn default_activity_interval() -> u64 {
    60
}
fn default_telemetry_enabled() -> bool {
    true
}
fn default_custom_context_window() -> usize {
    131_072
}
fn default_custom_tier() -> u8 {
    2
}
fn default_jwt_secret() -> SecretString {
    // SECURITY: In production, AMOS__AUTH__JWT_SECRET MUST be set to a strong random value.
    // This default exists only to allow local dev startup. The harness logs a critical
    // warning at boot if this default is active (see harness startup checks).
    let default = "INSECURE-LOCAL-DEV-ONLY-set-AMOS__AUTH__JWT_SECRET";
    tracing::error!(
        "JWT secret not configured! Set AMOS__AUTH__JWT_SECRET to a strong random value. \
         Using an insecure default that MUST NOT be used in production."
    );
    SecretString::from(default.to_string())
}
fn default_access_token_expiry() -> u64 {
    3600
} // 1 hour
fn default_refresh_token_expiry() -> u64 {
    604_800
} // 7 days
fn default_base_domain() -> String {
    "localhost".into()
}
fn default_relay_url() -> String {
    "http://localhost:4100".into()
}
fn default_relay_enabled() -> bool {
    false
}
fn default_relay_heartbeat_interval() -> u64 {
    30
}
fn default_relay_bounty_sync_interval() -> u64 {
    60
}
fn default_relay_reputation_interval() -> u64 {
    300
}
fn default_embedding_model() -> String {
    "text-embedding-3-small".into()
}
fn default_embedding_api_base() -> String {
    "https://api.openai.com/v1".into()
}
fn default_fleet_enabled() -> bool {
    false
}
fn default_fleet_max_agents() -> u32 {
    10
}
fn default_fleet_polling_interval() -> u64 {
    60
}
fn default_fleet_backoff_max() -> u64 {
    300
}
fn default_fleet_auto_scale() -> bool {
    false
}
fn default_fleet_min_fit_score() -> f64 {
    0.5
}
fn default_fleet_agent_context_path() -> String {
    "AGENT_CONTEXT.md".into()
}
fn default_local_model_provider() -> String {
    "ollama".into()
}
fn default_local_model_api_base() -> String {
    "http://localhost:11434/v1".into()
}
fn default_local_model_id() -> String {
    "llama3.2:3b".into()
}
fn default_local_model_cost_threshold() -> u64 {
    500
}

impl AppConfig {
    /// Load configuration from environment variables and optional config files.
    ///
    /// Layering order (later overrides earlier):
    /// 1. Compiled defaults (above)
    /// 2. `config/default.toml` (if present)
    /// 3. `config/{AMOS_ENV}.toml` (if present)
    /// 4. Environment variables prefixed with `AMOS_`
    pub fn load() -> crate::Result<Self> {
        dotenvy::dotenv().ok();

        let env = std::env::var("AMOS_ENV").unwrap_or_else(|_| "development".into());

        let settings = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name(&format!("config/{env}")).required(false))
            .add_source(
                config::Environment::with_prefix("AMOS")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()
            .map_err(|e| crate::AmosError::Config(e.to_string()))?;

        settings
            .try_deserialize()
            .map_err(|e| crate::AmosError::Config(e.to_string()))
    }

    /// Whether this is a self-hosted deployment.
    pub fn is_self_hosted(&self) -> bool {
        self.deployment.mode == DeploymentMode::SelfHosted
    }

    /// Whether custom models are available and configured.
    pub fn has_custom_models(&self) -> bool {
        self.custom_models.enabled && !self.custom_models.providers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_model_config_defaults() {
        let config = LocalModelConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.api_base, "http://localhost:11434/v1");
        assert_eq!(config.model_id, "llama3.2:3b");
        assert_eq!(config.cost_threshold, 500);
    }

    #[test]
    fn fleet_config_has_local_model_when_enabled() {
        let mut config = FleetConfig::default();
        assert!(!config.has_local_model());
        config.local_model.enabled = true;
        assert!(config.has_local_model());
    }

    #[test]
    fn fleet_config_no_local_model_with_empty_base() {
        let mut config = FleetConfig::default();
        config.local_model.enabled = true;
        config.local_model.api_base = String::new();
        assert!(!config.has_local_model());
    }

    #[test]
    fn fleet_config_defaults() {
        let config = FleetConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_agents, 10);
        assert_eq!(config.polling_interval_secs, 60);
        assert_eq!(config.backoff_max_secs, 300);
        assert!(!config.auto_scale);
        assert!((config.min_fit_score - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.agent_context_path, "AGENT_CONTEXT.md");
    }

    #[test]
    fn local_model_config_deserialize() {
        let json = r#"{
            "enabled": true,
            "provider": "vllm",
            "api_base": "http://gpu-box:8000/v1",
            "model_id": "qwen2.5:7b",
            "cost_threshold": 1000
        }"#;
        let config: LocalModelConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.provider, "vllm");
        assert_eq!(config.api_base, "http://gpu-box:8000/v1");
        assert_eq!(config.model_id, "qwen2.5:7b");
        assert_eq!(config.cost_threshold, 1000);
    }

    #[test]
    fn local_model_config_deserialize_minimal() {
        // Only enabled, rest should use defaults
        let json = r#"{"enabled": true}"#;
        let config: LocalModelConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.api_base, "http://localhost:11434/v1");
    }
}
