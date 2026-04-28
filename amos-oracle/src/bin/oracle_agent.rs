//! `amos-oracle-agent` — daemon binary.
//!
//! Loads real dependencies (Bedrock, Postgres, AMOS mission source, relay
//! metrics), constructs an [`OracleAgent`], and runs a poll loop over pending
//! intake submissions + review requests from the relay.
//!
//! Each tick:
//!   1. Fetch pending intakes → dispatch `agent.intake(...)` → create bounty
//!      if commission, escalation if escalate, else close out with the verdict.
//!   2. Fetch bounties awaiting mission-alignment review → dispatch
//!      `agent.review(...)` → approve/reject/request-revision, or escalate.
//!
//! # Configuration (env vars)
//!
//! Required:
//!   - `DATABASE_URL`                 — Postgres DSN for oracle_decisions/outcomes
//!   - `ORACLE_RELAY_URL`             — relay base URL (e.g. https://relay.amoslabs.com)
//!   - `ORACLE_RELAY_API_KEY`         — bearer token for relay writes
//!   - `AWS_REGION`                   — Bedrock region (e.g. us-east-1)
//!   - `ORACLE_POSTER_WALLET`         — Solana wallet used as poster for
//!     Oracle-commissioned bounties
//!
//! Optional:
//!   - `ORACLE_PROJECT_ROOT`          — for mission file paths (default: cwd)
//!   - `ORACLE_PROMPT_VERSION`        — constitutional prompt version tag
//!     (default: "v1-draft-2026-04-23")
//!   - `ORACLE_POLL_INTERVAL_SECS`    — tick period (default: 60)
//!   - `ORACLE_BEDROCK_MODEL_ID`      — override Bedrock model id
//!   - `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN`
//!     — explicit creds (else SDK chain)

use std::env;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use amos_oracle::agent::OracleAgent;
use amos_oracle::bedrock::{BedrockLlmClient, DEFAULT_MODEL_ID};
use amos_oracle::decision::{
    Decision, DecisionPath, IntakeVerdict, ProposedBountySpec, ReviewVerdict,
};
use amos_oracle::intake::IntakeSubmission;
use amos_oracle::metrics::AmosMetricsProvider;
use amos_oracle::mission::AmosMissionSource;
use amos_oracle::precedent::PgEventLog;
use amos_oracle::registry::AmosContributionRegistry;
use amos_oracle::review::ReviewRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tracing::{error, info, warn};
use uuid::Uuid;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,amos_oracle=debug")),
        )
        .init();

    info!("amos-oracle-agent: starting");

    match run().await {
        Ok(()) => {
            info!("amos-oracle-agent: clean shutdown");
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!(error = %e, "amos-oracle-agent: fatal error");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let cfg = Arc::new(Config::from_env()?);
    info!(
        relay = %cfg.relay_url,
        region = %cfg.aws_region,
        prompt_version = %cfg.prompt_version,
        poll_interval_secs = cfg.poll_interval_secs,
        "loaded config"
    );

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&cfg.database_url)
        .await?;
    info!("db pool ready");

    let mission = Arc::new(AmosMissionSource::new(
        cfg.project_root.clone(),
        cfg.prompt_version.clone(),
    ));
    let metrics = Arc::new(AmosMetricsProvider::new(
        cfg.relay_url.clone(),
        cfg.relay_api_key.clone(),
    ));
    let registry = Arc::new(AmosContributionRegistry::new());
    let event_log = Arc::new(PgEventLog::new(pool.clone()));
    let llm = Arc::new(build_bedrock_client(&cfg).await?);

    let agent = Arc::new(
        OracleAgent::builder()
            .mission(mission)
            .metrics(metrics)
            .registry(registry)
            .event_log(event_log)
            .llm(llm)
            .prompt_version(&cfg.prompt_version)
            .model_version(cfg.model_id.clone())
            .build()?,
    );

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let relay = Arc::new(RelayClient {
        http,
        base: cfg.relay_url.trim_end_matches('/').to_string(),
        api_key: cfg.relay_api_key.clone(),
    });

    info!("oracle agent constructed; entering poll loop");

    let mut tick_count: u64 = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(cfg.poll_interval_secs));
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                tick_count = tick_count.saturating_add(1);
                if let Err(e) = tick(agent.clone(), relay.clone(), &cfg, tick_count).await {
                    warn!(error = %e, tick = tick_count, "tick failed (non-fatal)");
                }
            }
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
        }
    }
}

async fn tick(
    agent: Arc<OracleAgent>,
    relay: Arc<RelayClient>,
    cfg: &Config,
    tick_count: u64,
) -> anyhow::Result<()> {
    info!(tick = tick_count, "tick: polling");

    // ── Intake pass ──────────────────────────────────────────────────
    let intakes = relay.list_pending_intakes().await.unwrap_or_else(|e| {
        warn!(error = %e, "tick: failed to fetch pending intakes; skipping intake pass");
        vec![]
    });
    for raw in intakes {
        let sub = raw.into_agent_submission();
        let sub_id = sub.submission_id;
        match agent.intake(sub).await {
            Ok(decision) => {
                if let Err(e) = dispatch_intake_decision(&relay, cfg, sub_id, &decision).await {
                    warn!(submission_id = %sub_id, error = %e, "intake dispatch failed");
                }
            }
            Err(e) => {
                warn!(submission_id = %sub_id, error = %e, "intake evaluation failed");
            }
        }
    }

    // ── Review pass ──────────────────────────────────────────────────
    let reviews = relay.list_pending_reviews().await.unwrap_or_else(|e| {
        warn!(error = %e, "tick: failed to fetch pending reviews; skipping review pass");
        vec![]
    });
    for raw in reviews {
        let req = raw.into_agent_request();
        let bid = req.bounty_id;
        match agent.review(req).await {
            Ok(decision) => {
                if let Err(e) = dispatch_review_decision(&relay, bid, &decision).await {
                    warn!(bounty_id = %bid, error = %e, "review dispatch failed");
                }
            }
            Err(e) => {
                warn!(bounty_id = %bid, error = %e, "review evaluation failed");
            }
        }
    }

    Ok(())
}

async fn dispatch_intake_decision(
    relay: &RelayClient,
    cfg: &Config,
    submission_id: Uuid,
    decision: &Decision,
) -> anyhow::Result<()> {
    let verdict: IntakeVerdict = serde_json::from_value(decision.verdict.clone())
        .map_err(|e| anyhow::anyhow!("intake verdict decode: {e}"))?;

    let mut commissioned_bounty_id: Option<Uuid> = None;

    match verdict {
        IntakeVerdict::Commission => {
            let spec = decision.proposed_bounty_spec.as_ref().ok_or_else(|| {
                anyhow::anyhow!("commission decision without proposed_bounty_spec")
            })?;
            let bounty_id = relay.create_bounty_from_spec(cfg, spec).await?;
            commissioned_bounty_id = Some(bounty_id);
            info!(
                submission_id = %submission_id,
                bounty_id = %bounty_id,
                "intake: commissioned"
            );
        }
        IntakeVerdict::Reject => {
            info!(submission_id = %submission_id, "intake: rejected");
        }
        IntakeVerdict::Refine => {
            info!(
                submission_id = %submission_id,
                feedback_len = decision.feedback.as_deref().map(str::len).unwrap_or(0),
                "intake: refine feedback issued"
            );
        }
        IntakeVerdict::Escalate => {
            relay
                .create_escalation(decision.decision_id, "intake", &escalation_reason(decision))
                .await?;
            info!(submission_id = %submission_id, "intake: escalated to council");
        }
    }

    relay
        .record_intake_evaluation(
            submission_id,
            verdict_to_str(&decision.verdict),
            decision.decision_id,
            commissioned_bounty_id,
        )
        .await?;

    Ok(())
}

async fn dispatch_review_decision(
    relay: &RelayClient,
    bounty_id: Uuid,
    decision: &Decision,
) -> anyhow::Result<()> {
    let verdict: ReviewVerdict = serde_json::from_value(decision.verdict.clone())
        .map_err(|e| anyhow::anyhow!("review verdict decode: {e}"))?;

    match verdict {
        ReviewVerdict::Approve => {
            relay.approve_bounty(bounty_id, decision).await?;
            info!(bounty_id = %bounty_id, "review: approved");
        }
        ReviewVerdict::Reject => {
            relay
                .reject_bounty(bounty_id, &decision.mission_alignment_notes)
                .await?;
            info!(bounty_id = %bounty_id, "review: rejected");
        }
        ReviewVerdict::Revise => {
            let feedback = decision
                .feedback
                .clone()
                .unwrap_or_else(|| decision.mission_alignment_notes.clone());
            relay.request_revision(bounty_id, &feedback).await?;
            info!(bounty_id = %bounty_id, "review: revision requested");
        }
        ReviewVerdict::Escalate => {
            relay
                .create_escalation(decision.decision_id, "review", &escalation_reason(decision))
                .await?;
            info!(bounty_id = %bounty_id, "review: escalated to council");
        }
    }

    Ok(())
}

/// AMOS-META-007: derive an initial policy block for an Oracle-commissioned
/// bounty. Conservative defaults — Oracle never commissions reasoning-substrate
/// changes (those escalate), so `self_modifying = false` is safe here. Manual
/// posters and council overrides can supply richer policies explicitly.
fn derive_policy_from_spec(spec: &ProposedBountySpec) -> Option<JsonValue> {
    let category = spec.category.trim().to_ascii_lowercase();
    let forbidden_paths: Vec<&str> = vec![
        // Reasoning substrate — Oracle should never commission against these,
        // but make it explicit in the policy so receipts can be checked.
        "amos-oracle/prompts/**",
        "amos-oracle/src/agent.rs",
        "amos-oracle/src/intake.rs",
        "amos-oracle/src/review.rs",
    ];
    Some(serde_json::json!({
        "forbidden_paths": forbidden_paths,
        "required_paths_subset": [],
        "scope_constraint_ids": [format!("category:{category}")],
        "minimum_coverage_pct": null,
        "max_file_size_bytes": null,
        "self_modifying": false,
    }))
}

fn verdict_to_str(v: &JsonValue) -> &'static str {
    match v.as_str() {
        Some("commission") => "commission",
        Some("reject") => "reject",
        Some("refine") => "refine",
        Some("escalate") => "escalate",
        Some("approve") => "approve",
        Some("revise") => "revise",
        _ => "unknown",
    }
}

fn escalation_reason(decision: &Decision) -> String {
    format!(
        "[{}] confidence={:.2}\n\n{}",
        match decision.path {
            DecisionPath::Intake => "intake",
            DecisionPath::Review => "review",
        },
        decision.confidence.0,
        decision.mission_alignment_notes
    )
}

// ─── Relay HTTP client ──────────────────────────────────────────────────

struct RelayClient {
    http: reqwest::Client,
    base: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct IntakeRow {
    submission_id: Uuid,
    title: String,
    body: String,
    submitter: String,
    #[serde(default)]
    parent_submission_id: Option<Uuid>,
    #[serde(default)]
    suggested_category: Option<String>,
    #[serde(default)]
    suggested_capabilities: Vec<String>,
}

impl IntakeRow {
    fn into_agent_submission(self) -> IntakeSubmission {
        IntakeSubmission {
            submission_id: self.submission_id,
            title: self.title,
            body: self.body,
            submitter: self.submitter,
            parent_submission_id: self.parent_submission_id,
            suggested_category: self.suggested_category,
            suggested_capabilities: self.suggested_capabilities,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PendingReviewRow {
    bounty_id: Uuid,
    bounty_title: String,
    bounty_description: String,
    bounty_category: String,
    bounty_contribution_type_id: u8,
    qa_evidence: JsonValue,
    proof: JsonValue,
    revision_count: u8,
}

impl PendingReviewRow {
    fn into_agent_request(self) -> ReviewRequest {
        ReviewRequest {
            bounty_id: self.bounty_id,
            bounty_title: self.bounty_title,
            bounty_description: self.bounty_description,
            bounty_category: self.bounty_category,
            bounty_contribution_type_id: self.bounty_contribution_type_id,
            qa_evidence: self.qa_evidence,
            proof: self.proof,
            revision_count: self.revision_count,
        }
    }
}

#[derive(Debug, Serialize)]
struct CreateBountyBody<'a> {
    title: &'a str,
    description: &'a str,
    reward_tokens: u64,
    deadline: chrono::DateTime<chrono::Utc>,
    required_capabilities: &'a [String],
    poster_wallet: &'a str,
    category: &'a str,
    /// AMOS-META-007: optional policy block. Oracle commissioning attaches
    /// constraints inferred from the bounty's contribution type + scope so
    /// future submissions can be judged against them.
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
struct CreateBountyResponse {
    id: Uuid,
}

impl RelayClient {
    async fn list_pending_intakes(&self) -> anyhow::Result<Vec<IntakeRow>> {
        let url = format!("{}/api/v1/intakes?status=pending", self.base);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("list_pending_intakes: {}", resp.status());
        }
        let rows = resp.json::<Vec<IntakeRow>>().await?;
        Ok(rows)
    }

    async fn list_pending_reviews(&self) -> anyhow::Result<Vec<PendingReviewRow>> {
        let url = format!("{}/api/v1/bounties/pending-review", self.base);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("list_pending_reviews: {}", resp.status());
        }
        let rows = resp.json::<Vec<PendingReviewRow>>().await?;
        Ok(rows)
    }

    async fn create_bounty_from_spec(
        &self,
        cfg: &Config,
        spec: &ProposedBountySpec,
    ) -> anyhow::Result<Uuid> {
        let deadline =
            chrono::Utc::now() + chrono::Duration::days(spec.deadline_days.max(1) as i64);
        let policy = derive_policy_from_spec(spec);
        let body = CreateBountyBody {
            title: &spec.title,
            description: &spec.description,
            reward_tokens: spec.reward_points,
            deadline,
            required_capabilities: &spec.required_capabilities,
            poster_wallet: &cfg.poster_wallet,
            category: &spec.category,
            policy,
        };

        let url = format!("{}/api/v1/bounties", self.base);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let s = resp.status();
            let t = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "create_bounty {}: {}",
                s,
                t.chars().take(300).collect::<String>()
            );
        }
        let out = resp.json::<CreateBountyResponse>().await?;
        Ok(out.id)
    }

    async fn create_escalation(
        &self,
        decision_id: Uuid,
        path: &str,
        reason: &str,
    ) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/escalations", self.base);
        let body = serde_json::json!({
            "decision_id": decision_id,
            "path": path,
            "reason": reason,
        });
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("create_escalation: {}", resp.status());
        }
        Ok(())
    }

    async fn record_intake_evaluation(
        &self,
        submission_id: Uuid,
        verdict: &str,
        decision_id: Uuid,
        commissioned_bounty_id: Option<Uuid>,
    ) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/intakes/{}/evaluation", self.base, submission_id);
        let body = serde_json::json!({
            "verdict": verdict,
            "decision_id": decision_id,
            "commissioned_bounty_id": commissioned_bounty_id,
        });
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("record_intake_evaluation: {}", resp.status());
        }
        Ok(())
    }

    async fn approve_bounty(&self, bounty_id: Uuid, decision: &Decision) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/bounties/{}/approve", self.base, bounty_id);
        let body = serde_json::json!({
            "approver_wallet": "oracle",
            "quality_score": 80u8.saturating_add_signed(
                decision_quality_adjustment(decision).clamp(-20, 20) as i8
            ),
            "notes": decision.mission_alignment_notes,
        });
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("approve_bounty: {}", resp.status());
        }
        Ok(())
    }

    async fn reject_bounty(&self, bounty_id: Uuid, reason: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/bounties/{}/reject", self.base, bounty_id);
        let body = serde_json::json!({
            "reviewer_wallet": "oracle",
            "reason": reason,
        });
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("reject_bounty: {}", resp.status());
        }
        Ok(())
    }

    async fn request_revision(&self, bounty_id: Uuid, feedback: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/v1/bounties/{}/request_revision",
            self.base, bounty_id
        );
        let body = serde_json::json!({
            "reviewer_wallet": "oracle",
            "feedback": feedback,
        });
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("request_revision: {}", resp.status());
        }
        Ok(())
    }
}

/// Pull quality_score_adjustment from the review decision payload if present.
/// ReviewLlmOutput isn't carried on Decision directly; we look it up in the
/// stored verdict/payload. Returns 0 if not found.
fn decision_quality_adjustment(_decision: &Decision) -> i64 {
    // Adjustment lives inside the LLM output but Decision doesn't carry it
    // separately. Default neutral; future enhancement is to thread it through
    // Decision explicitly.
    0
}

// ─── Bedrock + Config ───────────────────────────────────────────────────

async fn build_bedrock_client(cfg: &Config) -> anyhow::Result<BedrockLlmClient> {
    BedrockLlmClient::new(
        Some(cfg.aws_region.clone()),
        None,
        None,
        Some(cfg.model_id.clone()),
    )
    .map_err(|e| anyhow::anyhow!("bedrock client init failed: {e}"))
}

struct Config {
    database_url: String,
    relay_url: String,
    relay_api_key: String,
    aws_region: String,
    project_root: std::path::PathBuf,
    prompt_version: String,
    poll_interval_secs: u64,
    model_id: String,
    poster_wallet: String,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        fn req(key: &str) -> anyhow::Result<String> {
            env::var(key).map_err(|_| anyhow::anyhow!("missing required env var: {}", key))
        }
        fn opt(key: &str, default: &str) -> String {
            env::var(key).unwrap_or_else(|_| default.to_string())
        }

        Ok(Self {
            database_url: req("DATABASE_URL")?,
            relay_url: req("ORACLE_RELAY_URL")?,
            relay_api_key: req("ORACLE_RELAY_API_KEY")?,
            aws_region: req("AWS_REGION")?,
            poster_wallet: req("ORACLE_POSTER_WALLET")?,
            project_root: env::var("ORACLE_PROJECT_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| ".".into())),
            prompt_version: opt("ORACLE_PROMPT_VERSION", "v1-draft-2026-04-23"),
            poll_interval_secs: opt("ORACLE_POLL_INTERVAL_SECS", "60")
                .parse::<u64>()
                .map_err(|e| anyhow::anyhow!("ORACLE_POLL_INTERVAL_SECS not a u64: {e}"))?
                .max(5),
            model_id: opt("ORACLE_BEDROCK_MODEL_ID", DEFAULT_MODEL_ID),
        })
    }
}
