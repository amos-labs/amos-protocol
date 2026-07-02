# AMOS Proof Receipt — Open Schema (v1)

**Status: OPEN STANDARD.** This is the one layer that locks in early and irreversibly (see [`../NORTH-STAR.md`](../NORTH-STAR.md)), so it is deliberately kept open: **anyone may implement or verify a proof receipt, and a receipt must be verifiable outside AMOS.** The format will never be proprietary or closed. This document is the canonical schema; the reference implementations track it, not the reverse.

## What a receipt is

A **proof receipt** is one portable, auditable object recording a single operation: *what was intended, what rules applied, what was checked, what happened, and the verdict.* It is the atomic unit of a verified-work economy — the thing an AI (or a human) produces as evidence that work was done correctly, that a third party can check without trusting the producer.

## Schema (v1)

A receipt is a JSON object:

| Field | Type | Meaning |
|---|---|---|
| `receipt_version` | string | Schema version, e.g. `"1"`. Consumers branch on this. |
| `operation` | string | The verb performed, e.g. `deploy`, `db_write`, `set_finance_actual`. |
| `tenant_id` | UUID string | The subject the operation acted on. |
| `actor` | string | Who/what performed it (user, agent, or key identity). |
| `intent` | object | `{ summary: string, self_modifying: bool, scope_classification: string }` — what was meant, whether it modified its own guardrails (RSI-relevant), and the permission scope it claimed. |
| `policy` | object | `{ guardrails: string[] }` — the named guardrails/policies in force for this operation. |
| `inputs` | JSON | The operation's inputs (or a redacted/hashed form for secrets). |
| `validation` | array | The checks run — see below. |
| `outputs` | JSON | The operation's result payload. |
| `result_summary` | string | One-line human-readable outcome. |
| `emitted_at` | RFC 3339 timestamp | When the receipt was produced. |

**Check** (each element of `validation`):

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Stable check name, e.g. `image_pushed`, `health_endpoint`, `write_accepted`. |
| `status` | enum | `passed` \| `failed` \| `skipped`. |
| `detail` | string | Evidence for the check (a digest, an HTTP code, a row count, …). |

## Verdict

The verdict is **derived, not asserted**: a receipt is **verified** iff no check has status `failed` (a `skipped` check does not fail the receipt but is visible). Any consumer computes the verdict the same way from the `validation` array — no field to forge.

## Verifiability (the capture-resistant property)

A receipt is **self-describing**: given the object alone, a third party can (a) read intent + policy + verdict, and (b) where a check's `detail` names re-checkable evidence (a digest, a URL, a query), independently re-run it. Verification MUST NOT require access to AMOS. That is what makes receipts portable trust rather than one vendor's log.

## Reputation = portable history

An actor's reputation is the **exportable, verifiable set of its passed receipts.** It must be possible to export an actor's receipt history and verify it outside AMOS. This deliberately keeps switching costs low — the commitment device described in the North Star.

## Versioning

Additive fields within a major version; a breaking change bumps `receipt_version` and this doc. Older receipts remain valid and verifiable forever.

## Reference implementations (track this spec)

- `amos-platform` `src/proof/` — `OperationReceipt`, emitted by every governed MCP write (deploy, db_write, finance writes, …).
- `proofgate` — the composite gate + CLI that stamps/checks receipts in CI.

These are *implementations of* this standard, not definitions of it. If they drift, the spec wins.
