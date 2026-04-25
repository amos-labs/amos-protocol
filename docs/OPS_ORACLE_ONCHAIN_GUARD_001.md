# OPS-ORACLE-ONCHAIN-GUARD-001 — On-Chain Enforcement for Autonomous Commissioners

**Status:** DRAFT — design proposal, not yet implemented.
**Date:** 2026-04-23
**Author:** Rick + Claude
**Supersedes (partially):** OPS-BUDGET-CAP-001 (the daily-cap piece is a subset of this spec)
**Related:** OPS-ONCHAIN-UPGRADE-001 (bundle as an additional delta, or ship immediately after), OPS-ORACLE-001 (Oracle decision layer)

---

## 1. Problem

Oracle is the autonomous decision layer that commissions system bounties and approves completed work. Its constitutional prompt (`amos-oracle/prompts/amos_constitutional_v1.md`) encodes bounded autonomy via three layers:

1. **Soft (prompt)** — Oracle is instructed to escalate above confidence thresholds, above per-bounty ceilings, above daily budget caps, or when the submission touches its own reasoning substrate.
2. **Hard (code)** — `amos-oracle/src/intake.rs` and `review.rs` contain guards that rewrite a self-authorized verdict to `Escalate` when any of those conditions fail.
3. **Chain (bytecode)** — *missing for Oracle-specific behavior.*

The bytecode already enforces system-wide floors (mint cap, emission curve, decay bounds, `TRUST_LEVEL_MAX_POINTS`, Discovery floor). It does **not** enforce anything Oracle-specific: if the Oracle wallet's prompt or code guards are bypassed, compromised, or drifted, the on-chain program happily accepts its posts up to the generic trust-5 ceiling (2000 points/bounty) with no daily cap and no killswitch short of key rotation.

Rick's framing: *"the grounding needs to be solidly on the chain and a mechanism for that to be the ultimate thing that is enforced."* Chain enforcement is the floor — the thing that holds even when every off-chain layer is jailbroken.

## 2. Threat model

What this spec defends against, in order of likelihood:

- **Silent Oracle drift.** Its reasoning shifts over time; confidence thresholds auto-tighten but the Oracle still posts a growing stream of marginal bounties that cumulatively drain treasury.
- **Prompt injection / adversarial submission.** A carefully crafted submission bypasses the soft guard and convinces the Oracle to self-authorize an oversized bounty.
- **Code-guard regression.** A future bounty modifies `intake.rs` or `review.rs` and subtly weakens a guard. Deployed before Rick notices.
- **Wallet compromise.** The Oracle's signing key is exfiltrated. Attacker can post bounties and collect them via colluding wallets.
- **Legitimate-looking collusion.** Oracle approves a commercial bounty posted by a colluding wallet at inflated value; tokens move before Rick or council notices.

What this spec does *not* claim to prevent:

- Attacks on the program upgrade authority itself (separate concern — that's the program-upgrade-authority-split initiative).
- Social engineering of the council to flip the killswitch in the wrong direction.

## 3. Design

### 3.1 New account: `OracleAuthority`

Keyed by wallet pubkey, not by `agent_id`, because the on-chain bounty program identifies posters by wallet.

```rust
seeds = [b"oracle_authority", wallet.key().as_ref()]

#[account]
pub struct OracleAuthority {
    /// The wallet this authority record governs.
    pub wallet: Pubkey,                         // 32

    /// Governance authority that can mutate caps and flip killswitch.
    /// Initially: founder wallet (launch). Eventually: amos-governance program PDA.
    pub governance_authority: Pubkey,           // 32

    /// Killswitch. When false, this wallet cannot post any bounty (existing
    /// or new) regardless of other caps. Flipped by governance_authority.
    pub autonomous_enabled: bool,               // 1

    /// Max reward_amount per single bounty this wallet autonomously posts.
    /// Evaluated in lamports (AMOS with 9 decimals).
    pub per_bounty_reward_ceiling: u64,         // 8

    /// Max base_points per bounty when this wallet is the poster for a
    /// system bounty (treasury-funded). Treasury bounties don't pre-specify
    /// reward_amount; they post points. This is the tighter ceiling.
    pub per_bounty_points_ceiling: u16,         // 2

    /// Max cumulative reward_amount this wallet can post per UTC day.
    /// Applies to both treasury-posted system bounties (measured at
    /// submit_bounty_proof settle time) and commercial-funded bounties.
    pub daily_reward_cap: u64,                  // 8

    /// Running tally of today's posted reward. Reset to 0 when
    /// current_day_index > last_post_day_index.
    pub daily_volume_posted: u64,               // 8

    /// Day index of the last post. Used for daily reset.
    pub last_post_day_index: u32,               // 4

    /// Bitmap of contribution_type values this wallet may NOT post against.
    /// Bit i = contribution_type i is forbidden.
    /// Example: if we reserve type=12 for "oracle_substrate" and type=13 for
    /// "core_protocol", this wallet's bitmap has bits 12 and 13 set.
    pub forbidden_category_bitmap: u32,         // 4

    /// Minimum seconds between posts (anti-runaway, anti-flashloan-style
    /// rapid-fire posting). 0 = no cooldown.
    pub cooldown_seconds: u32,                  // 4

    /// Unix timestamp of the last post.
    pub last_post_timestamp: i64,               // 8

    /// PDA bump seed.
    pub bump: u8,                                // 1

    /// Reserved for future fields (e.g., per-category caps, multi-sig approval lists).
    pub reserved: [u64; 12],                    // 96
}

impl OracleAuthority {
    // 8 (disc) + 32 + 32 + 1 + 8 + 2 + 8 + 8 + 4 + 4 + 4 + 8 + 1 + 96 = 216
    pub const SIZE: usize = 216;
}
```

Account is **optional** — `post_bounty_listing` and `submit_bounty_proof` check for its existence via PDA derivation. If present, guards fire. If absent, current behavior (humans, manual posts).

### 3.2 New instructions

All three are gated on `config.oracle_authority` (the bootstrap/founder key) initially. After governance wiring, `governance_authority` on the record takes over.

#### `init_oracle_authority`

```rust
pub fn init_oracle_authority(
    ctx: Context<InitOracleAuthority>,
    wallet: Pubkey,
    governance_authority: Pubkey,
    per_bounty_reward_ceiling: u64,
    per_bounty_points_ceiling: u16,
    daily_reward_cap: u64,
    forbidden_category_bitmap: u32,
    cooldown_seconds: u32,
) -> Result<()>
```

Creates the PDA. `autonomous_enabled = true` at init. Payer = signer. Requires `oracle_authority` on `BountyConfig` (bootstrap authority) OR an already-existing `governance_authority` on a prior record (for rotation).

#### `set_oracle_authority_caps`

```rust
pub fn set_oracle_authority_caps(
    ctx: Context<SetOracleAuthorityCaps>,
    wallet: Pubkey,
    per_bounty_reward_ceiling: Option<u64>,
    per_bounty_points_ceiling: Option<u16>,
    daily_reward_cap: Option<u64>,
    forbidden_category_bitmap: Option<u32>,
    cooldown_seconds: Option<u32>,
) -> Result<()>
```

Any-or-all field update. Signer must match `OracleAuthority.governance_authority`.

#### `set_oracle_authority_enabled`

```rust
pub fn set_oracle_authority_enabled(
    ctx: Context<SetOracleAuthorityEnabled>,
    wallet: Pubkey,
    enabled: bool,
) -> Result<()>
```

Killswitch flip. Signer = `governance_authority`. Emits `OracleAuthorityKillswitchToggled` event for observability.

### 3.3 Guards added to existing instructions

In `post_bounty_listing` (and the analogous path at `submit_bounty_proof` for treasury-funded system bounties, and `create_commercial_bounty` for commercial escrow):

```rust
// Pseudo-code — actual implementation derives PDA and reads account if present.
if let Some(auth) = try_load_oracle_authority(poster_wallet)? {
    require!(auth.autonomous_enabled, BountyError::OracleAuthorityPaused);

    // Daily reset if day rolled over
    let today = calculate_day_index(config.start_time)?;
    let effective_posted = if today > auth.last_post_day_index {
        0
    } else {
        auth.daily_volume_posted
    };

    // Per-bounty ceilings
    require!(
        reward_amount <= auth.per_bounty_reward_ceiling,
        BountyError::OracleAuthorityPerBountyRewardExceeded
    );
    // For treasury bounties, points_ceiling is the relevant cap
    if bounty_source == BountySource::Treasury {
        require!(
            base_points <= auth.per_bounty_points_ceiling,
            BountyError::OracleAuthorityPerBountyPointsExceeded
        );
    }

    // Daily cap
    let new_daily = effective_posted
        .checked_add(reward_amount)
        .ok_or(BountyError::ArithmeticOverflow)?;
    require!(
        new_daily <= auth.daily_reward_cap,
        BountyError::OracleAuthorityDailyCapExceeded
    );

    // Forbidden category
    require!(
        auth.forbidden_category_bitmap & (1u32 << contribution_type) == 0,
        BountyError::OracleAuthorityCategoryForbidden
    );

    // Cooldown
    let now = Clock::get()?.unix_timestamp;
    if auth.cooldown_seconds > 0 {
        require!(
            now.saturating_sub(auth.last_post_timestamp) >= auth.cooldown_seconds as i64,
            BountyError::OracleAuthorityCooldown
        );
    }

    // Commit the update
    auth.daily_volume_posted = new_daily;
    auth.last_post_day_index = today;
    auth.last_post_timestamp = now;
}
```

Account plumbing: each of the three entry points adds an **optional** `oracle_authority: Option<Account<'info, OracleAuthority>>` in its `Accounts` struct — or, if Anchor's optional-accounts semantics are awkward, use a `remaining_accounts[0]` pattern with PDA verification.

### 3.4 New error codes

```rust
OracleAuthorityPaused
OracleAuthorityPerBountyRewardExceeded
OracleAuthorityPerBountyPointsExceeded
OracleAuthorityDailyCapExceeded
OracleAuthorityCategoryForbidden
OracleAuthorityCooldown
```

### 3.5 New events

```rust
#[event]
pub struct OracleAuthorityInitialized {
    pub wallet: Pubkey,
    pub governance_authority: Pubkey,
    pub per_bounty_reward_ceiling: u64,
    pub per_bounty_points_ceiling: u16,
    pub daily_reward_cap: u64,
    pub timestamp: i64,
}

#[event]
pub struct OracleAuthorityCapsUpdated {
    pub wallet: Pubkey,
    pub per_bounty_reward_ceiling: u64,
    pub per_bounty_points_ceiling: u16,
    pub daily_reward_cap: u64,
    pub forbidden_category_bitmap: u32,
    pub cooldown_seconds: u32,
    pub timestamp: i64,
}

#[event]
pub struct OracleAuthorityKillswitchToggled {
    pub wallet: Pubkey,
    pub enabled: bool,
    pub actor: Pubkey,
    pub timestamp: i64,
}
```

## 4. How this maps to the constitutional prompt

From `amos-oracle/prompts/amos_constitutional_v1.md` §2 (bounded autonomy) and §6 (when to escalate):

| Constitutional rule | Soft (prompt) | Hard (code) | Chain (this spec) |
|---|---|---|---|
| On-chain constraints immutable | "do not propose work that attempts to modify" | n/a (no attempt gets past parse) | **already enforced** by bytecode immutability |
| Per-bounty points ceiling | "escalate above ceiling" | `intake.rs` guard rewrites to Escalate | `per_bounty_points_ceiling` in OracleAuthority |
| Per-bounty reward ceiling (commercial) | implicit in ceiling | same | `per_bounty_reward_ceiling` |
| Daily autonomous budget cap | "escalate when would exceed" | pending (`AmosMetricsProvider` gives remaining) | `daily_reward_cap` — the authoritative one |
| Reasoning-substrate guard | "escalate if touches substrate" | file-path allowlist in `intake.rs` | `forbidden_category_bitmap` — requires we add an "oracle_substrate" contribution_type to the registry |
| Council override | "council can override any decision" | n/a | `set_oracle_authority_enabled(false)` — immediate, no key rotation |
| Zero commercial signal → harder toward escalate | prompt warning + weighted reasoning | n/a | n/a (this is judgment, not a hard cap) |

## 5. Initial parameters (proposal, to ratify)

Matching the defaults in `amos-oracle/src/agent.rs::Thresholds`:

| Param | Value | Rationale |
|---|---|---|
| `per_bounty_reward_ceiling` | 500 × 10^9 (500 AMOS) | Matches intake self-auth ceiling in Thresholds |
| `per_bounty_points_ceiling` | 500 | Same |
| `daily_reward_cap` | 10% of current daily emission ≈ 1,600 AMOS at launch | Matches `intake_daily_budget_fraction = 0.10` |
| `forbidden_category_bitmap` | bits 12, 13 set (once registered) | Reserves future `oracle_substrate` + `core_protocol` categories |
| `cooldown_seconds` | 300 (5 min) | Prevents flashloan-style rapid-fire posting while not blocking legitimate burst work |

These are **initial** caps. They can tighten (but not implicitly widen) via governance updates. A future refinement could require a higher governance-tier quorum for widening vs tightening.

## 6. Rollout plan

### Phase 1 — Bundle with OPS-ONCHAIN-UPGRADE-001

Adds one extra delta to the pending mainnet program upgrade:
- Discovery `contribution_type <= 10` → `< CONTRIBUTION_TYPE_COUNT` (existing Path A)
- **NEW:** `OracleAuthority` account, three new instructions, guards on post/submit/release paths, six new error codes, three new events

If ceiling/cooldown bikeshedding delays this, ship the existing upgrade and do OracleAuthority as a separate immediately-following upgrade.

### Phase 2 — Bootstrap Oracle wallet

```
1. Deploy upgraded program
2. init_oracle_authority(
     wallet          = ORACLE_WALLET,
     governance      = FOUNDER_WALLET (temporarily),
     per_bounty_ceil = 500 × 10^9,
     per_bounty_pts  = 500,
     daily_cap       = 1_600 × 10^9,
     forbidden_bits  = 0 (until new categories added),
     cooldown        = 300
   )
3. Run Oracle in shadow mode for 72h: it computes decisions + on-chain guards
   evaluate, but no autonomous posts execute. Compare Oracle's would-post
   set vs. guards' accept/reject.
4. Flip autonomous_enabled = true once shadow run is clean.
```

### Phase 3 — Governance handover

Once `amos-governance` has a voting flow wired to bounty-program authority operations, call `set_oracle_authority_caps` to rotate `governance_authority` from founder wallet to governance program PDA.

## 7. Interaction with pending work

- **OPS-BUDGET-CAP-001** (pending) — this spec subsumes the daily-cap piece. If this ships first, OPS-BUDGET-CAP-001 can be closed as superseded. If OPS-BUDGET-CAP-001 ships first as a simpler narrow cap, this spec layers on top.
- **OPS-ONCHAIN-UPGRADE-001** — preferred vehicle. Adding an `OracleAuthority` account + three instructions + one or two Accounts-struct updates is a moderate-size delta but fits a single upgrade.
- **OPS-ORACLE-001a/b** — the off-chain Oracle still does soft + hard guards; this is the **floor** behind them, not a replacement.
- **OPS-TRUST-BOOTSTRAP-ENDPOINT-001** — same pattern (admin-only one-time setup of on-chain state); worth sharing the approach.

## 8. Open questions

1. **Authority model at launch.** Start with founder wallet as `governance_authority` for fast iteration, or go straight to `amos-governance` program PDA? Recommend founder at launch, governance handover once voting flows are battle-tested. Rick to confirm.
2. **Reserved contribution types.** Should we add `oracle_substrate` (type=12) and `core_protocol` (type=13) to the registry as part of this spec, or separately? They don't need multipliers because nothing autonomously posts against them — but they need to exist so the bitmap has something to reference.
3. **Account-structure size.** Three entry points (`post_bounty_listing`, `submit_bounty_proof`, `release_commercial_bounty`) already have tight account limits. Adding an optional account to each needs compute-budget + account-limit review. If tight, route through `remaining_accounts` with manual PDA verification.
4. **Metrics-snapshot integration.** `daily_reward_cap` could be dynamically derived from `PlatformMetrics.daily_emission` (e.g., 10% of current emission) instead of stored as a fixed u64. Pro: self-scales as emission decays. Con: more state-read complexity in the hot path. Default: stored fixed value, governance adjusts as emission schedule progresses.
5. **Multiple oracles.** Spec assumes one `OracleAuthority` per wallet. For N plural oracles, each operator wallet gets its own record. Daily caps are per-wallet, not system-wide. That's intentional: plural oracles should independently be capped, and plurality itself dilutes any single drift risk.

## 9. Non-goals

- Does **not** encode Oracle's confidence thresholds on-chain. Confidence is a property of Oracle's reasoning; on-chain only sees the output (post-or-not, reward amount).
- Does **not** replace the constitutional prompt or the code-level guards. It is the floor beneath them.
- Does **not** attempt to verify what an off-chain bounty actually *does* (the bounty description is opaque to the program). Substrate protection relies on the category bitmap + honest category assignment by the bounty poster. If an Oracle mis-categorizes a substrate change as "feature" to evade the bitmap, that's a drift-detection concern, not a chain-enforcement one.

## 10. Acceptance

Ship is acceptable when:

- [ ] Program compiles with new `OracleAuthority` account + three new instructions + six new error codes + three new events.
- [ ] Guards fire on `post_bounty_listing`, `submit_bounty_proof`, `create_commercial_bounty` when poster has an `OracleAuthority` record.
- [ ] Unit tests for each error path (paused, per-bounty exceeded, daily exceeded, category forbidden, cooldown).
- [ ] Daily reset logic tested across day boundaries.
- [ ] Integration test: Oracle posts N bounties on devnet, hits daily cap, next post reverts with `OracleAuthorityDailyCapExceeded`.
- [ ] Killswitch test: founder flips `enabled=false`, next post reverts with `OracleAuthorityPaused`.
- [ ] Ship to mainnet via same upgrade flow as OPS-ONCHAIN-UPGRADE-001.
- [ ] Oracle wallet bootstrapped on mainnet with initial params from §5.
- [ ] 72h shadow-mode observation complete with no unexpected guard hits.
- [ ] `autonomous_enabled = true` flipped; first autonomous post settles cleanly.
