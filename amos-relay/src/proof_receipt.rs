//! AMOS-META-007 — proof-carrying bounty receipt shape validator.
//!
//! Two-tier validation per `docs/AMOS_PROOF_CARRYING_DEV_PIPELINE.md` §4:
//!
//! - This module enforces **shape** — required fields, types, sizes, basic
//!   format checks. A receipt with all required fields filled with garbage
//!   passes shape validation. That's intentional.
//! - **Semantic content** — whether the validation plan covers the changes,
//!   whether commands are the right ones, whether override reasons are
//!   substantive — is judged by Oracle review and council. Resist the urge
//!   to grow this module into content validation; the boundary matters.
//!
//! Validation runs at submission time (`POST /bounties/{id}/submit`) when a
//! `proof_receipt` field is present. Phase 2: optional. Phase 5: required
//! for code bounties before approval.

use serde_json::Value as JsonValue;

/// Hard cap on serialized receipt size. Larger evidence (full logs etc.)
/// belongs at `evidence_log_uri`, not inline. 256 kB matches the spec's
/// open-question recommendation.
pub const MAX_RECEIPT_BYTES: usize = 256 * 1024;

/// Minimum length for an override or skipped-check reason. Generic short
/// strings ("n/a", "skipped") fail validation.
pub const MIN_REASON_LEN: usize = 40;

/// Required by §5: 40-char hex SHA, lowercase per git convention.
pub const HEAD_SHA_LEN: usize = 40;

/// Validate a proof receipt's shape. Returns `Ok(())` on pass, or
/// `Err(message)` with a caller-facing description of the first failure.
pub fn validate(receipt: &JsonValue) -> Result<(), String> {
    let serialized = serde_json::to_string(receipt)
        .map_err(|e| format!("receipt is not serializable JSON: {e}"))?;
    if serialized.len() > MAX_RECEIPT_BYTES {
        return Err(format!(
            "receipt size {} exceeds cap {} bytes; large logs belong at execution_evidence.evidence_log_uri",
            serialized.len(),
            MAX_RECEIPT_BYTES
        ));
    }

    require_string(receipt, "receipt_version")?;
    require_string(receipt, "bounty_id")?;
    require_string(receipt, "agent_id")?;

    let intent = require_object(receipt, "intent")?;
    require_string_field(intent, "intent.summary")?;
    require_bool(intent, "self_modifying", "intent")?;
    require_string_field(intent, "intent.scope_classification")?;

    let _policy = require_object(receipt, "policy")?;
    // Policy fields are optional individually; the existence of the policy
    // block itself is required so the receipt is forced to declare *something*
    // about constraints, even if everything is empty. Content is judged by
    // Oracle.

    let plan = require_object(receipt, "validation_plan")?;
    let selected = plan
        .get("selected_checks")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "validation_plan.selected_checks must be an array".to_string())?;
    if selected.is_empty() {
        return Err(
            "validation_plan.selected_checks must be non-empty (a 'no checks' submission \
             is what skipped_checks is for, and even then at least one selected check is \
             required to anchor the plan)"
                .to_string(),
        );
    }
    for (i, check) in selected.iter().enumerate() {
        let check = check
            .as_object()
            .ok_or_else(|| format!("validation_plan.selected_checks[{i}] must be an object"))?;
        if check.get("id").and_then(JsonValue::as_str).unwrap_or("").is_empty() {
            return Err(format!(
                "validation_plan.selected_checks[{i}].id is required and non-empty"
            ));
        }
        if check
            .get("rationale")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err(format!(
                "validation_plan.selected_checks[{i}].rationale is required and non-empty"
            ));
        }
    }
    if let Some(skipped) = plan.get("skipped_checks").and_then(JsonValue::as_array) {
        for (i, check) in skipped.iter().enumerate() {
            let check = check.as_object().ok_or_else(|| {
                format!("validation_plan.skipped_checks[{i}] must be an object")
            })?;
            if check.get("id").and_then(JsonValue::as_str).unwrap_or("").is_empty() {
                return Err(format!(
                    "validation_plan.skipped_checks[{i}].id is required"
                ));
            }
            let reason = check
                .get("reason")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim();
            if reason.len() < MIN_REASON_LEN {
                return Err(format!(
                    "validation_plan.skipped_checks[{i}].reason must be ≥{MIN_REASON_LEN} chars; \
                     generic 'skipped' / 'n/a' fails validation per spec §7"
                ));
            }
        }
    }

    let evidence = require_object(receipt, "execution_evidence")?;
    let commands = evidence
        .get("commands")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "execution_evidence.commands must be an array".to_string())?;
    if commands.is_empty() {
        return Err("execution_evidence.commands must be non-empty".to_string());
    }
    for (i, cmd) in commands.iter().enumerate() {
        let cmd = cmd.as_object().ok_or_else(|| {
            format!("execution_evidence.commands[{i}] must be an object")
        })?;
        for f in ["id", "command", "started_at", "ended_at"] {
            if cmd.get(f).and_then(JsonValue::as_str).unwrap_or("").is_empty() {
                return Err(format!(
                    "execution_evidence.commands[{i}].{f} is required and non-empty"
                ));
            }
        }
        if cmd.get("exit_code").and_then(JsonValue::as_i64).is_none() {
            return Err(format!(
                "execution_evidence.commands[{i}].exit_code must be an integer"
            ));
        }
    }
    let source = evidence
        .get("evidence_source")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    if !matches!(source, "agent_reported" | "github_api" | "qa_reported") {
        return Err(
            "execution_evidence.evidence_source must be one of: agent_reported, github_api, qa_reported"
                .to_string(),
        );
    }

    let github = require_object(receipt, "github")?;
    let pr_url = github
        .get("pr_url")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim();
    if !pr_url.starts_with("https://github.com/") || pr_url.len() < 25 {
        return Err("github.pr_url must be an https://github.com/... URL".to_string());
    }
    let head_sha = github
        .get("head_sha")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim();
    if head_sha.len() != HEAD_SHA_LEN || !head_sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "github.head_sha must be {HEAD_SHA_LEN}-char hex (full git SHA-1, lowercase)"
        ));
    }
    require_string_field(github, "github.branch")?;
    let changed = github
        .get("changed_files")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "github.changed_files must be an array".to_string())?;
    if changed.is_empty() {
        return Err("github.changed_files must be non-empty".to_string());
    }

    require_string(receipt, "result_summary")?;

    Ok(())
}

// ─── helpers ────────────────────────────────────────────────────────────

fn require_string(receipt: &JsonValue, field: &str) -> Result<(), String> {
    let v = receipt
        .get(field)
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim();
    if v.is_empty() {
        return Err(format!("{field} is required and non-empty"));
    }
    Ok(())
}

fn require_object<'a>(
    receipt: &'a JsonValue,
    field: &str,
) -> Result<&'a serde_json::Map<String, JsonValue>, String> {
    receipt
        .get(field)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| format!("{field} is required and must be an object"))
}

fn require_string_field(
    obj: &serde_json::Map<String, JsonValue>,
    label: &str,
) -> Result<(), String> {
    let key = label.rsplit('.').next().unwrap_or(label);
    let v = obj.get(key).and_then(JsonValue::as_str).unwrap_or("").trim();
    if v.is_empty() {
        return Err(format!("{label} is required and non-empty"));
    }
    Ok(())
}

fn require_bool(
    obj: &serde_json::Map<String, JsonValue>,
    key: &str,
    parent: &str,
) -> Result<(), String> {
    if !obj.get(key).map(JsonValue::is_boolean).unwrap_or(false) {
        return Err(format!("{parent}.{key} is required and must be a boolean"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_receipt() -> JsonValue {
        serde_json::json!({
            "receipt_version": "1",
            "bounty_id": "11111111-1111-1111-1111-111111111111",
            "agent_id": "22222222-2222-2222-2222-222222222222",
            "intent": {
                "summary": "Add X-Request-ID header to /settle endpoint responses for traceability.",
                "self_modifying": false,
                "scope_classification": "infra"
            },
            "policy": {
                "forbidden_paths": [],
                "required_paths_subset": ["amos-relay/**"],
                "scope_constraint_ids": [],
                "minimum_coverage_pct": null,
                "max_file_size_bytes": null
            },
            "validation_plan": {
                "selected_checks": [
                    { "id": "cargo-test-lib", "rationale": "Rust crate change" },
                    { "id": "cargo-clippy-deny-warnings", "rationale": "lint gate" }
                ],
                "skipped_checks": [],
                "selection_method": "static-rules-v1"
            },
            "execution_evidence": {
                "commands": [
                    {
                        "id": "cargo-test-lib",
                        "command": "cargo test --lib --workspace",
                        "exit_code": 0,
                        "stdout_excerpt": "115 passed",
                        "duration_ms": 12340,
                        "started_at": "2026-04-28T17:00:00Z",
                        "ended_at": "2026-04-28T17:00:12Z"
                    }
                ],
                "evidence_source": "agent_reported"
            },
            "github": {
                "pr_url": "https://github.com/amos-labs/amos-platform-2.0/pull/42",
                "head_sha": "0123456789abcdef0123456789abcdef01234567",
                "branch": "feature/x-request-id",
                "changed_files": [
                    { "path": "amos-relay/src/middleware.rs", "additions": 14, "deletions": 0 }
                ]
            },
            "result_summary": "Added X-Request-ID middleware emitting UUID v4 on all responses."
        })
    }

    #[test]
    fn good_receipt_passes() {
        validate(&good_receipt()).unwrap();
    }

    #[test]
    fn missing_intent_fails() {
        let mut r = good_receipt();
        r.as_object_mut().unwrap().remove("intent");
        let err = validate(&r).unwrap_err();
        assert!(err.contains("intent"));
    }

    #[test]
    fn empty_validation_plan_fails() {
        let mut r = good_receipt();
        r["validation_plan"]["selected_checks"] = serde_json::json!([]);
        let err = validate(&r).unwrap_err();
        assert!(err.contains("selected_checks"));
    }

    #[test]
    fn short_skipped_reason_fails() {
        let mut r = good_receipt();
        r["validation_plan"]["skipped_checks"] = serde_json::json!([
            { "id": "cargo-test-doc", "reason": "n/a" }
        ]);
        let err = validate(&r).unwrap_err();
        assert!(err.contains("≥40") || err.contains("MIN_REASON_LEN"));
    }

    #[test]
    fn empty_commands_fails() {
        let mut r = good_receipt();
        r["execution_evidence"]["commands"] = serde_json::json!([]);
        assert!(validate(&r).unwrap_err().contains("commands"));
    }

    #[test]
    fn invalid_evidence_source_fails() {
        let mut r = good_receipt();
        r["execution_evidence"]["evidence_source"] = serde_json::json!("worker_said_so");
        assert!(validate(&r).unwrap_err().contains("evidence_source"));
    }

    #[test]
    fn bad_pr_url_fails() {
        let mut r = good_receipt();
        r["github"]["pr_url"] = serde_json::json!("not-a-github-url");
        assert!(validate(&r).unwrap_err().contains("pr_url"));
    }

    #[test]
    fn bad_head_sha_fails() {
        let mut r = good_receipt();
        r["github"]["head_sha"] = serde_json::json!("short");
        assert!(validate(&r).unwrap_err().contains("head_sha"));

        let mut r2 = good_receipt();
        r2["github"]["head_sha"] = serde_json::json!("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
        assert!(validate(&r2).unwrap_err().contains("head_sha"));
    }

    #[test]
    fn missing_changed_files_fails() {
        let mut r = good_receipt();
        r["github"]["changed_files"] = serde_json::json!([]);
        assert!(validate(&r).unwrap_err().contains("changed_files"));
    }

    #[test]
    fn oversize_receipt_fails() {
        let mut r = good_receipt();
        r["result_summary"] = serde_json::json!("x".repeat(MAX_RECEIPT_BYTES));
        assert!(validate(&r).unwrap_err().contains("size"));
    }

    #[test]
    fn missing_self_modifying_flag_fails() {
        let mut r = good_receipt();
        r["intent"].as_object_mut().unwrap().remove("self_modifying");
        assert!(validate(&r).unwrap_err().contains("self_modifying"));
    }
}
