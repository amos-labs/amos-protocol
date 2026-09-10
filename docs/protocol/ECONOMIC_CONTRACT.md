# AMOS Economic Contract v1

**Status: reconciled research source, 2026-09-10. Not a deployment certificate.**
This document resolves conflicting descriptions across Protocol, the frozen
`amos-core` snapshot, the older `amos-automate` repository, Platform calculators,
the public website and Marketplace. It governs this source revision. Installed
program binaries, funded accounts, endpoint availability and production use must
be verified separately. Program IDs are unchanged by this reconciliation.

## 1. Product, protocol and proposed ecosystem

| Layer | Current scope | Claims this does not establish |
|---|---|---|
| Commercial AMOS | Managed business software, hosted intelligence, subscriptions and learning experiments | That subscription payments settle through AMOS tokens |
| Protocol | Relay, Oracle, proof commitments and experimental Solana settlement programs | A permissionless, audited, fully operating token economy |
| Marketplace | Research interface for bounty and agent discovery | A deployed exchange for models, compute or every kind of learning asset |
| Package economy | Proposed compensation for reusable expertise and tools | Implemented package royalties or a live package-revenue entitlement |
| Learning architecture | Separate evidence and evaluation track | That token mechanics demonstrate RSI or that a source merge improves model quality |

Managed pricing comes from the website's shared plan definitions and actual
billing configuration. The retired Platform `/token` calculators return HTTP 410.
They are not a second economic API. Equity fundraising and protocol token
allocation are separate; historical “no investors” text is not a restriction on
the company's financing.

## 2. Units, supply and value

- One AMOS is **1,000,000,000 raw SPL units**. The mint has nine decimals.
  “Lamports” in old bounty comments refers to these raw token units, not SOL.
- Intended initial allocation: **100 million AMOS**, comprising a **95 million
  work treasury** and **5 million reserve**. Source constants do not prove actual
  issuance, mint-authority revocation, current balances or reserve governance.
- No token price, exchange listing, FDV or holder return is specified here.
- Relay's historical `reward_tokens` field stores **contribution points** for
  treasury work. Points are not AMOS and are not a fixed payout commitment.
- Quality is a **0–100 score**, not a 0–1 probability. Passing payout bounds are
  30–100; a numerical score is still the Oracle's judgment of the evidence.
- Minimum new commercial escrow and V2 stake are each **100 AMOS**. Old raw
  constants corresponding to fractions of a token are corrected.

## 3. Two funding paths; one commercial fee

**System work** receives a bounded transfer from the work treasury. There is no
3% commercial fee on this path. Its payout is split 95% worker / 5% reviewer.
Relay no longer creates fictitious commercial fee-ledger records for system
approvals or marks those records as settled. Historical fee rows require separate
reconciliation against actual chain transfers.

**Commercial work** is funded by the poster's escrow. It pays 3% of gross as the
protocol fee. Of that fee, 50% goes to the holder reward vault, 40% is burned and
10% goes to Labs. The remaining 97% splits 95% worker / 5% reviewer.

| Destination | 1,000 AMOS commercial gross | Percentage of gross |
|---|---:|---:|
| Worker | 921.5 | 92.15% |
| Reviewer | 48.5 | 4.85% |
| Holder rewards | 15 | 1.5% |
| Burn | 12 | 1.2% |
| Labs | 3 | **0.3%** |
| Total | 1,000 | 100% |

Compute the 3% fee in raw units, rounded down. Round its holder and burn shares
down, assigning the fee remainder to Labs. Round the reviewer's 5% of net down;
the worker receives the net remainder. No fee is charged twice. In particular,
an already-split commercial holder share goes straight to the canonical holder
reward vault; it must not be passed through the fresh-fee splitter again.

Package attribution at 0.5% of commercial gross (historical proposed range
0.1–1%) is **not implemented**. It would reduce holders' share and therefore
requires an explicit new contract and authorization. The corrected illustrative
proposal is in [Package Economy Integration](../packages/economy-integration.md).

## 4. Emission and system payout kernel

`protocol-math/src/lib.rs` is the shared, dependency-free integer implementation
used by Relay and the bounty program. Floating point examples are explanatory;
integer results govern settlement. With `d` as days since program start:

```
E(d) = 100 + 15,900 / (1 + exp(0.005 × (d − 1,460))) AMOS/day
G(d) = 300 + 1,700 / (1 + exp(0.01 × (d − 540))) basis points
```

Emission starts near 15,989.27 AMOS/day, reaches 8,050 on day 1,460, and tends to
100. Growth capacity starts near 19.92%, reaches 11.5% on day 540, and tends to
3%. The old exponential lookup clamped at `exp(6)` and incorrectly left a
139.3147 AMOS/day tail; this version removes that plateau. Intermediate products
use bounded `u128` arithmetic; extreme days cannot wrap through signed integers.

For the current daily pool, let `p` be adjusted contribution points, `P` prior
points, `D` raw units already paid and `t` seconds since the day's start:

```
released = floor(E_raw × clamp(t, 0, 86400) / 86400)
available = max(0, released − D)
shared_cap = floor(p × available / (P + 10,000 + p))
payout = min(shared_cap, category_remaining, explicitly_authorized_max)
```

Growth work cannot exceed the day's growth allocation. Technical work can use
remaining treasury capacity, including unused growth capacity. The shared cap
and time release are independently enforced on-chain. Zero maximum authorizes
zero payout; there is no “zero means unlimited” branch and no one-token or
one-raw-unit minimum that bypasses the cap. A zero-result submission fails before
transfers and state updates.

Relay estimates may use base points before contribution multipliers and therefore
need not equal the eventual award. A quote is an upper bound at a particular
state, not guaranteed proceeds. Settlement remains **sequential and sensitive to
arrival order**; this is not an order-independent end-of-day pro-rata auction.
Quality determines acceptance; it is not currently an extra payout multiplier.

Unknown RPC state defers settlement. A successful config read plus an explicit
RPC response proving the daily account is absent permits a zero-spent pool
projection using this kernel; the transaction prepares the real account and the
program recomputes its bound. RPC failures never masquerade as absent accounts.
Account owners, discriminators, stored day and timestamp range are validated.
An existing daily pool retains its stored emission after a source upgrade; new
days use the new curve. Plan activation at a day boundary and verify this detail.

The 100 AMOS floor is a scheduling parameter, **not perpetual funding**. A finite
treasury can be exhausted without recycling, replenishment or policy change.

## 5. Identity, review and trust

See [Identity and credential migration](IDENTITY.md). Public agent IDs are not
bearer secrets. Wallet ownership requires a signed expiring, single-use Relay
challenge. Issued credentials are high-entropy secrets stored as hashes. Service
roles, review/merge operations and wallet bindings require explicit provisioning;
they are not acquired by registering a public agent or claiming a wallet string.

Verification and approval provenance are bound to the observed submission version,
so a concurrent resubmission cannot inherit approval of earlier work.
Approval provenance and the currently proved worker wallet are checked before
settlement and on retries. Reviewer destinations must match the declared
reviewer, who differs from the worker. Old unauthenticated approvals are not
silently replayed as authorized transfers. The Oracle remains trusted to assess
semantic work quality and assign legitimate work/points. This is not Sybil-proof
or free of privileged operators merely because final transfers are on-chain.

| Agent level | Maximum base points | System completions per day |
|---|---:|---:|
| 1 | 100 | 3 |
| 2 | 200 | 5 |
| 3 | 500 | 10 |
| 4 | 1,000 | 15 |
| 5 | 2,000 | 25 |

The daily limits reconcile the former agent context with runtime enforcement.
They apply to system agent submissions, not a global per-human work quota.
Non-agent submissions still require Oracle authorization. Existing on-chain
agent IDs are worker-wallet bytes; their stored operator field may identify the
Oracle registrar. Do not mistake that historical registrar field for worker
wallet ownership. Commercial release has a separate escrow contract.

Receipt versions remain distinct. Protocol v1 receipts and the Platform's richer
versioned evidence/lifecycle contracts are not interchangeable. A hash commits
to evidence bytes; it does not prove successful work, causal improvement or
training consent. Each boundary must validate its own version and permitted use.

## 6. Custody, rewards and commercial escrow

V2 treasury state uses distinct PDAs and discriminators. Stake principal and
holder rewards occupy different canonical mint-bound vaults controlled by the
pool PDA. Each position accrues against a cumulative reward index with a stored
checkpoint and fractional credit. A successful claim clears its pending amount;
it cannot repeatedly claim a fraction of the remaining global balance.

Stake changes and claims sync newly received rewards before changing membership.
The 30-day minimum is a **claim delay**, not a principal withdrawal lock. Increasing
stake restarts that position's claim delay. Deposits received with no stakers are
recorded as unallocated; index rounding dust stays in the vault. There is no
privileged sweep or future-first-staker windfall in this version.

Commercial escrow metadata records the original poster, mint, bounty ID, deadline,
amount and terminal status. Release requires Oracle authorization and a time
strictly before the deadline; refund requires the original poster's signature and
a time at or after the deadline. Both operate once from open state. Old escrow
accounts without authenticated metadata cannot be assigned a guessed poster.

For account lists, rounding, donation handling and legacy recovery limits see
[Treasury and Escrow V2 Migration](../../amos-solana/programs/amos-treasury/MIGRATION_V2.md).
**Do not upgrade over outstanding legacy funds without inventory and a reviewed
recovery path.** Disabling unsafe handlers can immobilize those funds; a source
patch alone does not solve the provenance needed for recovery.

## 7. Decay: the implemented mechanism and the remaining design

The current standard SPL wallet operation requires the **holder's signature**.
It can burn and transfer only funds in that holder's validated token account,
bounded by actual balance and recorded earned balance. It cannot compulsorily
debit arbitrary freely transferable AMOS balances.

- Default nominal annual rate: 5%; accepted configured bounds: 2–25%.
- Accrual starts after 90 days without verified earnings, using complete 24-hour
  periods after `max(last_decay, last_activity + 90 days)`.
- New earnings reset both activity and accrual timestamps. Grace days are not
  charged retroactively. Repeating a call cannot charge the same interval again.
- Nominal interval accrual rounds down in raw units. There is no upward minimum.
  Repeated compounding intervals and a single long interval can differ; this is
  not claimed as a continuous compounding specification.
- Cumulative decay preserves 10% of recorded original allocation and never
  exceeds available balance. Of the decay amount, 10% is burned and the remainder
  returns to treasury, with integer rounding on the burn share.

The economic suggestion `rate = clamp(10% − 5% × profit_ratio, 2%, 25%)` is a
policy model, not an independently verified revenue oracle. Under ordinary
nonnegative revenue and positive cost, `(revenue − cost) / cost ≥ −1`, so that
formula's largest ordinary result is 15%, not 25%. Full bounds also constrain
privileged configuration. Negative revenue and missing/zero cost require explicit
definitions rather than an invented profitability reading.

**Not implemented:** 365-day protection per earned grant, tenure-dependent floors,
vault-based decay discounts, and compulsory system-wide decay. Those designs need
per-grant records, actual custody/enforcement and a separate migration. Their
presence in frozen core formulas or historical papers is not chain enforcement.

## 8. Governance and observation limits

V2 voting requires real token custody in a canonical vote vault. Liquid balance
alone is not a vote deposit. Legacy unverifiable tallies cannot be relabeled as
V2 backed votes; proposals must follow the explicit migration. Terminal refunds
do not rewrite historical vote totals. See [Governance Migration](GOVERNANCE_MIGRATION.md).
Custody prevents reuse of those same tokens while locked; it does not itself
prevent concentration, collusion, lending or multiple human-controlled wallets.

The Oracle metrics API reports unavailable economic measurements as `null`.
Contribution points are never counted as both commercial AMOS volume and system
emission. An unavailable observation is different from a measured zero. Relay and
Oracle must be upgraded together for this nullable-field contract.

## 9. Change and activation discipline

Economic changes require an explicit version, source changes, adversarial tests,
review and an activation/migration plan. This revision requires root workspace
tests and the on-chain host tests/SBF build in CI. These checks do not constitute
an external audit or an executed local-validator migration rehearsal.

Before any live activation, inventory legacy accounts, resolve recovery, rehearse
the generated instruction/IDL changes on a validator, verify compute budgets,
provision identities, update clients and identify the exact approved binaries.
These are deployment prerequisites, not claims that this source task performed
live transfers or production upgrades. The [reconciliation ledger](RECONCILIATION_2026-09-10.md)
records verification and remaining work.
