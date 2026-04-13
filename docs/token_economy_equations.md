# AMOS Token Economy: Equation Cheat Sheet

> Quick reference for all the math behind the token economy

---

## 🎯 THE ONE EQUATION THAT RULES THEM ALL

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│         DECAY RATE = 10% - (PROFIT RATIO × 5%)                              │
│                                                                             │
│         Clamped between 2% (min) and 25% (max)                              │
│                                                                             │
│         Where: PROFIT RATIO = (Revenue - Costs) / Costs                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**In plain English:**
- If we're 20% profitable → Decay = 10% - (0.20 × 5%) = **9%** (low decay)
- If we're break-even → Decay = 10% - (0 × 5%) = **10%** (base decay)
- If we're 20% unprofitable → Decay = 10% - (-0.20 × 5%) = **11%** (higher decay)

---

## 💰 COST EQUATIONS

### AWS Costs (What We Pay)

```
AI Cost = (Input Tokens ÷ 1000 × Input Rate) + (Output Tokens ÷ 1000 × Output Rate)
```

| Model | Input Rate | Output Rate |
|-------|------------|-------------|
| Qwen3-Next-80B | $0.20/1M | $0.80/1M |
| Claude 3.5 Haiku | $0.25/1M | $1.25/1M |
| Claude 3.5 Sonnet | $3.00/1M | $15.00/1M |

**Example:**
```
10K input + 5K output on Sonnet:
= (10 × $0.003) + (5 × $0.015)
= $0.03 + $0.075
= $0.105
```

### Total Monthly Costs

```
C_monthly = C_ai + C_email + C_compute + C_storage + C_other + C_fixed

Where:
  C_ai      = Sum of all AI/LLM usage
  C_email   = $0.09 per 1000 emails
  C_compute = ECS/Lambda costs  
  C_storage = S3/RDS costs
  C_other   = Textract, Comprehend, etc.
  C_fixed   = Infrastructure, personnel
```

---

## 💵 REVENUE EQUATIONS

### Protocol Fee Collection

```
Protocol Fee = Bounty Payout × 0.03  (3% protocol fee)
```

**Example:**
```
$100 bounty payout → $3 protocol fee collected by AMOS Network Relay
```

### Monthly Protocol Fees

```
R_monthly = R_protocol

Where:
  R_protocol = Bounty payout protocol fees (3% fee)

Note: The AMOS Network Relay is the ONLY monetized layer
```

---

## 📊 PROFIT RATIO (The Bridge)

```
              Revenue - Costs
Profit (π) = ─────────────────
                  Costs
```

| Scenario | Revenue | Costs | π |
|----------|---------|-------|---|
| Highly profitable | $60K | $40K | +0.50 |
| Healthy | $50K | $45K | +0.11 |
| Break-even | $50K | $50K | 0.00 |
| Slight loss | $45K | $50K | -0.10 |
| Significant loss | $30K | $50K | -0.40 |

---

## 🔄 DECAY EQUATIONS

### Base Decay Rate (from Platform Economics)

```
δ_base = 10% - (π × 5%)

Examples:
  π = +0.40 → δ = 10% - 2%  = 8%
  π = 0.00  → δ = 10% - 0%  = 10%
  π = -0.40 → δ = 10% + 2%  = 12%
```

### Effective Decay Rate (for Your Stake)

```
δ_effective = δ_base × (1 - tenure_reduction) × (1 - vault_reduction)
```

| Tenure | Reduction |
|--------|-----------|
| 0-1 years | 0% |
| 1-2 years | 20% |
| 2-5 years | 40% |
| 5+ years | 70% |

| Vault Tier | Lock Period | Reduction |
|------------|-------------|-----------|
| None | - | 0% |
| Bronze | 30 days | 20% |
| Silver | 90 days | 50% |
| Gold | 365 days | 80% |
| Permanent | No unlock | 95% |

**Example:**
```
Base rate = 10%, 3-year tenure (40% reduction), Silver vault (50% reduction):
δ_effective = 10% × (1 - 0.40) × (1 - 0.50)
            = 10% × 0.60 × 0.50
            = 3%
```

### Daily Decay

```
δ_daily = 1 - (1 - δ_annual)^(1/365)

For 10% annual:
δ_daily = 1 - (0.90)^(1/365) = 0.0289% per day
```

### Stake Value After Decay

```
V_tomorrow = V_floor + (V_decayable × (1 - δ_daily))

Where:
  V_floor = Initial × Floor% (protected, never decays)
  V_decayable = Current - V_floor
```

---

## 🎁 TOKEN EMISSION EQUATIONS

### Daily Emission Pool

```
E_daily = 16,000 × Halving_Multiplier

Halving Schedule:
  Year 0-1:  × 1.00    = 16,000/day
  Year 1-2:  × 0.50    =  8,000/day
  Year 2-3:  × 0.25    =  4,000/day
  Year 3-4:  × 0.125   =  2,000/day
  Year 4-5:  × 0.0625  =  1,000/day
  Year 5-6:  × 0.03125 =    500/day
  Floor:     Minimum   =    100 AMOS/day
```

### Sigmoid Pool Separation

The daily emission pool is split between technical and growth contributions using a sigmoid function that caps growth allocation over time:

```
growth_cap(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))

Where:
  ceiling = 2000 BPS (20%)
  floor = 300 BPS (3%)
  midpoint = 540 days
  k = 0.01 (k_scaled = 100)
```

**Example Trajectory:**
```
Day 0:    ≈ 20% (growth_cap ≈ 2000 BPS)
Day 270:  ≈ 18.9% (growth_cap ≈ 1889 BPS)
Day 540:  ≈ 11.5% (growth_cap ≈ 1150 BPS)
Day 900:  ≈ 3.5% (growth_cap ≈ 350 BPS)
Day 1260: ≈ 3.0% (growth_cap ≈ 300 BPS)
```

**Daily Split:**
```
Technical pool = E_daily - growth_pool
Growth pool = min(sigmoid_cap, natural_weighted_share)
Unused growth allocation rolls into technical pool
```

### Your Token Reward

```
                Your Points
Your Tokens = ────────────── × Daily Pool
              Total Points
```

**Example:**
```
You: 100 points, Total: 5,000 points, Pool: 16,000 AMOS

Your Tokens = (100 / 5,000) × 16,000 = 320 AMOS
```

---

## 📈 POINTS EQUATIONS

### Contribution Types (11 Total: 8 Technical + 3 Growth)

#### Technical Contributions
1. **Bounty** — Bounty Value (in AMOS)
2. **Referral** — Emails, signups, conversions, active months
3. **Sales** — Users signed up
4. (Additional technical contributions tracked per activity)

#### Growth Contributions
- **bug_report**: 10,000 BPS (100%) — Growth pool
- **referral**: 6,000 BPS (60%) — Growth pool
- **signup**: 4,000 BPS (40%) — Growth pool

### Referral Points

```
P_referral = (Emails × 1) + (Signups × 5) + (Conversions × 10) + (Active Months × 2)
```

**Example:**
```
Send 20 emails, get 4 signups, 2 convert, stay 6 months:
= (20 × 1) + (4 × 5) + (2 × 10) + (6 × 2)
= 20 + 20 + 20 + 12
= 72 points
```

### Sales Points

```
P_sales = Users Signed Up × 1
```

### Bounty Points

```
P_bounty = Bounty Value (in AMOS)

50 AMOS bounty = 50 points
```

---

## 💸 REVENUE SHARE EQUATIONS

### Protocol Fee Allocation

```
Staked Holders: 50% of Protocol Fees (commercial bounties only)
Permanently Burned: 40% of Protocol Fees (deflationary mechanism)
AMOS Labs:      10% of Protocol Fees (operations, development)
```

### Your Payout

```
                  Your Stake
Your Payout = ──────────────── × (Protocol Fees × 50%)
              Total Staked
```

**Example:**
```
You: 50,000 AMOS, Total: 10,000,000 AMOS, Protocol Fees: $100,000

Holder Pool = $100,000 × 50% = $50,000
Your Payout = (50,000 / 10,000,000) × $50,000 = $250/month
```

---

## 🛡️ PROTECTION EQUATIONS

### Grace Period

```
First 365 days: NO DECAY at all
```

### Decay Floor (Never Goes Below)

```
V_floor = Initial × Floor%

Floor Schedule:
  Year 0-1:  5% floor
  Year 1-2: 10% floor
  Year 2-5: 15% floor
  Year 5+:  25% floor
```

**Example:**
```
10,000 AMOS stake at Year 3:
V_floor = 10,000 × 15% = 1,500 AMOS (protected forever)
```

### Clawback (Distribution Stakes)

```
First 90 days: Stake can be clawed back if customer churns
After 90 days: Stake is confirmed permanent
```

---

## 📱 QUICK REFERENCE CARD

```
┌──────────────────────────────────────────────────────────────────┐
│                     KEY NUMBERS                                   │
├──────────────────────────────────────────────────────────────────┤
│  Total Supply:          100,000,000 AMOS                         │
│  Daily Emission:        16,000 AMOS (halving annually)           │
│  Base Decay:            10% annual                               │
│  Min/Max Decay:         2% - 25% annual                          │
│  Protocol Fee:          3% on commercial bounties                │
│  Fees to Stakers:       50%                                      │
│  Grace Period:          12 months                                │
│  Clawback Period:       90 days                                  │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                  CLAIM MECHANICS                                  │
├──────────────────────────────────────────────────────────────────┤
│  DEFAULT_CLAIM_TIMEOUT_HOURS:  72 hours                          │
│                                Range: 1-720 hours                │
│                                                                  │
│  MAX_CONCURRENT_CLAIMS (by trust level):                         │
│  • Level 1: 3 concurrent claims                                  │
│  • Level 2: 5 concurrent claims                                  │
│  • Level 3: 8 concurrent claims                                  │
│  • Level 4: 12 concurrent claims                                 │
│  • Level 5: 20 concurrent claims                                 │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                 DISPUTE PARAMETERS                                │
├──────────────────────────────────────────────────────────────────┤
│  DISPUTE_WINDOW_HOURS:          48 hours                         │
│  DISPUTE_STAKE_BPS:             500 BPS (5% of bounty value)     │
│  DISPUTE_RESOLUTION_TIMEOUT:    168 hours (7 days)               │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                 REGISTRY PARAMETERS                               │
├──────────────────────────────────────────────────────────────────┤
│  REGISTRY_AUTO_FREEZE_SECONDS:  94,608,000 (3 years)             │
│  REGISTRY_MAX_EXTENSIONS:       2                                │
│  REGISTRY_EXTENSION_DURATION:   31,536,000 seconds (1 year)      │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                     THE FLOW                                      │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Bounty Payouts ──→ 3% Protocol Fee ──→ Revenue                  │
│                                           │                      │
│                                           ▼                      │
│                                    Profit Ratio                  │
│                                           │                      │
│                                           ▼                      │
│                                    Decay Rate                    │
│                                           │                      │
│                                           ▼                      │
│                                 Token Value Stability            │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                     EARN → STAKE → EARN                          │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Contribute ──→ Earn Points ──→ Get Tokens                       │
│                                      │                           │
│                                      ▼                           │
│                              Stake Tokens                        │
│                                      │                           │
│                                      ▼                           │
│                           Get Revenue Share                      │
│                                      │                           │
│                                      ▼                           │
│                           Re-invest or Cash Out                  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

---

## 🧮 EXAMPLE: COMPLETE CYCLE

```
SCENARIO: You're an active contributor for one month

STEP 1: AWS Usage (Platform Level)
  Monthly AI costs:     $30,000
  Monthly email costs:   $1,000
  Monthly compute:       $5,000
  Monthly other:         $4,000
  ──────────────────────────────
  Total Costs:          $40,000

STEP 2: Protocol Fees
  Bounty payouts:       $2,000,000
  Protocol fee (3%):       $60,000
  ──────────────────────────────
  Total Protocol Fees:     $60,000

STEP 3: Profit Ratio
  π = ($60,000 - $40,000) / $40,000 = 0.50 (50% profit!)

STEP 4: Decay Rate
  δ = 10% - (0.50 × 5%) = 10% - 2.5% = 7.5%
  (Very healthy - low decay rewards holders)

STEP 5: Your Contribution
  You referred 5 users (signups): 5 × 5 = 25 points
  1 converted to paid:            1 × 10 = 10 points
  You completed a 50-point bounty: 50 points
  ──────────────────────────────────────────────
  Total points: 85 points

STEP 6: Daily Token Reward (average day)
  Daily pool: 16,000 AMOS
  Your daily points: ~3 (85 ÷ 30 days)
  Platform daily points: ~500
  Your daily tokens: (3 / 500) × 16,000 = 96 AMOS

STEP 7: Monthly Token Earnings
  96 AMOS × 30 days = 2,880 AMOS earned

STEP 8: Decay on Existing Stake
  Previous stake: 10,000 AMOS
  Annual decay: 7.5%
  Monthly decay: ~0.64%
  Decay amount: 10,000 × 0.64% = 64 AMOS

STEP 9: Net Position
  Previous: 10,000 AMOS
  Earned:   +2,880 AMOS
  Decayed:     -64 AMOS
  ─────────────────────
  New stake: 12,816 AMOS  (+28% growth!)

STEP 10: Protocol Fee Share
  Your stake: 12,816 AMOS
  Total staked: 10,000,000 AMOS
  Your share: 0.128%
  Holder pool: $60,000 × 50% = $30,000
  Your payout: 0.128% × $30,000 = $38.40/month
```

---

*This is the complete, bulletproof math. All tied together.*
