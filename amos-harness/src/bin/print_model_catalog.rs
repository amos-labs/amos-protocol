//! Print the shared-Bedrock model catalog, one ID per line.
//!
//! Consumed by the CI `live-model-probe` job to drive the per-model Converse
//! probe. The catalog itself lives in [`amos_harness::routes::settings`] —
//! single source of truth for both the customer dropdown and this probe.
//!
//!     $ cargo run --bin print-model-catalog
//!     us.anthropic.claude-haiku-4-5-20251001-v1:0
//!     us.anthropic.claude-sonnet-4-6
//!     us.anthropic.claude-opus-4-6-v1
//!     us.anthropic.claude-opus-4-7

fn main() {
    for id in amos_harness::routes::settings::catalog_model_ids() {
        println!("{}", id);
    }
}
