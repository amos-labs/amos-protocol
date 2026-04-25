//! Live smoke tests — require real AWS creds + Bedrock access. All `#[ignore]`
//! by default. Run with:
//!
//! ```bash
//! cargo test -p amos-oracle --test live_smoke -- --ignored --nocapture
//! ```
//!
//! Env vars honored:
//!   - AWS_REGION (default: us-east-1)
//!   - AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / AWS_SESSION_TOKEN (else
//!     falls back to ~/.aws/credentials)
//!   - ORACLE_BEDROCK_MODEL_ID (override model)

use amos_oracle::bedrock::BedrockLlmClient;
use amos_oracle::llm::LlmClient;

#[tokio::test]
#[ignore = "live-bedrock: requires AWS creds + Bedrock model access"]
async fn bedrock_ping_pong() {
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".into());
    let model_id = std::env::var("ORACLE_BEDROCK_MODEL_ID").ok();

    let client = BedrockLlmClient::new(Some(region), None, None, model_id)
        .expect("failed to build BedrockLlmClient — check AWS creds");

    let reply = client
        .complete(
            "You are a ping-pong bot. Reply with exactly one word: pong. \
             No punctuation, no prose, no markdown.",
            "ping",
        )
        .await
        .expect("bedrock complete failed");

    println!("bedrock reply: {reply:?}");
    assert!(
        reply.to_lowercase().contains("pong"),
        "expected reply to contain 'pong', got: {reply:?}"
    );
}

#[tokio::test]
#[ignore = "live-bedrock: requires AWS creds + Bedrock model access"]
async fn bedrock_structured_json_roundtrip() {
    // Verifies the model returns valid JSON when asked to. This is the key
    // invariant for Oracle's structured-output pipeline.
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".into());
    let model_id = std::env::var("ORACLE_BEDROCK_MODEL_ID").ok();

    let client = BedrockLlmClient::new(Some(region), None, None, model_id)
        .expect("failed to build BedrockLlmClient");

    let reply = client
        .complete(
            "You are a strict JSON emitter. Given the user's word, output JSON \
             matching schema: {\"word\": <string>, \"letters\": <u32>}. Emit \
             ONLY the JSON object, no fences, no prose.",
            "hello",
        )
        .await
        .expect("bedrock complete failed");

    println!("bedrock reply: {reply:?}");

    // Parse — tolerate common wrappers (code fences, leading prose)
    let trimmed = reply.trim();
    let cleaned = if let Some(rest) = trimmed.strip_prefix("```json") {
        rest.trim_end_matches("```").trim()
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest.trim_end_matches("```").trim()
    } else {
        trimmed
    };

    let parsed: serde_json::Value =
        serde_json::from_str(cleaned).expect("reply was not valid JSON");

    assert_eq!(parsed["word"].as_str(), Some("hello"));
    assert_eq!(parsed["letters"].as_u64(), Some(5));
}
