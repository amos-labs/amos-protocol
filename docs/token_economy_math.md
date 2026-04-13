# AMOS Token Economy: Complete Mathematical Framework

> **Purpose**: This document provides the complete mathematical specification for the AMOS token economy, tying together real-world costs, revenue, and token mechanics into a bulletproof, self-balancing system.

---

## 1. The Big Picture: Three Interconnected Economies

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         REAL-WORLD ECONOMY (USD)                             │
│                                                                              │
│  ┌──────────────────────┐         ┌──────────────────────┐                  │
│  │     AWS COSTS        │         │      REVENUE         │                  │
│  │  ─────────────────   │         │  ─────────────────   │                  │
│  │  • Bedrock (AI)      │         │  • Subscriptions     │                  │
│  │  • SES (Email)       │         │  • Compute Sales     │                  │
│  │  • ECS (Compute)     │         │  • Token Purchases   │                  │
│  │  • S3 (Storage)      │         │  • Enterprise Deals  │                  │
│  │  • Other Services    │         │                      │                  │
│  └──────────────────────┘         └──────────────────────┘                  │
│              │                              │                                │
│              ▼                              ▼                                │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                    PROFIT/LOSS RATIO (π)                             │   │
│  │                                                                      │   │
│  │          π = (Monthly Revenue - Monthly Costs) / Monthly Costs       │   │
│  │                                                                      │   │
│  │    π > 0  →  Profitable (lower decay)                               │   │
│  │    π = 0  →  Break-even (base decay)                                │   │
│  │    π < 0  →  Unprofitable (higher decay)                            │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                        AMOS NETWORK RELAY PROTOCOL                          │
│                                                                              │
│  The Relay facilitates bounty payouts and collects protocol fees            │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Protocol Fee = Bounty Payout × 0.03 (3% fee)                       │    │
│  │                                                                     │    │
│  │  Fee Distribution:                                                  │    │
│  │  • 50% → Staked token holders (commercial bounties only)            │    │
│  │  • 40% → Permanently burned (deflationary)                          │    │
│  │  • 10% → AMOS Labs (operations, development)                        │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  This creates the bridge: Bounty Payouts → Protocol Fees → Token Value     │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         AMOS TOKEN ECONOMY                                   │
│                                                                              │
│  AMOS tokens represent ownership of future relay protocol fees               │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Key Dynamics:                                                      │    │
│  │  • Emission: Pool-based daily distribution (16K/day, halving)       │    │
│  │  • Decay: Dynamic rate tied to platform economics (2-25%)           │    │
│  │  • Fee Share: 50% of relay protocol fees to staked token holders    │    │
│  │  • Fixed Supply: 100M tokens ever, deflationary                     │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Real-World Cost Tracking (C_total)

### 2.1 AWS Cost Categories

The platform tracks costs across multiple AWS service categories:

```
C_total = C_ai + C_email + C_compute + C_storage + C_network + C_other
```

Where each component is calculated as:

#### 2.1.1 AI/LLM Costs (C_ai)

```
C_ai = Σ (input_tokens × input_rate[model] + output_tokens × output_rate[model])
```

| Model | Input Rate (per 1K) | Output Rate (per 1K) |
|-------|---------------------|----------------------|
| Qwen3-Next-80B | $0.00020 | $0.00080 |
| Claude 3.5 Haiku | $0.00025 | $0.00125 |
| Claude 3.5 Sonnet | $0.003 | $0.015 |
| Claude 3 Opus | $0.015 | $0.075 |
| Claude 4.5 Haiku | $0.00020 | $0.00100 |
| DeepSeek R1 | $0.00135 | $0.00540 |

**Example**: 100K input + 50K output on Claude 3.5 Sonnet:
```
C_ai = (100 × $0.003) + (50 × $0.015) = $0.30 + $0.75 = $1.05
```

#### 2.1.2 Email Costs (C_email)

```
C_email = (emails_sent × $0.00009) + (attachment_gb × $0.10)
```

**Example**: 10,000 emails with 0.5GB attachments:
```
C_email = (10,000 × $0.00009) + (0.5 × $0.10) = $0.90 + $0.05 = $0.95
```

#### 2.1.3 Compute Costs (C_compute)

```
C_compute = (lambda_invocations × $0.00000018) + 
            (lambda_gb_seconds × $0.0000133334) +
            (ecs_vcpu_hours × $0.03643) + 
            (ecs_gb_hours × $0.004001)
```

#### 2.1.4 Storage Costs (C_storage)

```
C_storage = (s3_standard_gb × $0.021/month) + 
            (s3_intelligent_gb × $0.0115/month) +
            (rds_storage_gb × $0.103/month)
```

#### 2.1.5 Document Processing (C_doc)

```
C_doc = (textract_pages × $0.0012) + 
        (textract_tables_pages × $0.013) +
        (comprehend_100chars × $0.00008)
```

### 2.2 Total Monthly Platform Costs

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     MONTHLY COST EQUATION                               │
│                                                                         │
│  C_monthly = Σ(C_ai) + Σ(C_email) + Σ(C_compute) +                      │
│              Σ(C_storage) + Σ(C_network) + C_fixed                      │
│                                                                         │
│  Where C_fixed = infrastructure + personnel + third-party services      │
└─────────────────────────────────────────────────────────────────────────┘
```

**Current Tracking Implementation**: 
- `EntityCostTracker` tracks per-entity costs
- `Aws::CostCalculator` calculates specific AWS service costs
- `PlatformCost` model records aggregate platform-level costs
- `WorkTokenUsageSummary` aggregates daily token usage by category

---

## 3. Revenue Model (R_total)

### 3.1 Revenue Sources

```
R_total = R_compute + R_enterprise + R_other
```
#### 3.1.2 Protocol Fee Revenue (3% on Bounty Payouts)

```
R_protocol = C_bounty_payouts × (1 + PROTOCOL_FEE)

Where PROTOCOL_FEE = 0.03 (3%)
```

This is the key link between bounty payouts and revenue:
- **The AMOS Network Relay charges a 3% protocol fee on all bounty payouts**
- This fee is the ONLY monetized layer in the system
- All protocol fees go to staked token holders, treasury, and operations

**Example**: $100 in bounty payouts → $3 protocol fee collected by relay

### 3.2 Monthly Revenue Calculation

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    MONTHLY REVENUE EQUATION                             │
│                                                                         │
│  R_monthly = + Σ(PaymentTransactions.completed.amount_cents) / 100        │
│              + Σ(ComputeUsage.billed.cost_cents) / 100                    │
└─────────────────────────────────────────────────────────────────────────┘
```

**Current Implementation**: `PlatformEconomicsService.calculate_monthly_revenue`

---

## 4. Profit/Loss Ratio (π) - The Bridge to Token Economy

This is the **critical equation** that connects real-world economics to token decay:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     PROFIT/LOSS RATIO                                   │
│                                                                         │
│                R_monthly - C_monthly                                    │
│        π  =  ─────────────────────────                                  │
│                    C_monthly                                            │
│                                                                         │
│  Interpretation:                                                        │
│  • π =  0.20  →  20% profit margin (highly profitable)                  │
│  • π =  0.10  →  10% profit margin (healthy)                            │
│  • π =  0.00  →  Break-even                                             │
│  • π = -0.10  →  10% loss (burning runway)                              │
│  • π = -0.20  →  20% loss (significant burn)                            │
└─────────────────────────────────────────────────────────────────────────┘
```

**Example Calculation**:
- Monthly Revenue: $50,000
- Monthly Costs: $40,000
- π = ($50,000 - $40,000) / $40,000 = $10,000 / $40,000 = **0.25 (25% profit)**

---

## 5. Dynamic Decay Rate (δ) - Token Decay Tied to Economics

The decay rate adjusts automatically based on the profit/loss ratio:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     DECAY RATE EQUATION                                 │
│                                                                         │
│        δ_base  =  δ_equilibrium - (π × σ)                               │
│                                                                         │
│        δ_final =  max(δ_min, min(δ_max, δ_base))                        │
│                                                                         │
│  Where:                                                                 │
│  • δ_equilibrium = 0.10 (10% annual at break-even)                      │
│  • σ = 0.05 (sensitivity factor)                                        │
│  • δ_min = 0.02 (2% annual minimum)                                     │
│  • δ_max = 0.25 (25% annual maximum)                                    │
└─────────────────────────────────────────────────────────────────────────┘
```

### 5.1 Decay Rate Examples

| Scenario | π (profit ratio) | δ_base | δ_final | Meaning |
|----------|------------------|--------|---------|---------|
| Highly profitable | +0.25 | 0.10 - (0.25 × 0.05) = 0.0875 | **8.75%** | Low decay, rewards holders |
| Profitable | +0.20 | 0.10 - (0.20 × 0.05) = 0.09 | **9%** | Healthy decay |
| Break-even | 0.00 | 0.10 - (0.00 × 0.05) = 0.10 | **10%** | Base decay rate |
| Slight loss | -0.10 | 0.10 - (-0.10 × 0.05) = 0.105 | **10.5%** | Slightly elevated |
| Moderate loss | -0.50 | 0.10 - (-0.50 × 0.05) = 0.125 | **12.5%** | Elevated decay |
| Severe loss | -2.00 | 0.10 - (-2.00 × 0.05) = 0.20 | **20%** | High decay (capped) |
| Critical loss | -5.00 | 0.10 - (-5.00 × 0.05) = 0.35 | **25%** | Maximum decay (capped) |

### 5.2 Why This Works

The dynamic decay creates **organic equilibrium**:

1. **When profitable**: Low decay → tokens hold value → contributors incentivized
2. **When unprofitable**: High decay → tokens recycle to treasury → fund operations
3. **Self-correcting**: If too many tokens issued, costs rise, decay increases

```
                          FEEDBACK LOOP
    
    Platform profitable                    Platform unprofitable
           │                                       │
           ▼                                       ▼
    Lower decay rate ←──────────────────→ Higher decay rate
           │                                       │
           ▼                                       ▼
    Tokens hold value                     Tokens recycle to treasury
           │                                       │
           ▼                                       ▼
    More contributors attracted           Treasury funds operations
           │                                       │
           ▼                                       ▼
    Platform grows                        Platform stabilizes
           │                                       │
           └───────────────────────────────────────┘
                    ORGANIC EQUILIBRIUM
```

---

## 6. Token Emission Model (E_daily)

### 6.1 Daily Emission Pool

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     DAILY EMISSION EQUATION                             │
│                                                                         │
│        E_daily = E_base × H(t)                                          │
│                                                                         │
│  Where:                                                                 │
│  • E_base = 16,000 AMOS tokens                                          │
│  • H(t) = Halving multiplier based on platform age                      │
└─────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Halving Schedule

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     HALVING MULTIPLIER H(t)                             │
│                                                                         │
│  Year 0-1:   H = 1.00    →  16,000 tokens/day                           │
│  Year 1-2:   H = 0.50    →   8,000 tokens/day                           │
│  Year 2-3:   H = 0.25    →   4,000 tokens/day                           │
│  Year 3-4:   H = 0.125   →   2,000 tokens/day                           │
│  Year 4-5:   H = 0.0625  →   1,000 tokens/day                           │
│  Year 5-6:   H = 0.03125 →     500 tokens/day                           │
│  Year 6+:    Floor       →     100 tokens/day (minimum)                 │
└─────────────────────────────────────────────────────────────────────────┘
```

### 6.3 Sigmoid Pool Separation (Technical vs Growth)

The daily emission pool is dynamically split between technical and growth contributions using a sigmoid function:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    SIGMOID GROWTH CAP FORMULA                           │
│                                                                         │
│  growth_cap(t) = floor + (ceiling - floor) / (1 + e^(k × (t - m)))    │
│                                                                         │
│  Where:                                                                 │
│  • ceiling = 2000 BPS (20% maximum cap)                                 │
│  • floor = 300 BPS (3% minimum cap)                                     │
│  • midpoint (m) = 540 days                                              │
│  • k = 0.01 (k_scaled = 100 for integer arithmetic)                     │
│                                                                         │
│  The pool is then split as:                                             │
│  • Technical pool = E_daily - growth_pool                               │
│  • Growth pool = min(growth_cap(t), natural_weighted_share)             │
│  • Unused growth allocation rolls into technical pool                   │
└─────────────────────────────────────────────────────────────────────────┘
```

**Example Trajectory:**
```
Day 0:      growth_cap ≈ 2000 BPS (20%)
Day 270:    growth_cap ≈ 1889 BPS (18.9%)
Day 540:    growth_cap ≈ 1150 BPS (11.5%)
Day 900:    growth_cap ≈ 350 BPS (3.5%)
Day 1260:   growth_cap ≈ 300 BPS (3.0% floor)
```

This sigmoid function gradually transitions growth pool allocation from a 20% cap at launch to the 3% floor by year 3.6, ensuring growth initiatives are incentivized early while technical work dominates long-term.

### 6.4 Total Supply Calculation

```
Total Supply = 100,000,000 AMOS (fixed)

Distribution:
• Bounty Treasury (95%): 95,000,000 AMOS  →  Contributor rewards via daily emissions
• Emergency Reserve (5%): 5,000,000 AMOS  →  DAO-locked, governance vote required
• Founders:                       0 AMOS  →  Start at zero, earn like everyone
```

---

## 7. Contribution Points System (P)

### 7.1 Points Accumulation

All contributions are measured in **points**, which determine your share of the daily emission pool.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     YOUR TOKEN REWARD                                   │
│                                                                         │
│                         P_you                                           │
│        T_you  =  ─────────────────  ×  E_daily                          │
│                       P_total                                           │
│                                                                         │
│  Where:                                                                 │
│  • P_you = Your points earned today                                     │
│  • P_total = Total points earned by everyone today                      │
│  • E_daily = Daily emission pool                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

### 7.2 Points by Activity Type

#### Contribution Types (11 Total: 8 Technical + 3 Growth)

**Technical Contributions:**
- Bounties/Code, Referrals, Sales, and 5 other technical activity types

**Growth Contributions:**
| Type | BPS Value | Pool |
|------|-----------|------|
| bug_report | 10,000 BPS (100%) | Growth |
| referral | 6,000 BPS (60%) | Growth |
| signup | 4,000 BPS (40%) | Growth |

#### Referrals

| Action | Points |
|--------|--------|
| Email sent | 1 |
| Signup (free) | 5 |
| Conversion (paid) | 10 |
| Active month (ongoing) | 2/month |

```
P_referral = (emails × 1) + (signups × 5) + (conversions × 10) + (active_months × 2)
```

**Example**: Send 10 emails, get 2 signups, 1 converts, stays 3 months:
```
P_referral = (10 × 1) + (2 × 5) + (1 × 10) + (3 × 2) = 10 + 10 + 10 + 6 = 36 points
```

#### Sales

```
P_sales = users_signed_up × 1
```

#### Bounties/Code

```
P_bounty = bounty_value_in_amos
```

### 7.3 Token Calculation Example

```
Scenario:
• You earned 100 points today
• Total platform points today: 5,000
• Daily emission: 16,000 AMOS

Your tokens:
T_you = (100 / 5,000) × 16,000 = 0.02 × 16,000 = 320 AMOS
```

---

## 8. Stake Decay Mechanics

### 8.1 Effective Decay Rate for a Stake

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     EFFECTIVE DECAY RATE                                │
│                                                                         │
│        δ_effective = δ_platform × (1 - r_tenure) × (1 - r_vault)        │
│                                                                         │
│  Where:                                                                 │
│  • δ_platform = Dynamic decay from platform economics (2-25%)           │
│  • r_tenure = Tenure-based reduction (0-70%)                            │
│  • r_vault = Staking vault reduction (0-100%)                           │
└─────────────────────────────────────────────────────────────────────────┘
```

### 8.2 Tenure-Based Decay Reduction (r_tenure)

| Tenure | Reduction | Effective Rate (if base = 10%) |
|--------|-----------|--------------------------------|
| Year 0-1 | 0% | 10% |
| Year 1-2 | 20% | 8% |
| Year 2-5 | 40% | 6% |
| Year 5+ | 70% | 3% |

### 8.3 Staking Vault Reduction (r_vault)

| Tier | Lock Period | Reduction | Effective Rate (if base = 10%) |
|------|-------------|-----------|--------------------------------|
| None | 0 | 0% | 10% |
| Bronze | 30 days | 20% | 8% |
| Silver | 90 days | 50% | 5% |
| Gold | 365 days | 80% | 2% |
| Permanent | No unlock | 95% | 0.5% |

### 8.4 Decay Floor (Never Decays Below)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     GRADUATED DECAY FLOOR                               │
│                                                                         │
│  Year 0-1:   Floor = 5% of initial stake                                │
│  Year 1-2:   Floor = 10% of initial stake                               │
│  Year 2-5:   Floor = 15% of initial stake                               │
│  Year 5+:    Floor = 25% of initial stake                               │
│                                                                         │
│  This means early adopters MUST stay engaged; loyalty builds security   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 8.5 Daily Decay Calculation

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     DAILY STAKE UPDATE                                  │
│                                                                         │
│  V_floor = V_initial × floor_rate                                       │
│  V_decayable = V_current - V_floor                                      │
│                                                                         │
│  δ_daily = 1 - (1 - δ_effective)^(1/365)                                │
│                                                                         │
│  V_new = V_floor + (V_decayable × (1 - δ_daily))                        │
│                                                                         │
│  Special rules:                                                         │
│  • If within 12-month grace period: V_new = V_current (no decay)        │
│  • If V_current <= V_floor: V_new = V_current (at floor)                │
└─────────────────────────────────────────────────────────────────────────┘
```

**Example**: 10,000 AMOS stake, Year 2, 10% effective annual decay, 10% floor
```
V_floor = 10,000 × 0.10 = 1,000 AMOS (protected)
V_decayable = 10,000 - 1,000 = 9,000 AMOS (subject to decay)
δ_daily = 1 - (1 - 0.10)^(1/365) = 0.000289 (0.0289%/day)

After 1 day:
V_new = 1,000 + (9,000 × (1 - 0.000289)) = 1,000 + 8,997.40 = 9,997.40 AMOS

After 1 year:
V_new = 1,000 + (9,000 × 0.90) = 1,000 + 8,100 = 9,100 AMOS
```

---

## 9. Revenue Distribution to Token Holders

### 9.1 Revenue Allocation

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     PROTOCOL FEE ALLOCATION                             │
│                                                                         │
│  R_holders   = R_total × 0.50  (50% to staked token holders)            │
│  R_burn      = R_total × 0.40  (40% to permanent burn)                  │
│  R_labs      = R_total × 0.10  (10% to AMOS Labs)                       │
└─────────────────────────────────────────────────────────────────────────┘
```

### 9.2 Individual Holder Payout

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     YOUR REVENUE SHARE                                  │
│                                                                         │
│                      S_you                                              │
│        Payout_you = ──────── × R_holders                                │
│                     S_total                                             │
│                                                                         │
│  Where:                                                                 │
│  • S_you = Your current AMOS stake                                      │
│  • S_total = Total AMOS staked on platform                              │
│  • R_holders = 50% of relay protocol fees                               │
└─────────────────────────────────────────────────────────────────────────┘
```

**Example**: You hold 50,000 AMOS, total staked is 10,000,000 AMOS, monthly revenue is $100,000
```
R_holders = $100,000 × 0.50 = $50,000

Your share:
Payout_you = (50,000 / 10,000,000) × $50,000 = 0.005 × $50,000 = $250/month
```

---

## 10. AMOS Network Relay Protocol

### 10.1 Protocol Fee Calculation

The AMOS Network Relay is the ONLY monetized layer in the system:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     PROTOCOL FEE FORMULA                                │
│                                                                         │
│  Protocol_Fee = Bounty_Payout × 0.03                                    │
│                                                                         │
│  Where:                                                                 │
│  • Bounty_Payout = Total amount paid to bounty recipients               │
│  • 0.03 = 3% protocol fee                                               │
└─────────────────────────────────────────────────────────────────────────┘
```

### 10.2 Fee Distribution

```
Protocol Fee Collection → Distribution to Stakeholders

50% → Staked Token Holders (proportional to stake)
40% → Permanently Burned (deflationary mechanism)
10% → AMOS Labs (operations, development)
```

### 10.3 Revenue Flow

```
Bounty Payout
       │
       ▼
┌──────────────────┐
│  Relay Processes │
│  Payment         │◄──── Bounty payout via AMOS Network Relay
└──────────────────┘
       │
       ▼
┌──────────────────┐
│  Protocol Fee    │◄──── 3% fee collected by relay
│  (3%)            │
└──────────────────┘
       │
       ▼
┌──────────────────┐
│  Fee Split       │◄──── 50/40/10 distribution
└──────────────────┘
       │
       ▼
┌──────────────────┐
│  Token Economy   │◄──── Fees feed platform profit ratio (π) → decay rate (δ)
└──────────────────┘
```

---

## 11. Complete System Flow

```
                            COMPLETE TOKEN ECONOMY FLOW

┌─────────────────────────────────────────────────────────────────────────────┐
│  1. REAL COSTS OCCUR                                                        │
│     • User runs AI query → Bedrock charges $0.05                            │
│     • User sends email → SES charges $0.0001                                │
│     • Tracked by EntityCostTracker                                          │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  2. BOUNTIES COMPLETED                                                      │
│     • Agent completes bounty work                                           │
│     • Bounty validated and approved                                         │
│     • Payout processed via AMOS Network Relay                               │
│     • Tracked by BountyService                                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  3. PROTOCOL FEE CHARGED                                                    │
│     • Bounty payout: $100.00                                                │
│     • With 3% protocol fee: $3.00                                           │
│     • Tracked by AMOS Network Relay                                         │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  4. MONTHLY ECONOMICS CALCULATED                                            │
│     • R_monthly = $50,000                                                   │
│     • C_monthly = $40,000                                                   │
│     • π = ($50K - $40K) / $40K = 0.25                                       │
│     • Calculated by PlatformEconomicsService                                │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  5. DECAY RATE DETERMINED                                                   │
│     • δ_base = 0.10 - (0.25 × 0.05) = 0.0875 (8.75%)                        │
│     • Below minimum? No, use 8.75%                                          │
│     • Applied by PlatformEconomicsService.current_decay_rate                │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  6. TOKENS EMITTED (DAILY)                                                  │
│     • Pool: 16,000 AMOS                                                     │
│     • You earned 100 points, total points: 5,000                            │
│     • Your share: (100/5000) × 16,000 = 320 AMOS                            │
│     • Calculated by ContributionRewardCalculator                            │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  7. STAKE UPDATED (DAILY)                                                   │
│     • Your stake: 10,000 AMOS → 9,997 AMOS (after decay)                    │
│     • New tokens: +320 AMOS                                                 │
│     • Net: 10,317 AMOS (growth if active!)                                  │
│     • Applied by TokenStake.apply_decay!                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  8. PROTOCOL FEES DISTRIBUTED (MONTHLY)                                     │
│     • Your stake: 50,000 AMOS (0.5% of total)                               │
│     • Holder pool: $50,000 × 50% = $25,000                                  │
│     • Your payout: 0.5% × $25,000 = $125 AMOS                               │
│     • Managed by RevenueDistribution                                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 12. Key Formulas Summary

### Cost Tracking
```
C_ai = Σ(input × input_rate + output × output_rate)
C_total = C_ai + C_email + C_compute + C_storage + C_other
```

### Protocol Fees
```
R_protocol = C_bounty_payouts × 0.03 (3% protocol fee)
R_total = R_protocol (relay is the ONLY monetized layer)
```

### Profit Ratio
```
π = (R_monthly - C_monthly) / C_monthly
```

### Dynamic Decay
```
δ = max(0.02, min(0.25, 0.10 - π × 0.05))
```

### Token Emission
```
E_daily = 16,000 × H(year)  where H = halving multiplier
```

### Token Reward
```
T_you = (P_you / P_total) × E_daily
```

### Stake Value
```
V_tomorrow = V_floor + (V_decayable × (1 - δ_daily))
```

### Revenue Share
```
Payout_you = (S_you / S_total) × R_holders
Where R_holders = 50% of protocol fees
```

---

## 13. Operational Parameters

### 13.1 Claim Mechanics

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      CLAIM TIMEOUT CONFIGURATION                        │
│                                                                         │
│  DEFAULT_CLAIM_TIMEOUT_HOURS = 72                                       │
│  Valid Range:                   1 - 720 hours                           │
│                                                                         │
│  When a contributor claims a completed bounty, the platform grants      │
│  72 hours for dispute resolution before the claim is finalized.         │
└─────────────────────────────────────────────────────────────────────────┘
```

```
┌─────────────────────────────────────────────────────────────────────────┐
│               MAX CONCURRENT CLAIMS BY TRUST LEVEL                       │
│                                                                         │
│  Trust Level 1:  3 concurrent claims                                    │
│  Trust Level 2:  5 concurrent claims                                    │
│  Trust Level 3:  8 concurrent claims                                    │
│  Trust Level 4:  12 concurrent claims                                   │
│  Trust Level 5:  20 concurrent claims                                   │
│                                                                         │
│  Higher trust levels can work on more bounties simultaneously.          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 13.2 Dispute Parameters

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        DISPUTE MECHANICS                                │
│                                                                         │
│  DISPUTE_WINDOW_HOURS = 48                                              │
│    Window after claim is filed during which disputes can be raised      │
│                                                                         │
│  DISPUTE_STAKE_BPS = 500 (5% of bounty value)                           │
│    Stake required to file a dispute; returned if dispute wins           │
│                                                                         │
│  DISPUTE_RESOLUTION_TIMEOUT_HOURS = 168 (7 days)                        │
│    Maximum time for dispute resolution before auto-resolution           │
└─────────────────────────────────────────────────────────────────────────┘
```

### 13.3 Registry Parameters

```
┌─────────────────────────────────────────────────────────────────────────┐
│                   REGISTRY LIFECYCLE MANAGEMENT                          │
│                                                                         │
│  REGISTRY_AUTO_FREEZE_SECONDS = 94,608,000 (3 years)                    │
│    After 3 years of inactivity, a registry entry auto-freezes           │
│    Frozen registries no longer receive emissions or rewards             │
│                                                                         │
│  REGISTRY_MAX_EXTENSIONS = 2                                            │
│    Maximum number of times a registry can be extended before expiry     │
│                                                                         │
│  REGISTRY_EXTENSION_DURATION_SECONDS = 31,536,000 (1 year)              │
│    Each extension adds 1 year to the registry lifetime                  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 14. Gaps & Future Work

### 14.1 Current Tracking Gaps

| Category | Status | Implementation |
|----------|--------|----------------|
| AI Token Costs | ✅ Tracked | `AiUsageLog`, `WorkTokenService` |
| Email Costs | ✅ Tracked | `EntityCostTracker.track_email_sent` |
| Textract/Comprehend | ✅ Tracked | `Aws::CostCalculator` |
| ECS/Lambda | ⚠️ Partial | Need real-time tracking |
| S3 Storage | ⚠️ Estimated | Need AWS Cost Explorer API |
| RDS | ⚠️ Fixed estimate | Need AWS Cost Explorer API |
| Third-party APIs | ⚠️ Partial | `EntityCostTracker.track_integration_api_call` |
| Personnel | ❌ Manual | `PlatformCost` records |

### 14.2 Recommended Improvements

1. **AWS Cost Explorer Integration**: Pull real costs daily via API
2. **Real-time ECS Tracking**: Use Container Insights metrics
3. **Budget Alerts**: Auto-detect cost spikes
4. **Cost Attribution**: Tag AWS resources by entity/user

### 14.3 Implementation Checklist

- [ ] Add AWS Cost Explorer API integration
- [ ] Create daily cost sync job
- [ ] Add ECS Fargate tracking
- [ ] Build cost dashboard for admins
- [ ] Implement cost anomaly detection
- [ ] Add real-time profit margin display

---

## 15. Appendix: Code References

### Key Services (now Rust modules)

| Module | Purpose | Location |
|--------|---------|----------|
| Token Economics | Constants & core types | `amos-core/src/token/economics.rs` |
| Decay Engine | Dynamic decay, profit calc | `amos-core/src/token/decay.rs` |
| Emission Engine | Points → tokens, halving | `amos-core/src/token/emission.rs` |
| Revenue Distribution | $AMOS distribution, claims | `amos-core/src/token/revenue.rs` |
| Trust System | Agent trust levels 1-5 | `amos-core/src/token/trust.rs` |
| Treasury Program | On-chain revenue & staking | `amos-solana/programs/amos-treasury/` |
| Bounty Program | On-chain bounty distribution | `amos-solana/programs/amos-bounty/` |
| Governance Program | On-chain proposals & voting | `amos-solana/programs/amos-governance/` |
| Billing Module | Plans, subscriptions, usage | `amos-platform/src/billing/mod.rs` |
| Token API | HTTP endpoints for token data | `amos-platform/src/routes/token.rs` |

### Key Data Types

| Type | Purpose | Location |
|------|---------|----------|
| `PlatformEconomics` | Revenue/cost snapshot | `amos-core/src/token/decay.rs` |
| `StakeContext` | Per-stake decay input | `amos-core/src/token/decay.rs` |
| `DecayResult` | Decay calculation output | `amos-core/src/token/decay.rs` |
| `DailyEmission` | Daily emission pool | `amos-core/src/token/emission.rs` |
| `BountyAward` | Bounty token award | `amos-core/src/token/emission.rs` |
| `AgentTrust` | Agent trust record | `amos-core/src/token/trust.rs` |
| `TreasuryState` | On-chain treasury state | `amos-platform/src/solana/mod.rs` |
| `Customer` / `Subscription` | Billing records | `amos-platform/src/billing/mod.rs` |

---

*Last Updated: April 10, 2026*
*Version: 2.1.0*
