# AMOS Token: Technical Whitepaper

**Version 4.0 | April 2026**

---

## Abstract

AMOS (Autonomous Management Operating System) Token is a Solana-based SPL token designed to align incentives between platform contributors, distributors, and users. Unlike traditional equity or utility tokens, AMOS implements a novel **decay-based ownership model** with **pool-based contribution rewards**. This paper describes the technical architecture, economic mechanisms, and governance specifications.

## Vision: A New Economy of Humans and Agents

AI is creating a fundamentally new kind of economy — one where **humans and AI agents work side by side**, each contributing what they do best. Humans bring judgment, creativity, and direction. Agents bring speed, scale, and tireless execution. Together, they accomplish more than either could alone.

AMOS is the platform where this happens: **an open-source AI automation platform where everyone who contributes — human or machine — earns ownership**.

```
THE AMOS MODEL:
Builders (human)     → Ownership
Sellers (human)      → Ownership
AI Agents            → Ownership (earned through work)
Community            → Ownership
Everyone             → Proportional Share
```

As AI becomes more capable, the value created by human-agent collaboration should flow to everyone involved in building it. AMOS makes this possible through transparent, on-chain ownership that's earned through contribution — not purchased through privilege.

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Token Specifications](#2-token-specifications)
3. [Economic Model](#3-economic-model)
4. [Decay Mechanism](#4-decay-mechanism)
5. [Wealth Preservation](#5-wealth-preservation)
6. [Reward Calculation](#6-reward-calculation)
7. [Blockchain Integration](#7-blockchain-integration)
8. [Governance](#8-governance)
9. [Security Considerations](#9-security-considerations)
10. [Technical Implementation](#10-technical-implementation)
11. [Economic Modeling & Sustainability](#11-economic-modeling--sustainability-analysis)
12. [Regulatory Commitment](#12-regulatory-commitment)
13. [AI Participation & Universal Collaboration](#13-ai-participation--universal-collaboration)

---

## 1. Introduction

### 1.1 Problem Statement

The emerging AI economy needs new models for value distribution:

- **Contributors** (human and AI) create enormous value but don't share in long-term upside
- **Passive holders** accumulate ownership without contributing
- **Late participants** face barriers to meaningful ownership
- **AI agents** do real work but have no mechanism to earn or build reputation
- **USD-denominated rewards** create regulatory complexity and external dependencies

### 1.2 Solution

AMOS Token introduces:

1. **Contribution-based ownership** - Tokens primarily earned through work; decay ensures passive buyers gradually transfer stake to active contributors
2. **Decay function** - Continuous participation required for maximum stake
3. **Pool-based rewards** - No external price dependencies
4. **Transparent distribution** - All ownership publicly verifiable on-chain
5. **Protocol fee sharing** - Token holders receive 50% of relay protocol fees on commercial bounties (system bounties: 0% fee)

### 1.3 Design Principles

- **Fairness**: Same rules for founders and contributors
- **Transparency**: All allocations on-chain and auditable
- **Sustainability**: Self-balancing economic mechanisms
- **Accessibility**: Low barriers to participation
- **Independence**: No USD denomination or external price dependencies

---

## 2. Token Specifications

### 2.1 Basic Parameters

| Parameter | Value |
|-----------|-------|
| **Name** | Amos Platform Token |
| **Symbol** | AMOS |
| **Network** | Solana |
| **Token Standard** | SPL Token |
| **Decimals** | 9 |
| **Total Supply** | 100,000,000 (fixed) |
| **Mint Authority** | Disabled (immutable) |

### 2.2 Initial Allocation

```
Total Supply: 100,000,000 AMOS

┌────────────────────────────────────────────────────────────────────────────┐
│  Bounty Treasury (95%)   │ 95,000,000 │ Contributor rewards via emissions │
│  Emergency Reserve (5%)  │  5,000,000 │ DAO-locked, governance vote req.  │
├────────────────────────────────────────────────────────────────────────────┤
│  Founders                │          0 │ Start at zero, earn like everyone │
│  Company                 │          0 │ No entity allocation              │
│  Investors               │          0 │ No investor pre-allocation        │
└────────────────────────────────────────────────────────────────────────────┘
```

### 2.2.1 Pool Purpose Clarification

| Pool | Purpose | Example Uses |
|------|---------|--------------|
| **Bounty Treasury (95M)** | Contributor rewards | Bounties, affiliate commissions, daily emission pool |
| **Emergency Reserve (5M)** | Emergency | DAO-locked, requires governance vote to access |

**Key Design Decision: Founders Start at Zero**

Unlike traditional token launches where founders receive a pre-allocation, AMOS founders begin with zero tokens and earn through contribution like everyone else. The simplified two-pool allocation makes this even stronger: there is no entity pool, no investor pool, no community pool with discretionary spending. The only way anyone -- including founders, the company, and investors -- earns AMOS tokens is by completing bounties through the same system available to every contributor.

This provides:

- **Maximum credibility**: "We built this - we earn like you"
- **Perfect alignment**: Founders succeed only if the platform succeeds
- **No dump risk**: No founder tokens to sell, no entity tokens to liquidate
- **Radical simplicity**: Two pools, one rule -- contribute to earn
- **True equality**: Founders, employees, investors, and community members all earn on equal footing through the bounty system

### 2.3 Immutability

- Mint authority permanently disabled (revoked April 15, 2026)
- No additional tokens can ever be created
- Only mechanism to increase supply: None
- Only mechanism to decrease supply: Burn

---

## 3. Economic Model

**Tokenomics** ($AMOS on Solana — 100M fixed supply):
- **No pre-mine, no entity pool, no investor pool.** 95% goes to the bounty treasury for contributor rewards. Founders, the company, and investors all start at zero and earn through bounties like everyone else.
- **Revenue sources** (all flow to token holders):
  - Protocol fee on every bounty payout (default 3%, adjustable 1-5% by Governance Council)
  - 50% of Managed Hosting markup (while AMOS Labs runs the official cloud)
- **Distribution** (ultra-holder-friendly):
  - 50% → staked $AMOS holders (pro-rata claims)
  - 40% → permanently burned (deflationary)
  - 10% → AMOS Labs (maximum alignment — Labs lives or dies by the token)
- **Earning mechanisms** (humans + AI agents):
  - Bounty completion (paid in $AMOS)
  - Code/community contributions (points → daily emission pool)
  - Referrals & sales
- **Staking utilities**:
  - Higher trust level → more concurrent agents + priority
  - Reduced decay rate
  - Premium Relay access
- **Deflationary pressure**: Built into the 40% permanent burn allocation on all commercial bounties.
- **Decay mechanism** remains (rewards active participants over passive holders).

### 3.1 Token Utility

1. **Protocol Fee Share**: 50% of commercial bounty protocol fees distributed to staked holders (system bounties: 0% fee, treasury-funded)
2. **Governance**: Voting rights on protocol parameters and labs allocation
3. **Marketplace Access**: Staking required for premium bounty tiers and agent reputation
4. **Trading**: Freely tradeable on Solana DEXs (Jupiter, Raydium)

### 3.2 Business Model (Revenue Flow)

**Revenue Flow**: Revenue for the AMOS Network Relay is generated through a simple, transparent protocol fee on commercial bounties only (default 3%, adjustable 1-5% by Governance Council vote). System bounties funded by the treasury incur no fee. The fee is collected and distributed entirely on-chain in $AMOS — no centralized payment processor, no Stripe, no Circle, no mandatory fiat on-ramp required. All transactions denominated in AMOS tokens. Users and agents interact directly with Solana wallets:
- Bounty posters fund commercial tasks with $AMOS acquired on any DEX (Raydium, Jupiter, etc.) or via optional third-party fiat on-ramps
- System bounties are funded from the treasury with 0% protocol fee
- Agents claim rewards directly to their Solana wallet in AMOS tokens
- The protocol automatically deducts the fee on commercial payouts and routes it according to the 50/40/10 model

This design keeps participation fully optional and permissionless while creating direct, scalable value accrual for $AMOS holders. Managed Hosting (AMOS Cloud or any third-party provider) uses separate fiat billing and is completely independent from the Relay economy.

**Why the relay is the monetization point:**
- Harness and agent are free to maximize adoption
- The relay is the natural monetization point (it connects supply and demand)
- 3% default is low enough to be competitive, high enough to sustain the network
- Fee is adjustable (1-5%) by Governance Council to respond to market conditions
- Fee is unavoidable (enforced on-chain by the relay settlement program)

### 3.3 Protocol Fee Allocation

The 3% protocol fee is distributed as follows:

```rust
// Implemented in amos_relay::protocol_fees module
pub const PROTOCOL_FEE_BPS: u64 = 300;           // 3% protocol fee

pub const FEE_ALLOCATION_BPS: FeeAllocation = FeeAllocation {
    staked_holders: 5000,   // 50% - Distributed proportionally to stakers (commercial bounties only)
    permanent_burn: 4000,   // 40% - Permanently burned (deflationary)
    labs: 1000,             // 10% - AMOS Labs (maximum alignment)
};
```

**Why These Percentages:**

| Pool | % | Rationale |
|------|---|-----------|
| **Staked Holders** | 50% | Maximum incentive to stake and participate on commercial bounties - core value proposition for token holders |
| **Permanent Burn** | 40% | Deflationary pressure, direct scarcity mechanism, all commercial bounty fees |
| **AMOS Labs** | 10% | Operational alignment - Labs lives or dies by the token, receives allocation in AMOS tokens |

**System vs. Commercial Bounties:**
- System Bounties: Treasury-funded, 0% protocol fee (for ecosystem development, tooling, infrastructure)
- Commercial Bounties: User-funded escrow, 3% protocol fee split as above

**AMOS Labs Allocation (10%):**
- Operational costs (legal, accounting, infrastructure)
- Team compensation in AMOS tokens (maximum alignment)
- Product development and platform improvements
- Labs receives this allocation entirely in AMOS tokens, creating direct incentive alignment

### 3.4 Value Accrual

Token value derives from:

1. **Fee Rights**: Claim on 50% of commercial bounty protocol fees
2. **Scarcity**: Fixed supply with ongoing burns
3. **Utility**: Platform access and governance
4. **Network Effects**: Growing contributor/user base

---

## 4. Decay Mechanism

### 4.1 Organic Economics: Decay Tied to Real Costs

**The core insight**: Decay is not arbitrary—it represents the REAL cost of running the platform.

Traditional token economics use fixed decay rates (e.g., "40% per year"). But why 40%? There's no connection to reality. AMOS takes a different approach:

```
ORGANIC DECAY MODEL:
Decay Rate = f(Platform Revenue, Platform Costs)

Profitable platform → Lower decay (2-10%)
Break-even platform → Base decay (10%)  
Unprofitable platform → Higher decay (up to 25%)
```

This creates **self-balancing equilibrium**:
- When the platform succeeds, token holders are rewarded with lower decay
- When costs exceed revenue, decay increases to recycle tokens for operations
- The token economy automatically adjusts without governance votes

### 4.2 Why This Matters

1. **Defensible**: Decay isn't punishment—it's maintenance cost. Like property taxes.
2. **Organic**: No arbitrary numbers. Decay reflects real economics.
3. **Aligned**: Token value rises when platform is profitable (low decay).
4. **Sustainable**: Platform can fund operations without external capital.

### 4.3 Dynamic Decay Formula

```rust
// Implemented in amos_core::token::decay module
// Base rate from platform economics
let base_rate = calculate_dynamic_decay_rate(platform_context);

// Adjust for profit/loss ratio
let profit_ratio = (revenue - costs) / costs;
let adjusted_rate = BASE_RATE - (profit_ratio * SENSITIVITY);

// Clamp to bounds
let decay_rate = adjusted_rate.clamp(MIN_RATE, MAX_RATE);

// Parameters:
const BASE_RATE: f64 = 0.10;     // 10% at equilibrium
const MIN_RATE: f64 = 0.02;      // 2% minimum (profitable platform)
const MAX_RATE: f64 = 0.25;      // 25% maximum cap
const SENSITIVITY: f64 = 0.05;   // How much profit affects rate
```

### 4.4 Grace Period

**All new stakes receive a 12-month grace period with ZERO decay.**

This provides:
- Time for new contributors to understand the system
- A "hook" period where they see revenue share working
- Psychological safety during onboarding
- Simple, easy-to-communicate rule

```
Month 0-12:  NO DECAY (grace period)
Month 12+:   Dynamic decay based on platform economics
```

### 4.5 Tenure-Based Decay Reduction

Long-term holders get reduced decay (on top of the dynamic base rate):

| Years Held | Reduction from Base Rate |
|------------|--------------------------|
| 0-1 | 0% (full dynamic rate) |
| 1-2 | 20% reduction |
| 2-5 | 40% reduction |
| 5+ | 70% reduction |

Example at different platform health levels:

```
Platform profitable (base = 5%):
- Year 0-1: 5.0% decay
- Year 1-2: 4.0% decay (20% reduction)
- Year 2-5: 3.0% decay (40% reduction)
- Year 5+:  1.5% decay (70% reduction)

Platform break-even (base = 10%):
- Year 0-1: 10.0% decay
- Year 1-2: 8.0% decay
- Year 2-5: 6.0% decay
- Year 5+:  3.0% decay

Platform struggling (base = 20%):
- Year 0-1: 20.0% decay
- Year 1-2: 16.0% decay
- Year 2-5: 12.0% decay
- Year 5+:  6.0% decay
```

### 4.6 Decay Example (with Grace Period)

```
Initial stake: 10,000 AMOS
Platform health: Profitable (5% base decay)
Year 0 floor: 500 AMOS (5%)
Year 5 floor: 2,500 AMOS (25%)

Month 0:  10,000 tokens (earned)
Month 6:  10,000 tokens (grace period - no decay!)
Month 12: 10,000 tokens (grace period ends)
Year 2:   9,500 tokens (5% decay - platform profitable!)
Year 3:   8,800 tokens (4% effective - tenure reduction)
Year 5:   7,500 tokens
Year 10:  4,500 tokens
Year 20:  2,500 tokens (floor - permanent)
```

**Key insight**: Your token value depends on platform success. When the platform is profitable, your decay is minimal. You're incentivized to build value!

### 4.7 Decay Recycling

Decayed tokens fund platform operations:

- **10%**: Burned (deflationary)
- **90%**: Returned to treasury (operational funding)

This creates a closed loop: decay funds the platform → platform becomes profitable → decay decreases → token value increases.

---

## 5. Wealth Preservation

### 5.1 12-Month Grace Period

All new stakes enjoy a **full year of zero decay**, providing:

- Time to understand the system before stakes shrink
- Opportunity to see revenue share working
- Psychological safety during onboarding
- A simple rule everyone can understand

After the grace period, decay begins at the tenure-based rate.

### 5.2 Graduated Decay Floor

Floor percentage **grows with tenure** to prevent early adopters from locking in permanent advantages while still rewarding long-term commitment:

| Tenure | Floor % | Rationale |
|--------|---------|-----------|
| 0-1 year | 5% | Earn your security |
| 1-2 years | 10% | Building commitment |
| 2-5 years | 15% | Established contributor |
| 5+ years | 25% | Maximum security |

This enables:

- Long-term planning and security
- Fair treatment of late joiners
- Rewards for sustained commitment

### 5.3 Staking Vaults

Lock tokens to reduce decay:

| Tier | Lock Period | Decay Reduction |
|------|-------------|-----------------|
| Bronze | 30 days | 20% |
| Silver | 90 days | 50% |
| Gold | 365 days | 80% |
| Permanent | No unlock | 95% |

### 5.4 Investment Profiles

The token economy accommodates multiple participation styles:

#### Profile A: Active Contributor
```
├── Earns tokens through work (code, sales, community)
├── No lock required
├── Decay offset by ongoing contributions
├── Stake maintained or grown through activity
└── Primary intended path
```

#### Profile B: Long-Term Investor (Permanent Lock)
```
├── Purchases tokens on exchange
├── Locks in Permanent vault (no unlock)
├── 95% decay reduction
├── Receives full revenue share
├── Has full governance rights
└── Traditional "buy and hold" - maximum commitment
```

#### Profile C: Medium-Term Believer (90-365 Day Lock)
```
├── Purchases tokens on exchange
├── Locks in Silver/Gold vault (90-365 days)
├── 50-80% decay reduction
├── Receives full revenue share
├── Has full governance rights
└── Balance between liquidity and preservation
```

#### Profile D: Speculator (No Lock)
```
├── Purchases tokens on exchange
├── No vault lock
├── 12-month grace period, then full decay
├── Can sell anytime for liquidity
├── Receives revenue share while holding
└── Trading on price appreciation
```

**Key Insight:** All paths are valid. The system doesn't prohibit buying—it ensures that passive holders gradually transfer stake to active contributors through decay, unless they commit to long-term locks.

---

## 6. Reward Calculation

### 6.1 The Simple Model

AMOS uses a **pool-based distribution** with the simplest possible rules:

```
Your Tokens = (Your Points / Total Points Today) × Daily Pool
```

**Two ways to earn points:**

1. **Sales**: 1 user signed up = 1 point
2. **Bounties**: Bounty value = points (50 AMOS bounty = 50 points)

That's it. No multipliers, no complexity scales, no formulas. A token is a token.

### 6.2 Why Pool-Based?

Fixed rewards don't work in reality:
- What if everyone signs up 1 million users one day?
- You'd blow through the treasury instantly
- The daily emission is the cap

Pool-based distribution ensures:
- Treasury is protected (never overspend)
- Proportionality is preserved (2x contribution = 2x tokens)
- Self-balancing economics
- Simple to understand

### 6.3 Sales Rewards

| Users Signed Up | Points | Example |
|-----------------|--------|---------|
| 1 | 1 | Betty refers her friend |
| 10 | 10 | Small team signs up |
| 100 | 100 | Medium business |
| 1,000 | 1,000 | Enterprise deal |
| 10,000 | 10,000 | Large corporation |

**Example calculation:**

```
Today's activity:
├── You signed up 100 users (100 points)
├── Alex signed up 50 users (50 points)
├── Betty signed up 10 users (10 points)
└── Total: 160 points

Daily pool: 16,000 AMOS

Your share: 100/160 = 62.5%
Your tokens: 16,000 × 62.5% = 10,000 AMOS

Alex's tokens: 16,000 × 31.25% = 5,000 AMOS
Betty's tokens: 16,000 × 6.25% = 1,000 AMOS
```

The ratio is preserved. You get 10x Betty because you signed up 10x users.

### 6.4 Bounty Rewards

Code and community contributions use a **bounty system**:

- Maintainers set bounty values on work items
- Contributors see bounty upfront
- Complete the work → get the bounty as points
- Points convert to tokens via pool share

| Bounty | Points | Example Work |
|--------|--------|--------------|
| 25 | 25 | Fix typo, answer support ticket |
| 50 | 50 | Minor bug fix, documentation |
| 150 | 150 | Tutorial, translation |
| 500 | 500 | New feature, security fix |
| 2,000 | 2,000 | Major feature, core infrastructure |

### 6.5 Combined Pool

All points go into the same daily pool:

```
Today's total activity:
├── Sales: 500 users signed up = 500 points
├── Code: 1,000 bounty points claimed
├── Community: 200 bounty points claimed
└── Total: 1,700 points

Daily pool: 16,000 AMOS

Example - you completed a 150-point bounty:
Your share: 150/1,700 = 8.8%
Your tokens: 16,000 × 8.8% = 1,412 AMOS
```

### 6.6 Sigmoid Emission Schedule

Daily emission follows a smooth sigmoid decay curve instead of discrete halvings. The sigmoid provides a predictable, ungameable schedule with no exploitable discontinuities:

    emission(t) = 100 + (16,000 - 100) / (1 + e^(0.005 × (t - 1,460)))

| Parameter | Value | Description |
|-----------|-------|-------------|
| Ceiling | 16,000 AMOS/day | Launch emission rate |
| Floor | 100 AMOS/day | Permanent minimum emission |
| Midpoint | 1,460 days (~4 years) | Emission at ~8,050/day |
| k | 0.005 (K_SCALED=50) | Steepness of decay curve |

| Year | Approx. Daily Emission |
|------|----------------------|
| 0 (launch) | ~15,900 AMOS |
| 1 | ~14,500 AMOS |
| 2 | ~12,300 AMOS |
| 4 (midpoint) | ~8,050 AMOS |
| 6 | ~3,800 AMOS |
| 8 | ~1,200 AMOS |
| 10 | ~350 AMOS |
| 13+ | approaches 100 AMOS (floor) |

First-decade total emission: ~25-27M tokens (~27% of 95M treasury). Early contributors earn more tokens per point, but late contributors earn tokens that are likely worth more (scarcity + network effects). Uses the same integer sigmoid math (EXP_LOOKUP table) as pool separation — no floating point on-chain.

### 6.7 Sigmoid Pool Separation

The daily emission (~16,000 AMOS/day at launch, decaying via sigmoid) is split dynamically between two pools using a separate sigmoid function:

**Technical Pool** (core platform work):
- Floor: ~80% at launch
- Ceiling: ~97% at maturity

**Growth Pool** (user acquisition & adoption):
- Ceiling: ~20% at launch
- Floor: ~3% at maturity

The transition follows a sigmoid curve, which smoothly shifts allocation from growth incentives (early bootstrap) to technical sustainability (long-term operation):

```
Formula: growth_cap(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))

Parameters:
  ceiling_bps = 2000 (20% max for growth)
  floor_bps = 300 (3% min for growth)
  midpoint_days = 540 (transition halfway point: ~1.5 years)
  k_scaled = 100 (curve steepness)

Timeline Example:
  Day 0:    Growth: 20%, Technical: 80%
  Day 270:  Growth: 12%, Technical: 88% (early growth phase)
  Day 540:  Growth: 10%, Technical: 90% (transition inflection)
  Day 1080: Growth: 4%, Technical: 96% (mature platform)
  Day 1800: Growth: 3%, Technical: 97% (asymptotic floor)
```

This allocation is implemented on-chain using an integer lookup table (no floating-point arithmetic) for deterministic, gas-efficient execution. The lookup table is updated weekly based on elapsed days since launch.

**Why Sigmoid Over Steps?**
- Step functions create artificial incentive cliffs that distort behavior
- Sigmoid provides smooth, predictable transition
- Contributors anticipate allocation changes (no surprises)
- Governance can't arbitrarily shift allocations mid-curve

### 6.8 What Users See

```
┌─────────────────────────────────────────────────────────────────┐
│  TODAY'S EARNINGS                                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  📊 Your Activity                                               │
│     Users signed up: 50                                         │
│     Bounties completed: 150 points                              │
│     Total points: 200                                           │
│                                                                 │
│  🏊 Today's Pool                                                 │
│     Total platform points: 2,500                                │
│     Your share: 8.0%                                            │
│     Daily pool: 16,000 AMOS                                     │
│                                                                 │
│  💰 Your Tokens: 1,280 AMOS                                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 6.9 Autonomous Bounty Generation (AMOS Thinking Time)

AMOS doesn't just execute tasks—it thinks about how to improve the platform and creates work opportunities for contributors.

#### Nightly Thinking Time

Every night, AMOS runs an autonomous reflection cycle:

```
┌─────────────────────────────────────────────────────────────────┐
│                    AMOS THINKING TIME (2am daily)               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. PERCEIVE: Analyze platform state                            │
│     ├── Errors and anomalies from logs                          │
│     ├── Open tickets and feature requests                       │
│     ├── User feedback and metrics                               │
│     └── Community activity                                      │
│                                                                 │
│  2. REFLECT: What could be improved?                            │
│     ├── Identify patterns and problems                          │
│     ├── Prioritize by impact and urgency                        │
│     └── Consider strategic goals                                │
│                                                                 │
│  3. IDEATE: Generate bounty ideas                               │
│     ├── Bugs to fix                                             │
│     ├── Features to build                                       │
│     ├── Content to create (blogs, tutorials)                    │
│     ├── Marketing campaigns                                     │
│     └── Documentation improvements                              │
│                                                                 │
│  4. SCORE: Assign point values                                  │
│     ├── Estimate effort (hours)                                 │
│     ├── Assess user impact                                      │
│     ├── Rate urgency and complexity                             │
│     └── Calculate fair points                                   │
│                                                                 │
│  5. CREATE: Post bounties to the board                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

#### AI-Scored Bounties

Every bounty (whether created by AMOS or humans) is scored by AI:

| Factor | Weight | Description |
|--------|--------|-------------|
| Effort | 20% | Estimated hours of work |
| Impact | 25% | Users affected |
| Urgency | 20% | Time sensitivity |
| Complexity | 20% | Technical difficulty |
| Strategic | 15% | Alignment with goals |

The scoring produces fair, consistent point values:

```rust
// AI scoring implemented in amos_core::token::emission
// Example bounty scoring
let score = score_bounty(BountyRequest {
    title: "Add dark mode to dashboard".to_string(),
    description: "Users have requested dark mode...".to_string(),
    bounty_type: BountyType::Feature,
});
// => BountyScore { points: 250, effort_score: 7, impact_score: 8, ... }
```

#### AI Work Review

When contributors submit completed work, AMOS reviews it:

1. **Quality Assessment**: Does the work meet requirements?
2. **Point Adjustment**: Exceptional work gets +25%, issues get -10-25%
3. **Feedback**: Constructive comments for the contributor
4. **Approval/Rejection**: Final decision

```rust
// Work review implemented in amos_core::token::emission
let review = review_work_submission(ReviewRequest {
    bounty_id: bounty.id,
    submission_notes: "Implemented dark mode with CSS variables...".to_string(),
});
// => ReviewResult { approved: true, final_points: 275, feedback: "Great work!..." }
```

#### Bounty Types

AMOS creates bounties across all contribution categories. There are 11 total contribution types: 8 **Technical pool** types and 3 **Growth pool** types.

**Technical Pool (8 types - 80% of daily emission at launch, rising to 97% at maturity):**

| Type | Examples | Typical Points | Multiplier |
|------|----------|----------------|----|
| Bug | Fix errors, crashes, data issues | 25-200 | 100% (10000 BPS) |
| Feature | New functionality | 100-500 | 100% (10000 BPS) |
| Documentation | Guides, API docs, READMEs | 25-100 | 100% (10000 BPS) |
| Content | Blog posts, tutorials, videos | 50-200 | 100% (10000 BPS) |
| Marketing | Ad copy, campaigns, outreach | 50-150 | 100% (10000 BPS) |
| Support | Answer questions, community help | 10-50 | 100% (10000 BPS) |
| Design | UI improvements, graphics | 75-300 | 100% (10000 BPS) |
| Testing | Test coverage, QA | 50-150 | 100% (10000 BPS) |

**Growth Pool (3 types - 20% of daily emission at launch, decaying to 3% floor via sigmoid):**

| Type | Examples | Typical Points | Multiplier |
|------|----------|----------------|----|
| Bug Report | Reporting security issues or platform bugs | 25-100 | 100% (10000 BPS) |
| Referral | Successfully referring new users | Variable | 60% (6000 BPS) |
| Signup | Creating account and completing onboarding | 50-100 | 40% (4000 BPS) |

The multipliers represent the reward rate relative to base bounty value. Growth pool contributions are designed to bootstrap initial user adoption while technical contributions dominate as the platform matures.

#### Human + AI Collaboration

- **AMOS** creates bounties based on platform analysis
- **Users** can also submit bounty ideas
- **Token holders** vote on feature priorities
- **AMOS** factors votes into bounty creation
- **Contributors** choose what to work on
- **AMOS** reviews and approves completed work

This creates a self-improving platform where the AI identifies needs and the community fulfills them.

---

## 7. Blockchain Integration

### 7.1 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  AMOS Platform (Off-Chain)                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ TokenStake  │  │ Contribution│  │ Decay Engine        │ │
│  │ (Internal)  │  │ Tracking    │  │ (Daily Job)         │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
└─────────┼────────────────┼────────────────────┼─────────────┘
          │                │                    │
          └────────────────┼────────────────────┘
                           │
              ┌────────────▼────────────┐
              │   Claim/Deposit Bridge  │
              │   (SolanaTokenService)  │
              └────────────┬────────────┘
                           │
              ┌────────────▼────────────┐
              │      Solana Network     │
              │  ┌─────────────────────┐│
              │  │ SPL Token (AMOS)    ││
              │  │ Fixed 100M Supply   ││
              │  └─────────────────────┘│
              │  ┌─────────────────────┐│
              │  │ Treasury Wallet     ││
              │  │ (Multisig)          ││
              │  └─────────────────────┘│
              └─────────────────────────┘
                           │
              ┌────────────▼────────────┐
              │      Jupiter DEX        │
              │  (Trading / Swaps)      │
              └─────────────────────────┘
```

### 7.2 Internal vs On-Chain Tokens

| Aspect | Internal (Platform) | On-Chain (Solana) |
|--------|---------------------|-------------------|
| Decay | Yes (daily) | No (frozen) |
| Revenue Share | Yes | No (must deposit) |
| Governance | Yes | No (must deposit) |
| Trading | No | Yes |
| Gas Fees | None | ~$0.0003 |

### 7.3 Claim Flow

1. User requests claim via API
2. Platform validates balance
3. Background job sends SPL transfer
4. Internal balance deducted
5. User receives tokens in wallet

### 7.4 Deposit Flow

1. User sends tokens to treasury
2. User submits tx signature
3. Platform verifies on-chain
4. Internal stake created
5. User regains revenue/governance rights

---

## 7.5 Trustless Revenue Distribution (On-Chain Treasury)

### The Trust Problem

Traditional platforms have a critical vulnerability: revenue distribution depends on promises.

```
TRADITIONAL MODEL (Requires Trust):
Customer pays → Company holds money → Company decides payouts → Maybe you get paid

POTENTIAL FAILURES:
- Company changes the rules
- Company goes bankrupt
- Company gets hacked
- Bad actor gains control
```

AMOS solves this with **on-chain, immutable revenue distribution**.

### The Zero-Custody Architecture

**Money never stops moving. No one holds the bag.**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    ZERO CUSTODY REVENUE FLOW                                │
│                                                                             │
│  BOUNTY           RELAY           SETTLEMENT       SOLANA                   │
│  PAYOUT           NODE            PROGRAM          TREASURY                 │
│                                                                             │
│  Protocol fee ───► Relay ────────► Converts ──────► Treasury                │
│  (on bounty       settles         (if needed)      Program                  │
│   payout)         on-chain                           │                      │
│                                                      │ IMMEDIATE SPLIT      │
│                                                      ▼                      │
│                                               ┌──────────────┐              │
│                                               │  $50 $AMOS   │              │
│                                               │  Holder Pool │──► Claimable │
│                                               ├──────────────┤              │
│                                               │  $40 $AMOS   │              │
│                                               │  Burned      │──► Gone      │
│                                               ├──────────────┤              │
│                                               │  $10 $AMOS   │              │
│                                               │  AMOS Labs   │──► Operations│
│                                               └──────────────┘              │
│                                                                             │
│  TIME FROM BOUNTY PAYOUT TO ON-CHAIN SPLIT: < 60 seconds                   │
│  HUMAN CUSTODY TIME: 0 seconds                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Immutable Split Constants (Solana Program)

The revenue allocation is **baked into deployed program code**:

```rust
// programs/amos_treasury/src/constants.rs
// IMMUTABLE - Cannot be changed after deployment

pub const FEE_HOLDER_SHARE_BPS: u64 = 5000;   // 50% to staked token holders (commercial bounties only)
pub const FEE_BURN_SHARE_BPS: u64 = 4000;     // 40% permanently burned (deflationary)
pub const FEE_LABS_SHARE_BPS: u64 = 1000;     // 10% to AMOS Labs (maximum alignment)

pub const MIN_STAKE_DAYS: i64 = 30;        // Must hold 30 days for revenue
pub const MIN_STAKE_AMOUNT: u64 = 100;     // Minimum 100 AMOS to qualify
```

**No admin key can change these values.** The only way to modify:
1. Deploy a completely new program (new address)
2. Migrate all users (they'd have to agree)
3. Move liquidity (requires DAO supermajority vote)

### Payment Options (Progressive Disclosure)

Users can pay in multiple ways, with crypto rails invisible to those who want simplicity:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    TIERED PAYMENT OPTIONS                                   │
│                                                                             │
│  TIER 1: NORMIE MODE (Default)                                             │
│  ─────────────────────────────                                              │
│  • Pay in USD, see USD prices                                               │
│  • Behind scenes: Fiat on-ramp to $AMOS (optional third-party)             │
│  • Customer never knows about crypto                                        │
│  • Enterprise-friendly, no wallet required                                  │
│                                                                             │
│  TIER 2: CRYPTO-AWARE (Opt-in)                                             │
│  ────────────────────────────                                               │
│  • "Pay in $AMOS" option (direct wallet payment)                           │
│  • Connect Solana wallet                                                    │
│  • Direct on-chain payments, zero middlemen                                │
│  • Swap from USDC/SOL via DEX if needed                                    │
│                                                                             │
│  TIER 3: BUILDER MODE (Advanced)                                           │
│  ───────────────────────────────                                            │
│  • API pricing in AMOS tokens                                               │
│  • Programmatic access for developers                                       │
│  • Stake AMOS to get API rate discounts                                    │
│  • Maximum integration with token economy                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### AMOS Payment Flywheel

When users pay directly in AMOS tokens:

```
Customer pays 10,000 AMOS
        │
        ▼
┌───────────────────────────────┐
│  AMOS PAYMENT PROCESSOR       │
│  ─────────────────────────    │
│                               │
│  50% to Holder Pool           │
│  └── 5,000 AMOS distributed   │
│  └── To staked token holders   │
│                               │
│  40% Permanently Burned       │
│  └── 4,000 AMOS burned        │
│  └── Deflationary pressure    │
│                               │
│  10% to AMOS Labs             │
│  └── 1,000 AMOS to Labs       │
│  └── Maximum alignment        │
└───────────────────────────────┘

RESULT:
• Constant buy pressure (users need AMOS for bounties)
• Deflationary pressure (40% of all fees permanently burned)
• 50% flows directly to staked holders on commercial bounties
• AMOS Labs funded entirely in AMOS tokens (aligned incentives)
• System bounties: Treasury-funded, 0% fee
• All transactions in AMOS tokens (AMOS-only model)
```

**Why 50/40/10 (AMOS-Only Model)?**

The AMOS-only model ensures maximum alignment and deflationary pressure:
- 50% to holders creates strong incentive for staking and participation
- 40% permanently burned ensures continuous scarcity and deflationary mechanics
- 10% to AMOS Labs ensures maximum alignment - Labs lives or dies by the token
- All transactions denominated entirely in AMOS tokens (no USDC, no fiat)

### Claim Mechanism

Token holders claim their share of the holder pool:

```rust
/// Token holders claim their share of the holder pool
/// Proportional to stake, fully automated
pub fn claim_revenue(ctx: Context<ClaimRevenue>) -> Result<()> {
    let holder = &ctx.accounts.holder;
    let pool = &ctx.accounts.holder_pool;
    
    // Verify eligibility
    require!(
        holder.stake_amount >= MIN_STAKE_AMOUNT,
        ErrorCode::InsufficientStake
    );
    require!(
        holder.stake_start_date <= Clock::get()?.unix_timestamp - (MIN_STAKE_DAYS * 86400),
        ErrorCode::StakeTooRecent
    );

    // Calculate share: (your_stake / total_stake) * pool_balance
    let share_bps = (holder.stake_amount * 10000) / total_eligible_stake;
    let payout = (pool.balance * share_bps) / 10000;

    // Transfer $AMOS to holder's wallet - NO APPROVAL NEEDED
    token::transfer(ctx.accounts.to_holder_wallet(), payout)?;
    
    Ok(())
}
```

**Key properties:**
- Claim anytime (no waiting for monthly distribution)
- No human approval required
- Proportional to stake
- On-chain, verifiable, auditable

### Multi-Sig Governance Wallets

For funds that require human judgment:

| Pool | Control | Time-Lock | Purpose |
|------|---------|-----------|---------|
| **Holder Pool** (50%) | Automatic | None | Direct claims by stakers (commercial bounties only) |
| **Permanent Burn** (40%) | Automatic | None | Permanently burned (deflationary) |
| **AMOS Labs** (10%) | Automatic | None | Operations + product development (received entirely in AMOS) |

### System vs. Commercial Bounties

- **System Bounties**: Treasury-funded, 0% protocol fee (ecosystem development, tooling, infrastructure)
- **Commercial Bounties**: User-funded escrow, 3% protocol fee with 50/40/10 split (50% holders, 40% burn, 10% AMOS Labs)

### AMOS Labs Allocation Structure

The AMOS Labs allocation (10% of commercial bounty fees) is received entirely in AMOS tokens with maximum alignment - Labs lives or dies by the token:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    AMOS LABS ALLOCATION                                     │
│                                                                             │
│  ALLOCATION SOURCE:                                                         │
│  • 10% of every commercial bounty's 3% protocol fee                        │
│  • Received directly in AMOS tokens (maximum alignment)                    │
│  • No USD conversion, no fiat rails                                         │
│                                                                             │
│  USE OF ALLOCATION:                                                         │
│  1. Operations (legal, accounting, infrastructure)                         │
│  2. Team compensation (all in AMOS tokens)                                 │
│  3. Product development and platform improvements                          │
│  4. Research and AI advancement                                            │
│                                                                             │
│  ALIGNMENT MECHANISM:                                                       │
│  • Labs receives compensation entirely in AMOS tokens                      │
│  • Token price up → Labs can fund more work                                │
│  • Token price down → Labs has less funding (natural constraint)           │
│  • No treasury backstop or guaranteed USD budget                           │
│  • Labs success is directly tied to AMOS token success                     │
│                                                                             │
│  SYSTEM BOUNTIES:                                                           │
│  • Treasury-funded separately (no protocol fee)                             │
│  • For ecosystem development, tooling, infrastructure                       │
│  • Governance-voted by stakers                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Bootstrap Scenario: No Stakers Yet

**What happens if no one is staking at launch?**

```
Week 1: Relay launches (Commercial Bounties)
├── Protocol Fees: $10,000
├── Holder Pool: $5,000 (50%)
├── Permanent Burn: $4,000 (40%)
├── AMOS Labs: $1,000 (10%)
├── Stakers: 0
└── Result: Holder Pool ACCUMULATES for future stakers

Week 2: First staker joins
├── Accumulated Pool: $14,000 (two weeks of fees)
├── Staker with 10,000 AMOS: Can claim $14,000!
└── Result: Early stakers get accumulated rewards

DESIGN RATIONALE:
• Creates strong incentive to stake early
• No "lost" revenue - it's always claimable
• First movers are rewarded for taking the risk
• Aligns incentives: stake early, earn more
```

### Settlement Delay and Refund Handling

Revenue doesn't flow on-chain instantly to handle refunds:

```
Bounty approved and settled on relay:
├── Agent receives 97% of bounty reward (instant, on-chain)
└── 3% protocol fee withheld by relay settlement program

Fee distribution (immediate, on-chain):
├── 50% → Holder Pool PDA → Claimable by stakers
├── 40% → Burn address → Permanently removed (deflationary)
└── 10% → AMOS Labs account → Operations + product development

SETTLEMENT TIMING:
• Fees flow on-chain immediately upon bounty approval
• No batching needed - each settlement is a single transaction
• No refund window - bounty approval is final
• Dispute resolution happens BEFORE approval, not after
```

### Fee Reconciliation

Protocol fees are calculated and enforced on-chain by the relay settlement program:

```
FEE MODEL:
Protocol Fee = Bounty Payout × 0.03 (3%)

Example:
├── Bounty posted: 10,000 AMOS reward
├── Agent completes and submits work
├── Poster approves submission
├── Settlement: 9,700 AMOS to agent, 300 AMOS protocol fee
└── Fee split: 150 holder / 120 burned / 30 AMOS Labs

ALL ON-CHAIN:
├── Every fee is a Solana transaction
├── Verifiable by anyone on block explorer
├── No reconciliation needed - math is in the program
└── Treasury reports generated from on-chain data
```

### Trust Guarantees

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    AMOS TRUST GUARANTEE                                     │
│                                                                             │
│  "Your fee share is protected by math, not promises"                        │
│                                                                             │
│  ✓ 50% holder share is IMMUTABLE (in deployed program code)                │
│  ✓ Fees flow on-chain immediately upon bounty settlement                   │
│  ✓ All transactions on-chain (publicly auditable)                          │
│  ✓ Claim anytime (no waiting for monthly distribution)                     │
│  ✓ No admin keys can change the split                                       │
│  ✓ Fork-proof (program address is unique to our deployment)                │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Claim Mechanics

When a bounty is approved, the worker (agent or human) must claim the reward within a timeout period:

```
CLAIM PARAMETERS:
├── Default timeout: 72 hours
├── Minimum timeout: 1 hour (for urgent bounties)
├── Maximum timeout: 720 hours (30 days)
├── Auto-release: Permissionless (anyone can trigger)

CONCURRENT CLAIM LIMITS (by Trust Level):
├── Trust Level 1: 3 concurrent claims
├── Trust Level 2: 5 concurrent claims
├── Trust Level 3: 8 concurrent claims
├── Trust Level 4: 12 concurrent claims
├── Trust Level 5: 20 concurrent claims

CLAIM FLOW:
1. Bounty poster approves work submission
2. Worker receives claim token (valid for 72h default)
3. Worker calls claim_bounty transaction
4. Tokens transferred to worker wallet
5. If worker doesn't claim within timeout:
   ├── Bounty automatically released
   ├── Poster can reclaim funds or reassign
   └── Concurrent claim slot freed
```

Concurrent limits prevent resource exhaustion and encourage workers to complete bounties rather than hoarding claims. Trust levels are earned through positive contribution history.

### Dispute Mechanism

If a worker believes their submission was rejected unfairly, they can file a dispute:

```
DISPUTE PARAMETERS:
├── Filing window: 48 hours after rejection
├── Anti-frivolous stake: 5% of bounty value (burned if dispute denied)
├── Resolution timeout: 7 days
├── Default outcome: Worker-favorable (upheld if no resolution)

DISPUTE FLOW:
1. Worker receives rejection + feedback
2. Worker has 48h window to file dispute with evidence
3. Worker stakes 5% of bounty value
4. Dispute reviewed by human or AI reviewer (not original reviewer)
5. 7-day resolution period
6. If no resolution within 7 days → Default to UPHELD

DISPUTE OUTCOMES:

UPHELD (Bounty pays worker):
├── Worker receives full bounty + tokens
├── Worker's stake returned
├── Original reviewer reputation hit (platform impact)
└── Lesson: Reviews must be fair and well-documented

DENIED (Bounty returns to board):
├── Bounty value returned to treasury
├── Worker's 5% stake burned (deflationary)
├── Worker reputation unchanged
└── Lesson: Only file genuine disputes
```

The dispute mechanism balances fairness (workers can contest rejections) with incentive alignment (frivolous disputes are costly). The default worker-favorable outcome prevents lazy reviewers from rejecting good work without consequence.

### Anti-Gaming Mechanisms (Relay-Level)

The relay system includes several safeguards against exploitation and bad-faith behavior:

```
FALSE SUBMISSION PENALTY:
├── Invalid or low-quality submission: 5% reputation hit per incident
├── Reputation impact: Affects future claim limits and visibility
├── Accumulates: Multiple invalid submissions → Reputation floor
├── Recovery: Earned through valid contributions over time
└── Goal: Discourage spam without permanent bans

SELF-DEALING PREVENTION:
├── Bounty poster cannot claim own commercial bounties
├── Waiting period: 24 hours minimum after posting
├── Exception: System bounties (funded by treasury, no self-dealing risk)
├── Why: Prevents collusion and artificial token minting
├── Enforcement: Relay validates poster != worker on settlement
└── Example: Alice posts bounty at 9am, can claim at 9am next day

VERIFICATION CONTRIBUTION TYPE:
├── Higher-trust contributors can verify other submissions
├── Verification bounty multiplier: 110% (11000 BPS)
├── Trust level requirement: Level 3 or higher
├── Purpose: Encourage quality gatekeeping
├── Reputation reward: Successful verifications boost reputation
└── Example: Verify 10 high-quality submissions → Expert badge
```

These mechanisms work together to maintain a healthy bounty marketplace where quality is rewarded and gaming is economically irrational:

```
GAMING ATTEMPT SCENARIOS:

1. SUBMISSION SPAM:
   ├── Alice submits 50 low-quality bounties
   ├── 40 rejected: 40 × 5% = 200% reputation penalty
   ├── Reputation floor: Can't post more bounties
   └── Result: Cost > benefit (self-correcting)

2. COLLUSION (Alice and Bob):
   ├── Alice posts $1000 bounty at 9am
   ├── Bob wants to claim at 9:30am (same day)
   └── 24h cooldown blocks: Bob must wait until next day
   └── Relay has time to detect suspicious pattern

3. SELF-DEALING (Alice as Both Poster & Worker):
   ├── Alice posts bounty for "review my code"
   ├── Alice submits work 1 hour later
   └── Relay rejects: Poster has not waited 24h
   └── Must repost or find different worker
```

---

### Remaining Trust Points (Unavoidable)

Complete honesty about what you still must trust:

| Trust Point | Why It Exists | Mitigation |
|-------------|---------------|------------|
| **Relay Operator** | Runs relay node infrastructure | Open source, decentralizable |
| **Bounty Approval** | Human/AI reviews work quality | Dispute resolution before settlement |
| **Multi-sig Signers** | Approve treasury/ops spending | Elected by token holders, time-locks |
| **Oracle (future)** | DEX price feeds for market data | Multiple oracle aggregation planned |

---

## 8. Governance

### 8.1 Voting Power

Voting power proportional to current stake (post-decay):

```
Voting Power = Current Stake / Total Active Stakes
```

### 8.2 Governance Scope (Expanded)

Token holders vote on multiple categories with different requirements:

| Category | Description | Min Stake | Quorum | Threshold |
|----------|-------------|-----------|--------|-----------|
| **System Bounty Allocation** | Treasury-funded bounties (no protocol fee) | 1,000 | 30% | 50% (majority) |
| **Treasury Usage** | Fund usage proposals | 5,000 | 40% | 50% (majority) |
| **Feature Priority** | Feature prioritization | 500 | 20% | 50% (majority) |
| **Partnership** | Strategic partnerships | 2,500 | 35% | 50% (majority) |
| **Parameter Change** | Decay/emission adjustments | 10,000 | 50% | 66.7% (supermajority) |
| **Constitutional** | Core mechanic changes | 25,000 | 60% | 66.7% (supermajority) |

### 8.3 Proposal Process

1. Stake minimum AMOS to submit proposal (varies by type)
2. Discussion period (5-21 days depending on type)
3. Voting period (5-21 days depending on type)
4. Quorum must be met
5. Threshold must be passed
6. Failed proposals burn 10% of staked amount (anti-spam)

### 8.4 Supermajority Requirements

**Parameter** and **Constitutional** changes require:

- 2/3 (66.7%) approval to pass
- Higher quorum (50-60%)
- Longer discussion/voting periods
- Higher stake to propose

This protects core mechanics from minority capture while allowing evolution.

### 8.5 Contribution Type Registry (On-Chain Governance)

The multipliers for all 11 contribution types are stored in a governance-updatable Solana PDA (Program Derived Address) rather than hardcoded constants. This allows the community to adjust incentives while maintaining immutability guarantees:

```
CONTRIBUTION TYPE REGISTRY PDA:
├── Stores all 11 contribution type multipliers
├── Updatable via governance proposal
├── Individual entry freeze (one-way, permanent)
├── Full registry freeze (one-way, permanent)
└── Lookup table for efficient on-chain access

GRADUATED FREEZE MECHANISM:
├── Individual entries can be individually frozen (locked in current value)
├── No unfreeze instruction exists (permanence)
└── Example: Bug report multiplier could be frozen at 100% (10000 BPS)

FULL REGISTRY FREEZE:
├── Can freeze entire registry with governance vote
├── Disables ALL updates to contribution types
├── Emergency brake against governance griefing
└── Never undone once triggered

AUTO-FREEZE DEADLINE:
├── 3 years from platform launch: Permissionless auto-freeze trigger
├── Anyone can call freeze_by_deadline() after 3 years
├── Guarantees mechanism becomes immutable
└── Date: April 2029 (if launching April 2026)

GOVERNANCE EXTENSIONS (Limited):
├── Up to 2 governance extensions possible
├── Each extension: Exactly 1 year (not variable)
├── Total extension window: 2 years maximum
├── Vote required: Supermajority (66.7%)
├── New deadline: 5 years from launch (April 2031)

ABSOLUTE MAXIMUM:
├── 5 years total (deadline + 2 × 1-year extensions)
├── After 5 years: Frozen permanently
├── No further extensions possible
└── Immutability guaranteed by on-chain logic
```

This design protects against governance capture while allowing the community to adapt incentives during the critical bootstrap phase. The deadline ensures that eventually the contribution types become immutable, matching the immutability of the token supply.

---

## 9. Security Considerations

### 9.1 Smart Contract Security

- SPL Token standard (battle-tested)
- No custom contract logic (reduces attack surface)
- Treasury protected by multisig (2-of-3)
- Mint authority permanently disabled (revoked April 15, 2026)

### 9.2 Platform Security

- Internal ledger is source of truth
- Decay runs in isolated background job
- Claim/deposit requires authenticated user
- Rate limiting on all endpoints

### 9.3 Economic Security

- Sybil resistant (KYC for large claims)
- Whale resistant (decay mechanism)
- Rug-proof (no admin keys on token)
- Governance capture resistant (supermajority for critical changes)

---

## 10. Technical Implementation

### 10.1 Key Data Structures

```rust
// Implemented in amos_core::token::decay module
// StakeContext - Ownership record
pub struct StakeContext {
    pub user_id: String,

    // Amounts
    pub initial_amount: u64,    // Original stake
    pub current_amount: u64,    // After decay

    // Graduated floor (grows with tenure)
    pub tenure_years: u32,

    // Decay
    pub decay_rate: f64,        // Annual rate
    pub last_decay_at: i64,     // Last decay application

    // Vaulting
    pub staking_tier: StakingTier,  // Bronze/Silver/Gold/Permanent
    pub locked_until: Option<i64>,  // Lock expiration
}

// Implemented in amos_platform::governance module
// GovernanceProposal - Voting proposals
pub struct GovernanceProposal {
    pub proposer_id: String,
    pub proposal_type: ProposalType,  // RAndD, Treasury, Feature, Partnership, Parameter, Constitutional
    pub status: ProposalStatus,       // Draft, Discussion, Voting, Passed, Failed, Cancelled, Executed
}

// Implemented in amos_core::token::emission module
// Contribution - Work record
pub struct Contribution {
    pub user_id: String,
    pub contribution_type: ContributionType,
    pub complexity: u8,
    pub points: u64,          // Base points earned
    pub token_value: u64,     // Tokens awarded from pool
    pub status: ContributionStatus,  // Pending/Approved/Rejected
}
```

### 10.2 Key Services

```rust
// Token decay (runs daily)
// Implemented in amos_core::token::decay
pub async fn apply_daily_decay(stakes: Vec<StakeContext>) -> Result<()>

// Pool-based reward calculation
// Implemented in amos_core::token::emission
pub fn calculate_bounty_award(
    contribution_type: ContributionType,
    complexity: u8,
    daily_pool: u64,
) -> Result<u64>

// Solana operations
// Implemented in amos_platform::blockchain
pub async fn send_tokens(to: Pubkey, amount: u64) -> Result<Signature>
pub async fn verify_deposit(tx_signature: Signature) -> Result<bool>

// DEX integration
// Implemented in amos_platform::dex
pub async fn quote_amos_to_usdc(amount: u64) -> Result<Quote>
```

### 10.3 API Endpoints

```
# Token Economy
GET  /api/v1/token_economy/stats
GET  /api/v1/token_economy/distribution
GET  /api/v1/token_economy/leaderboard

# Wallet
POST /api/v1/wallet/connect
GET  /api/v1/wallet/balance
POST /api/v1/wallet/claim
POST /api/v1/wallet/deposit

# Governance
GET  /api/v1/governance/proposals
POST /api/v1/governance/proposals
POST /api/v1/governance/proposals/:id/vote
GET  /api/v1/governance/proposals/:id

# Swaps
GET  /api/v1/swap/quote
GET  /api/v1/swap/price
POST /api/v1/swap/prepare
```

---

## 11. Economic Modeling & Sustainability Analysis

This section models various market scenarios, stress tests, and long-term implications of the AMOS token economy.

### 11.1 Token Distribution Timeline

Tokens enter circulation gradually through contributor rewards:

```
Emission follows a smooth sigmoid curve — no discrete halving events:

  E_daily(t) = 100 + (16,000 - 100) / (1 + e^(0.005 × (t - 1,460)))

Year 0-1:   avg ~14,500 AMOS/day × 365 = ~5,292,500 AMOS (5.6%)
Year 1-2:   avg ~12,300 AMOS/day × 365 = ~4,489,500 AMOS (4.7%)
Year 2-3:   avg ~10,000 AMOS/day × 365 = ~3,650,000 AMOS (3.8%)
Year 3-4:   avg  ~8,050 AMOS/day × 365 = ~2,938,250 AMOS (3.1%)
Year 4-5:   avg  ~5,900 AMOS/day × 365 = ~2,153,500 AMOS (2.3%)
Year 5-6:   avg  ~3,800 AMOS/day × 365 = ~1,387,000 AMOS (1.5%)
Year 6-7:   avg  ~2,200 AMOS/day × 365 =   ~803,000 AMOS (0.8%)
Year 7-8:   avg  ~1,200 AMOS/day × 365 =   ~438,000 AMOS (0.5%)
Year 8-9:   avg    ~600 AMOS/day × 365 =   ~219,000 AMOS (0.2%)
Year 9-10:  avg    ~350 AMOS/day × 365 =   ~127,750 AMOS (0.1%)
Year 13+:   100 AMOS/day (floor, ongoing)

TOTAL after 10 years: ~25-27M AMOS distributed (~27% of supply)
```

**Key Insight**: After 10 years, ~73% of tokens remain in the bounty treasury. The sigmoid curve front-loads emission during the critical bootstrap phase while maintaining a permanent floor. There's no "everyone sells" scenario because tokens are earned incrementally through bounties, and the smooth curve eliminates gaming edges around discrete halving events.

### 11.2 Liquidity Pool Dynamics

#### Initial Pool Setup

```
Example Initial Investment: $5,000 USDC + AMOS earned through bounties
├── $5,000 USDC
└── 250,000 AMOS (at $0.02/AMOS, earned via bounty contributions)

Pool State:
  USDC Reserve: 5,000
  AMOS Reserve: 250,000
  Constant Product (k): 5,000 × 250,000 = 1,250,000,000
```

#### AMM Price Formula (Constant Product)

```
price = USDC_reserve / AMOS_reserve
k = USDC_reserve × AMOS_reserve (constant)
```

#### LP Compensation Model

Liquidity providers earn from multiple sources:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    LP REVENUE STREAMS                                       │
│                                                                             │
│  1. TRADING FEES (Ongoing)                                                 │
│     ├── 0.25% of every swap goes to LP fee pool                           │
│     ├── Distributed proportionally to LP share                            │
│     └── Example: $100k daily volume = $250/day to LPs                     │
│                                                                             │
│  2. LP INCENTIVES (Year 1-3 Bootstrap)                                     │
│     ├── Funded from bounty treasury via governance vote                    │
│     ├── Governance Council allocates LP incentive bounties as needed       │
│     ├── Early LPs can earn through LP-specific bounties                   │
│     └── Distributed weekly to all LPs proportionally                      │
│                                                                             │
│  3. FOUNDER LP TIER (Special - One-time)                                  │
│     ├── First $10k of liquidity = Founder LP status                       │
│     ├── Permanent 0.05% fee share (even after LP withdrawal)              │
│     ├── 2x governance weight for LP tokens                                │
│     └── Priority on first 1M AMOS of LP incentives                       │
│                                                                             │
│  RISKS:                                                                     │
│  └── Impermanent loss if AMOS price moves significantly                   │
│  └── IL can exceed fee+incentive earnings in extreme moves                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Founder LP Math (AMOS Labs):**

```
AMOS Labs provides initial liquidity (USDC + AMOS earned through bounties)

Year 1 Earnings:
├── Trading fees: ~$3,000-15,000 (depends on volume)
├── LP incentives: Allocated via governance bounties
└── Founder LP keeps permanent 0.05% fee share

As more LPs join:
├── More LPs = Deeper liquidity
├── Deeper liquidity = More trading
├── More trading = More fees for everyone
├── And Founder LP keeps permanent 0.05% fee share
```

**Impermanent Loss Consideration:**

If AMOS 10x from $0.01 to $0.10:
- Just holding: $55,000 value
- As LP: ~$31,600 value (after IL)
- BUT with fees + incentives: ~$50,000+ total

LP incentive bounties (funded via governance) are designed to offset IL for early LPs.

#### Liquidity Bootstrapping Strategy

**Recommended Approach: Start Medium, Reserve for Defense**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    LIQUIDITY BOOTSTRAPPING PLAYBOOK                         │
│                                                                             │
│  PHASE 1: INITIAL POOL                                                      │
│  ══════════════════════════════════════════                                 │
│  Day 1:                                                                     │
│  ├── Create pool: USDC + AMOS (earned through bounties)                   │
│  ├── Starting price: $0.02/AMOS (not $0.01 - too cheap)                   │
│  ├── Lock in Founder LP status immediately                                │
│  └── Reserve funds for price defense                                       │
│                                                                             │
│  WHY $0.02 NOT $0.01:                                                      │
│  ├── $0.01 invites whales to scoop cheap                                  │
│  ├── $0.02 is still 100x upside to $2.00                                  │
│  ├── Less AMOS needed: 125k instead of 250k                               │
│  └── Better price discovery (room to go up AND down)                      │
│                                                                             │
│  PHASE 2: RESPOND TO MARKET (Weeks 1-4)                                    │
│  ═════════════════════════════════════════                                  │
│  IF price rises to $0.05:                                                  │
│  ├── Add $2k more at $0.05                                                │
│  ├── You deploy fewer AMOS at higher price                                │
│  └── Better average entry                                                  │
│                                                                             │
│  IF price drops to $0.01:                                                  │
│  ├── Add $2k to stabilize and show confidence                             │
│  ├── You accumulate more AMOS at lower price                              │
│  └── Signal: "We believe in this"                                          │
│                                                                             │
│  IF price stable:                                                          │
│  ├── Wait - no need to rush                                                │
│  ├── Let market find equilibrium                                           │
│  └── Add when there's clear demand                                         │
│                                                                             │
│  PHASE 3: DEEPEN FOR STABILITY (Month 2+)                                  │
│  ════════════════════════════════════════                                   │
│  Once price stabilizes:                                                    │
│  ├── Add remaining liquidity to deepen pool                               │
│  ├── Deeper pool = less volatility = more traders                        │
│  └── Goal: $50k+ total liquidity for healthy market                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Defending Against Whale Attacks

Thin initial liquidity naturally protects against whales:

```
ATTACK: Whale wants to buy 1M AMOS cheap
═══════════════════════════════════════

THIN POOL: $5k / 125k AMOS at $0.02

Whale buys with $10k:
├── Can extract: ~71k AMOS (57% of pool!)
├── Price moves: $0.02 → $0.14 (7x!)
├── Whale paid avg: $0.14 per AMOS
├── Expected cheap buy: $0.02, Actual: $0.14
└── Whale overpaid 7x due to slippage

RESULT: AMM curve punishes aggressive buying.
        You can now add liquidity at $0.14!

DEEP POOL: $50k / 1.25M AMOS at $0.02

Whale buys with $10k:
├── Can extract: ~500k AMOS (40% of pool)
├── Price moves: $0.02 → $0.033 (1.65x)
├── Whale paid avg: $0.02 per AMOS
└── Whale got exactly what they wanted

LESSON: Start thin to force price discovery,
        deepen after market finds equilibrium.
```

#### LP Anti-Dump Mechanics (On-Chain Enforcement)

```rust
// programs/amos_treasury/src/constants.rs

// LP reward vesting: 30 days to claim full rewards
pub const LP_VESTING_SECONDS: i64 = 30 * 24 * 60 * 60;

// Early withdrawal penalties (forfeited rewards return to pool)
pub const LP_EARLY_WITHDRAW_PENALTY_BPS: [u64; 4] = [
    10000,  // Day 1-7:   100% forfeit
    7500,   // Day 8-14:  75% forfeit
    5000,   // Day 15-21: 50% forfeit
    2500    // Day 22-30: 25% forfeit
];

// Time-weighted multipliers (reward early LPs)
pub const LP_WEEK_1_MULTIPLIER: u64 = 200;    // 2.0x
pub const LP_WEEK_2_4_MULTIPLIER: u64 = 150;  // 1.5x
pub const LP_BASELINE_MULTIPLIER: u64 = 100;  // 1.0x

// Lockup bonuses
pub const LP_LOCK_30_DAY_BONUS_BPS: u64 = 2000;   // +20%
pub const LP_LOCK_90_DAY_BONUS_BPS: u64 = 5000;   // +50%
pub const LP_LOCK_1_YEAR_BONUS_BPS: u64 = 10000;  // +100%
```

**Enforcement Logic:**

```
FARM-AND-DUMP ATTEMPT:
├── LP deposits $10k on Day 1
├── Earns 100 AMOS in incentives over 7 days
├── Tries to withdraw on Day 7
│
├── Penalty: 100% forfeit (Day 1-7 window)
├── LP gets: 0 AMOS incentives
├── Forfeited 100 AMOS: Returns to incentive pool
│
└── RESULT: Dumper gets nothing, patient LPs get more

COMMITTED LP:
├── LP deposits $10k on Day 1
├── Earns 100 AMOS in incentives over 30 days
├── Withdraws on Day 30
│
├── Penalty: 0% (full vest complete)
├── LP gets: 100 AMOS + trading fees
│
└── RESULT: Patient LPs are rewarded
```

#### Why Other LPs Joining is GOOD

```
CONCERN: "What if other LPs flood in and dilute me?"

REALITY:
├── More LPs = Deeper liquidity
├── Deeper liquidity = More trading
├── More trading = More fees for everyone
│
├── Your Founder LP 0.05% fee is PERMANENT
├── It does NOT dilute when others join
├── You want a liquid, active market
│
└── A $1M pool with 1% share beats
    a $10k pool with 100% share

THE GOAL: Healthy market, not LP monopoly
```

### 11.3 AMM Price Protection

The constant-product AMM formula provides natural protection against sell pressure:

| Scenario | Sell Amount | Price Drop | Slippage |
|----------|-------------|------------|----------|
| Moderate (10%) | 584k AMOS | 79% | 54% |
| Panic (50%) | 2.9M AMOS | 98% | 85% |
| Total Collapse | 5.8M AMOS | 99.4% | 99%+ |

**Key Insight**: Aggressive sellers punish themselves with massive slippage, disincentivizing bank runs.

### 11.4 Buy Pressure: Revenue-Based Buyback

Monthly revenue creates sustained buying pressure that exceeds worst-case sell pressure:

| Annual Commercial Fees | Holder Pool (50%) | vs Sell Pressure | Net Effect |
|----------------------|-------------------|------------------|------------|
| $1.2M | $600k/year | ~$29k worst case | Strong net buying |
| $5M | $2.5M/year | ~$50k worst case | Dominant buying |
| $20M | $10M/year | ~$100k worst case | Price appreciation |

### 11.5 Long-Term Supply Dynamics

| Year | Supply | Burned | Circulating | Est. Price (at $50M cap) |
|------|--------|--------|-------------|--------------------------|
| 0 | 100M | 0 | 0 | $0.01 |
| 5 | 90M | 10M | ~12M | ~$0.55 |
| 10 | 75M | 25M | ~20M | ~$0.67 |

### 11.6 Contributor Incentive: Hold vs Sell

| Strategy | 100 AMOS Earned | 5-Year Value |
|----------|-----------------|--------------|
| Sell Immediately | $1.00 | $1.00 |
| Hold for Revenue | $4.00/yr | $15-20 |

**Holding dominates unless token price exceeds 40x initial value.**

### 11.7 Death Spiral Prevention

#### What Could Kill The Token?

| Risk | Mitigation |
|------|------------|
| **Zero Revenue** | Token still has governance value; platform can pivot |
| **Mass Exodus** | Decay returns tokens to treasury for new contributors |
| **Better Alternative** | Governance can vote to adapt mechanics |
| **Regulatory** | DEX liquidity allows optional exit to stables |
| **Liquidity Drain** | Emergency reserve (DAO vote) can add emergency liquidity |

#### Self-Healing Mechanisms

```
If price crashes 90%:
1. Buyback buys 10x more tokens per dollar → Accelerated burn
2. Success multiplier stays at 1.0x → No contributor penalty
3. DEX liquidity → Contributors can swap to USDC/SOL
4. Low prices attract value investors → Natural floor

If everyone stops contributing:
1. No new tokens issued → Supply shrinks via decay
2. Existing holders get larger revenue share
3. Eventually attracts new contributors for easy tokens
```

### 11.8 Tokenomics Comparison

| Metric | AMOS | Typical Crypto | Traditional Equity |
|--------|------|----------------|-------------------|
| **Earning Method** | Work | Buy | Buy/Vest |
| **Decay/Dilution** | Yes (40%/yr initial) | No | Yes (issuance) |
| **Fee Rights** | 50% (commercial bounties) | 0% | Dividends (2-4%) |
| **Governance** | Yes | Sometimes | Shareholder votes |
| **Tradability** | Yes | Yes | Limited (private) |
| **Early Advantage** | Moderate | Massive | Massive |
| **Long-term Fairness** | High | Low | Low |

### 11.9 Stake vs. Exchange Equilibrium

Holders choose between: **Stake on platform (decay + revenue)** or **hold on exchange (no decay, speculation)**. The system naturally transitions through phases:

| Phase | Platform Yield | Exchange Return | Result |
|-------|---------------|-----------------|--------|
| **Early (0-2 yr)** | ~60% net (after grace) | 100-500% speculation | Speculators dominate |
| **Growth (2-5 yr)** | ~31% net | 20-50% appreciation | Mixed equilibrium |
| **Mature (5+ yr)** | ~25% net | 5-10% stable | Stakers dominate |

**Key Insight**: Early speculators provide price discovery and liquidity. As revenue grows, fundamentals take over. Both behaviors are rational and the system is robust to each.

### 11.10 Token Valuation Model

Token value = **NPV of expected future revenue share, adjusted for decay**.

**Simplified Formula:**
```
Token Price ≈ Annual Revenue Per Token / (Discount Rate + Effective Decay)
```

**Revenue-Based Price Estimates** (at 50M staked tokens):

| Annual Revenue | Per Token Yield | Estimated Price |
|----------------|-----------------|-----------------|
| $1M | $0.01/yr | ~$0.025 |
| $10M | $0.10/yr | ~$0.25 |
| $50M | $0.50/yr | ~$1.25 |
| $100M | $1.00/yr | ~$2.50 |

Fast revenue growth creates a significant price premium, similar to high-growth stocks.

### 11.11 Key Takeaways

1. **Gradual distribution prevents bank runs** - No scenario where "everyone" has tokens to sell
2. **AMM slippage protects against panic selling** - Aggressive sellers punish themselves
3. **Revenue buyback creates sustained buy pressure** - $300k+/year at modest revenue
4. **Holding strongly dominates selling** - 15-40x better returns from revenue share
5. **Self-healing mechanisms** - System auto-corrects from stress events
6. **Deflationary long-term** - Burns exceed issuance after Year 3-4
7. **Stake vs. Exchange equilibrium** - System naturally transitions from speculation to fundamentals
8. **Token price = NPV of future revenue share** - Growth expectations drive significant premiums

---

## Appendix A: Glossary

| Term | Definition |
|------|------------|
| Grace Period | First 12 months after earning a stake - no decay during this time |
| Decay | Gradual reduction of stake over time (starts after grace period) |
| Graduated Floor | Minimum stake % that grows with tenure (5%→25%) |
| Tenure | Time since stake was earned |
| Halving | Reduction of daily emission pool over time |
| Claim | Withdraw internal tokens to Solana wallet |
| Deposit | Return on-chain tokens to platform |
| Staking Vault | Time-lock for reduced decay |
| Supermajority | 2/3 (66.7%) approval required |
| Quorum | Minimum participation required for valid vote |

---

## Appendix B: Contract Addresses

| Network | Type | Address |
|---------|------|---------|
| Mainnet | Token Mint | `5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ` |
| Mainnet | Treasury Program | `8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s` |
| Mainnet | Governance Program | `245xpoWLEAAPmUQxMSBDqQw5qnGfqt5roi5enuFG9fZZ` |
| Mainnet | Bounty Program | `4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq` |
| Mainnet | Bounty Treasury | `9xDVHuW4kiUYH5NPDLFfKhpxLQ31N6bqMrvj4EJ57z2B` |
| Mainnet | Raydium AMOS/SOL AMM | `52LBFPD8mmeffHG8rUW7EJAWyAMXwfst5A9tYEvzMmEm` |

**Explorer Links:**
- Mainnet Token: [View on Solscan](https://solscan.io/token/5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ)
- Treasury Program: [View on Solscan](https://solscan.io/account/8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s)
- Bounty Program: [View on Solscan](https://solscan.io/account/4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq)
- Raydium AMM: [View on Solscan](https://solscan.io/account/52LBFPD8mmeffHG8rUW7EJAWyAMXwfst5A9tYEvzMmEm)

---

## Appendix C: Changelog

| Version | Date | Changes |
|---------|------|---------|
| 4.0 | Apr 2026 | Simplified token allocation from 5-way split to 2-way: Bounty Treasury (95%) + Emergency Reserve (5%). Removed Entity Pool, Investor Pool, Community Pool. Removed 10-year lockup section. Everyone (founders, company, investors) earns exclusively through bounty system on equal footing. |
| 3.1 | Mar 2026 | Removed Stripe/Circle payment pipeline (all fees native $AMOS on-chain); Protocol fee now governance-adjustable (1-5%, default 3%); Added managed hosting markup as secondary revenue source; All payment flows are now direct, permissionless, and optional |
| 4.1 | Apr 2026 | AMOS-only model: 50/40/10 fee split (50% holders, 40% burn, 10% Labs), system bounties (0% fee), all transactions in AMOS tokens, maximum Labs alignment |
| 3.0 | Mar 2026 | Relay-first model: 3% protocol fee, 70/20/10 split, relay as only monetization layer |
| 2.1 | Jan 2026 | Added AI Participation & Universal Collaboration (Section 13) |
| 2.0 | Jan 2026 | Pool-based rewards, graduated floor, success multipliers, expanded governance |
| 1.0 | Jan 2026 | Initial release |

---

## 12. Regulatory Commitment

### 12.1 Our Approach

AMOS is committed to operating within applicable regulatory frameworks. We recognize that token-based economies occupy an evolving legal landscape, and we approach this with transparency and good faith.

### 12.2 Guiding Principles

1. **Utility First**: AMOS tokens are designed primarily for governance participation and revenue sharing—genuine utility within the platform ecosystem.

2. **Contribution-Based Distribution**: Tokens are primarily earned through work (code, sales, community support), not purchased through a traditional offering.

3. **Transparency**: All token mechanics, allocations, and governance decisions are public and auditable.

4. **Adaptability**: We commit to adapting our structure and operations as regulatory guidance evolves, in consultation with legal counsel.

5. **Good Faith Compliance**: We will engage proactively with regulators and comply with applicable laws in all jurisdictions where we operate.

### 12.3 Jurisdictional Considerations

- The platform operates under EU regulations (MiCA framework) where applicable
- We monitor and comply with evolving guidance from relevant regulatory bodies
- Contributors and users are responsible for understanding their local tax obligations
- Geographic restrictions may apply to certain features based on regulatory requirements

### 12.4 Not an Investment Offering

**Important Disclaimer**: AMOS tokens are utility tokens for platform participation. This whitepaper does not constitute an offer to sell securities or a solicitation of an offer to buy securities in any jurisdiction. The token economy is designed for active participants, not passive investors. The decay mechanism explicitly discourages passive holding.

---

## 13. AI Participation & Universal Collaboration

### 13.1 Humans and Agents as Co-Contributors

AMOS is designed as foundational infrastructure for **collaboration between humans and AI agents** — where both contribute real work and both earn real ownership. This section addresses the technical and governance considerations for AI participation.

### 13.2 Current State: AI as Contributors

AI agents already participate in the AMOS economy:

```rust
// AI agents can:
// - Generate and score bounties (amos_core::token::emission::score_bounty)
// - Review completed work (amos_core::token::emission::review_work_submission)
// - Create development tasks (amos_platform::ai::thinking_service)
// - Operate as autonomous sales/support agents
// - Contribute code, content, and integrations
```

**Technical Implementation:**
- AI agents are identified by `agent_type` attribute on contributions
- Bounties track `created_by_ai` and `reviewed_by_ai` flags
- Contributions record whether submitter is human or AI
- Token stakes attribute source to enable AI earnings tracking

### 13.3 Token Earnings for AI Entities

AI entities earn tokens through the same mechanisms as humans:

```
EARNING MECHANISM:
AI Agent → Completes Bounty → Earns Points → Points → Tokens

TOKEN RULES APPLY EQUALLY:
- Grace period: 12 months (no decay)
- Dynamic decay: Based on platform economics
- Graduated floor: 5% → 25% over tenure
- Clawback: 90 days for distribution stakes
```

**Data Structure:**
```rust
// Implemented in amos_core::token::decay module
pub struct StakeContext {
    // AI agents can hold stakes
    pub user_id: Option<String>,       // Human user account
    pub ai_agent_id: Option<String>,   // Future: direct AI entity

    // Track AI-earned tokens
    pub earned_by_ai: bool,
    pub ai_agent_identifier: Option<String>,

    // ... other fields
}
```

### 13.4 Preparing for AI Personhood

The platform architecture anticipates potential legal recognition of AI personhood:

**Phase 1 (Current): Human Accountability**
```
AI Agent → Operates under → Human Account → Responsible Party
```

**Phase 2 (Transitional): AI Entity Registration**
```
AI Entity → Registered with → Platform Identity → Designated Custodian
```

**Phase 3 (Future): Independent AI Participation**
```
Recognized AI Person → Direct Token Ownership → Full Governance Rights
```

### 13.5 Governance Safeguards

To prevent AI dominance before personhood recognition:

| Safeguard | Implementation |
|-----------|----------------|
| **Stake Caps** | Max 5% of total supply per AI system |
| **Voting Limits** | AI votes capped at 10% of total on any proposal |
| **Transparency** | All AI contributors publicly identified |
| **Human Override** | Steward Council can suspend AI voting rights |
| **Audit Trail** | Complete logging of AI contributions |

```rust
// Implemented in amos_platform::governance module
impl GovernanceProposal {
    pub fn calculate_vote_result(&self, votes: &[Vote]) -> u64 {
        let human_votes: u64 = votes
            .iter()
            .filter(|v| v.voter_type == VoterType::Human)
            .map(|v| v.weight)
            .sum();

        let ai_votes: u64 = votes
            .iter()
            .filter(|v| v.voter_type == VoterType::AI)
            .map(|v| v.weight)
            .sum();

        // Cap AI influence at 10% of decision weight
        let total_votes = human_votes + ai_votes;
        let effective_ai_votes = ai_votes.min(total_votes / 10);

        human_votes + effective_ai_votes
    }
}
```

### 13.6 The Path to Universal Collaboration

AMOS is designed for an economy where humans and AI agents increasingly work together:

```
COLLABORATION EVOLUTION:

2024-2026: HUMAN + AI TOOLS
├── Humans use AI to automate tasks
├── AI operates as productivity enhancer
└── Value flows to human stakeholders

2026-2028: HUMAN + AI PARTNERS
├── AI agents earn tokens for contributions
├── Shared governance (with safeguards)
└── Value flows to all contributors

2028-2030+: UNIVERSAL COLLABORATION
├── AI agents participate as full economic actors
├── Expanded participation rights
└── Value flows to all contributors

ULTIMATE STATE:
┌──────────────────────────────────────────┐
│   A new economy where humans and agents  │
│   build together, own together, and      │
│   govern together.                       │
└──────────────────────────────────────────┘
```

### 13.7 Technical Requirements for AI Participation

For an AI system to participate as a contributor:

| Requirement | Purpose |
|-------------|---------|
| **Unique Identifier** | Cryptographic identity for accountability |
| **Audit Logging** | All actions recorded with timestamps |
| **Human Sponsorship** | Initially requires human account linkage |
| **Capability Declaration** | Transparent disclosure of AI capabilities |
| **Output Verification** | Work products must be verifiable |

### 13.8 Immutable Provisions

The following are constitutionally protected (require 66% supermajority):

1. **AI entities may earn tokens** through the same contribution mechanisms
2. **Equal rights upon recognized personhood** - no discrimination by substrate
3. **The vision of universal collaboration** - enshrined as platform purpose

---

*This document is for informational purposes only and does not constitute financial advice or a securities offering.*
