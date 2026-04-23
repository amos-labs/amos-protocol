//! `amos-oracle-agent` — daemon binary.
//!
//! Stub. Real implementation will:
//!   1. Load config (relay URL, wallet keypair, mission paths, LLM provider)
//!   2. Construct [`OracleAgent`] with AMOS-specific trait impls
//!   3. Poll the relay for pending intake submissions + review requests
//!   4. Call `agent.intake()` / `agent.review()` for each
//!   5. Submit verdicts back to the relay (escalate → queue for council;
//!      commission/approve/revise/reject → relay endpoints)
//!   6. Emit structured logs to CloudWatch
//!
//! For now: a startup banner to prove the binary wires up.

use std::process::ExitCode;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,amos_oracle=debug")),
        )
        .init();

    tracing::info!("amos-oracle-agent: scaffold binary — not yet implemented");
    tracing::info!("see amos-oracle/src/bin/oracle_agent.rs for the build-out plan");
    ExitCode::from(0)
}
