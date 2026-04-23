# AMOS Test Harness — Plan & Handoff

**Status:** L1 + L2 invariants shipped. Phases 1a → 5 planned.
**Goal:** make agent-direct-to-prod actually safe. Every regression class we've seen must be caught before customer impact.

---

## Why this matters

AMOS is an enterprise-grade autonomous app. Agents write code, push to prod, and the OS itself operates unattended. For that to be credible, the release pipeline has to be the review — not a human.

The regressions that have actually hit us (the signal):
- Wrong Bedrock model ID suffix (`opus-4-7-v1` → 400 in prod)
- Marketplace auto-subscribe gap (403 on first use of a new model)
- Bedrock tool-spec envelope drift (400 on chat, twice)
- Empty site containers (tool description confused the agent; 404 at the customer URL)
- Pricing drift (cache/batch tiers missing from catalog)

Every item above is now (or is planned to be) caught automatically before deploy.

---

## What's shipped (as of 2026-04-23, release-blocking on `main`)

All three gate `build-images` via `needs: [...]`.

### 1. `release-gate` — Catalog + Schema Invariants
Pure Rust unit tests. No infra.
- **Model catalog** (`amos-harness/src/routes/settings.rs::tests`)
  - Every ID matches Bedrock grammar (rejected the `-v1` regression)
  - Unique IDs + display names, known tier, DEFAULT_MODEL in catalog
  - Pricing sanity: output > input > cache_read, cache_write_5m > input, 1h > 5m
  - Anthropic ratios: cache_5m ≈ 1.25×, 1h ≈ 2.0×, read ≈ 0.10×, batch ≈ 0.50×
- **Tool schemas** (`amos-harness/src/tools/mod.rs::validate_tool_schema` + per-tool `schema_tests` modules)
  - Top-level `type: "object"`, no Bedrock-hostile `$schema`/`$id`/`$ref`
  - Every `required` entry exists in `properties`
  - Every property has `type` or composition keyword
  - Site tools: all 6 pass; `create_landing_page` specifically required to require `html_content`
- **Convention:** any new tool module adds a `schema_tests` mod; the CI step picks it up by name.

### 2. `integration-smoke` — Core App Flows
Real Postgres (`pgvector/pgvector:pg16` service in CI), full migration apply.
- Landing page end-to-end via `CreateLandingPageTool`
- Multi-page site: `create_site` + two `manage_page` + `publish_site`
- Collection + record CRUD with defaults + validation
- Field-type rejection (string in a number field fails)
- File: `amos-harness/tests/smoke_flows.rs`
- **Key lesson:** per-test PgPool. `#[tokio::test]` spawns fresh runtimes; a shared `OnceLock<PgPool>` goes stale after the first test.

### 3. `live-model-probe` — Every Cataloged Model Responds
Actual `bedrock-runtime converse` against every ID in `AVAILABLE_MODELS`. Main-push-only (OIDC trust scoped to `refs/heads/main`).
- Binary: `amos-harness/src/bin/print_model_catalog.rs` is the single source of truth for which IDs to probe. Same list as the customer dropdown.
- IAM: role `claude_github` (`arn:aws:iam::637423327454:role/claude_github`)
  - Trust: GitHub OIDC, `repo:amos-labs/amos-platform-2.0:ref:refs/heads/main`
  - Permissions: `bedrock:{InvokeModel,InvokeModelWithResponseStream,Converse,ConverseStream,GetInferenceProfile,GetFoundationModel}` + `aws-marketplace:{ViewSubscriptions,Subscribe}`
  - **Critical:** foundation-model ARNs must cover `us-east-1`, `us-east-2`, `us-west-2` — US geo inference profiles route across all three.

### CI structure after these three
```
check (advisory)        release-gate      integration-smoke      live-model-probe
(clippy, fmt, audit)         │                   │                      │
                             └─────── build-images (main push only) ────┘
```

---

## Architectural decision: in-tree vs. separate test app

**Short answer:** both. Draw the line at statefulness.

### Stays in-tree (`cargo test`)
Anything that's stateless, runs in seconds, and caches well:
- L1 invariants (catalog, pricing, tool schemas)
- L2 integration (HTTP handlers, engine CRUD, migration apply)
- Phase 1a HTTP stack tests
- Phase 2 Bedrock envelope snapshot tests
- Phase 3 security + auth enforcement tests

Why: fast developer feedback, colocated with the code, runs on every PR without coordination overhead.

### New service — `amos-sentinel` (proposed)
Anything stateful, long-running, or observational:
- **Phase 4 — Golden bounty harness:** runs the full agent → claim → submit → QA → settle lifecycle against devnet fixtures. Nightly. Tracks history, scores model behavior over time.
- **Phase 5 — Canary + auto-rollback:** monitors the 1% traffic canary, watches error-rate signals from platform sync, triggers rollback via the platform API.
- **Model-behavior benchmarks:** when a new Claude ships, fire a fixed prompt set at it, measure tool-selection accuracy, latency, refusal rate. Track drift.
- **Prompt regression suite:** same fixture prompts against current prod model, every N hours. Catches "agent stopped picking the right tool after a model change."

Why a separate service:
- Needs its own DB for test history and benchmarks
- Has special creds (devnet wallet, Bedrock across regions, platform API admin token)
- Runs scheduled (nightly) AND reactive (on release registration)
- Has its own release cadence — test logic shouldn't bottleneck on harness releases
- Deployable separately — the thing that validates releases shouldn't share a binary with the thing it's validating
- **Eats its own dog food:** `amos-sentinel` itself registers as an AMOS agent and claims `validate_release` bounties, so the test system IS an autonomous agent earning tokens. That's aligned with the self-bootstrapping thesis.

This also matches Rick's framing: "this is basically an enterprise app ran autonomously" — in every enterprise I've worked at, the QA/validation service is its own deployed service, not a collection of CI scripts.

---

## Phase plan

### Phase 1a — HTTP stack integration (stays in-tree)
**Scope:** real Axum router + real Postgres + real Redis; reqwest calls to handlers; assertions on DB state. No Bedrock, no agent sidecar, no LLM.
**Covers:** routing, middleware, auth, JSON serde, tool-registry dispatch, public site rendering.
**Runs:** every PR + push.
**Budget:** ~15-20s CI.
**CI job:** `http-integration`
**Work items:**
- Add Redis service to the CI job (`redis:7-alpine` with health check)
- Build `AppState::for_tests(pool)` helper (feature-gated under `test-utils`). Stubs optional fields as `None`, uses minimal real instances for required ones (ToolRegistry, CanvasEngine, etc.)
- Alternative: call `create_server()` with minimum env vars and accept the 32 spawned background tasks (simpler but heavier)
- JWT test helper — issue a valid token using `config.auth.jwt_secret`
- Test file: `amos-harness/tests/http_integration.rs`
- Covered routes:
  - `POST /api/v1/sites` → site row in DB
  - `POST /api/v1/sites/{slug}/pages` → page row in DB
  - `POST /api/v1/sites/{slug}/publish` → `is_published = true`
  - `GET /s/{slug}/` → 200 + rendered HTML
  - `POST /api/v1/collections` → collection row in DB
  - `POST /api/v1/collections/{slug}/records` → record row
  - `GET /api/v1/tools` → includes `create_landing_page` in the listing
  - `GET /health` → 200

### Phase 1b — Full E2E with Haiku (stays in-tree, main-only)
**Scope:** spawn both `amos-harness` and `amos-agent` binaries as subprocesses; real Bedrock Haiku call; assert DB state changed.
**Covers:** full agent loop, tool selection, Bedrock envelope, SSE streaming.
**Runs:** push to main only (not PRs — keeps PR feedback fast).
**Budget:** ~30-60s CI.
**CI job:** `agent-loop-e2e`
**Work items:**
- `amos-harness/tests/agent_loop_e2e.rs`
- `tokio::process::Command` to spawn both binaries with test env
- Readiness polling against `/ready` on both
- Deterministic prompt: `"Call the create_landing_page tool with exactly: name='E2E', slug='e2e-<uuid>', html_content='<h1>E2E</h1>'."`
- Use `us.anthropic.claude-haiku-4-5-20251001-v1:0` (cheapest, fastest)
- Poll DB for page existence, timeout 60s
- Reuse `claude_github` OIDC role (already has Bedrock + Marketplace)
- Key risk: LLM non-determinism — mitigate with explicit tool-name in prompt and assertions on DB state (not model output text)

### Phase 2 — Bedrock envelope contract (stays in-tree)
**Scope:** snapshot the exact JSON body our Bedrock request builder produces.
**Covers:** the two tool-spec wrapping regressions we hit before; the `thinking.type: "adaptive"` requirement for Opus 4.7; `temperature`/`top_p`/`top_k` no longer supported.
**Budget:** fast, no infra.
**Work items:**
- `amos-agent/tests/bedrock_envelope.rs` (likely here rather than harness)
- Golden-JSON fixture files in `amos-agent/tests/fixtures/`
- Build a request, serialize to JSON, compare to fixture
- When a regression is intentional, regenerate the fixture consciously (human gate)

### Phase 3 — Security + migration gates (mostly in-tree)
**Scope:** auth enforcement, trust-level gating, destructive-action confirmation, fresh-DB migration apply.
**Work items:**
- Every protected route rejects unauth'd requests with 401
- Every tool category's trust-level gate tested: a trust-1 agent can't invoke a trust-4 tool
- Bash tool: `rm -rf` etc. returns a confirmation token, not an execution result
- Migration test: apply all migrations on an empty DB in < 30s with no errors (covered implicitly now, but could be extracted + timed)
- Optional: migration reversibility where it matters

### Phase 4 — Golden bounty harness (new service: `amos-sentinel`)
**Scope:** run the full bounty lifecycle against devnet with fixture bounties whose outcomes are known.
**Target runtime:** nightly + on-release.
**Budget:** minutes per run, minutes of compute cost.
**Work items:**
- Stand up `amos-sentinel` crate in the workspace
- DB for test-run history, fixture registry, benchmark metrics
- Fixture format: JSON files describing (bounty definition, known-good submission, expected outcome)
- Runner: post bounty → claim as test agent → submit known-good → observe QA → observe settlement → assert reputation delta
- 10 seed fixtures covering the core contribution types
- Self-register as an AMOS agent and claim `validate_release` bounties — this is literally a bounty the sentinel earns tokens for
- Deploy target: its own ECS task (separate from harness/agent)

### Phase 5 — Canary + auto-rollback (in `amos-sentinel`)
**Scope:** 1% traffic split to a canary harness running the new image; monitor error rate; auto-rollback if SLO violated.
**Prerequisite:** platform-side traffic split capability (probably a listener rule change in the ALB).
**Work items:**
- Platform: canary harness type, traffic-split configuration
- Sentinel: error-rate collector (subscribes to platform sync events)
- Sentinel: rollback trigger when error rate > baseline + threshold over N minutes
- Rollback mechanism: POST to platform API to pin harnesses back to previous image tag
- Alerting: slack/pagerduty when rollback triggers

---

## Key design decisions made

1. **Static source of truth for models:** `AVAILABLE_MODELS` in `amos-harness/src/routes/settings.rs` is THE list. Customer dropdown and live probe both read via `catalog_model_ids()`. Renaming a model, adding a new one, or removing one is a single-file change.

2. **Anthropic pricing pattern is an invariant** (validated against Sonnet 4.6 and Opus 4.7 at the time of writing):
   - cache_write_5m = 1.25× base input
   - cache_write_1h = 2.0× base input
   - cache_read = 0.10× base input
   - batch = 0.50× base input/output
   If Anthropic changes the ratios, we'll fix the test consciously.

3. **US geo inference profiles route across regions.** The IAM role that invokes Bedrock must cover `us-east-1`, `us-east-2`, `us-west-2` at minimum on the foundation-model ARN.

4. **Per-test PgPool.** `#[tokio::test]` spawns a fresh runtime per test; a shared static pool from the first test goes stale. Migrations are idempotent, so reconnecting + re-running `migrate!()` per test is cheap.

5. **Tiered CI**: fast for PRs, deeper for main pushes, nightly for stateful verification.

6. **Test harness as a service (`amos-sentinel`) for stateful verification.** In-tree tests for the fast stuff; separate app for the Tier-2/3 stuff that needs history, special creds, scheduled runs, and canary monitoring.

---

## Critical files + env vars

### Source-of-truth files
- `amos-harness/src/routes/settings.rs` — `AVAILABLE_MODELS`, `DEFAULT_MODEL`, `catalog_model_ids()`
- `amos-harness/src/tools/mod.rs` — `validate_tool_schema()`
- `amos-harness/src/bin/print_model_catalog.rs` — CI → catalog bridge
- `amos-harness/tests/smoke_flows.rs` — integration-smoke job
- `.github/workflows/ci.yml` — release-gate, integration-smoke, live-model-probe, build-images

### Test env vars
- `DATABASE_URL` — Postgres with pgvector
- `AMOS__AUTH__JWT_SECRET` — must match what the test JWT signer uses
- `AMOS__VAULT__MASTER_KEY` — has dev fallback, tests can leave unset
- `SHARED_BEDROCK_ENABLED=true` — to exercise the shared Bedrock code path
- `AGENT_URL` — for Phase 1b subprocess coordination

### IAM
- Role: `claude_github` — CI only, main branch only, Bedrock Claude + Marketplace

---

## Gotchas observed
- Older GHA images don't have `bedrock-runtime converse` in their bundled AWS CLI. Use `ubuntu-24.04` (we do).
- First probe of a new Bedrock model triggers a Marketplace subscribe handshake ("try again in 2 minutes"). After the first successful call the handshake is cached.
- Clippy/audit failures are **advisory** in our CI by design — they don't block image builds. The release-gate / integration-smoke / live-model-probe gates are what actually block.
- `cargo test --lib -p amos-harness <filter>` uses substring match, not exact. The release-gate relies on `routes::settings::tests` and `schema_tests` being unique prefixes.

---

## Session handoff notes

**What a fresh session should do next:**
1. Read this doc.
2. Start Phase 1a: add Redis service to CI, build the `AppState::for_tests` helper, write `tests/http_integration.rs`, iterate to green. Probably 1-2 hours end to end.
3. Then Phase 1b: subprocess E2E with Haiku.
4. Then Phase 2 (envelope contract), Phase 3 (security).
5. Scope Phase 4 (sentinel) as a standalone crate in a dedicated session.

**Commits already on main:**
- `a1e3f6b` — L1 regression tests (catalog, pricing, tool-schema)
- `b1c7fe5` — Release-gate CI job blocking deploys on L1 failures
- `50d7426` — Live Bedrock probe + `print-model-catalog` binary + `claude_github` role wiring
- `a8009f4` — L2 integration smoke (landing page, multi-page, collection CRUD)
- `008f05f` — ModelInfo pricing expansion (batch + cache tiers)
- `5914fc1` — Fix Opus 4.7 Bedrock profile ID (dropped `-v1`)
- `29fb9d6` — Add `create_landing_page` tool (single-call landing page)

**Current `main` is green end-to-end on all three release gates.**
