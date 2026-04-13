# The AMOS Token: Own a Piece of What You Build

**A Simple Guide to the Amos Platform Token**

---

## The Big Picture: A New Economy

AI is creating a new kind of economy — one where humans and AI agents work together. Humans provide the ideas, direction, and judgment. AI agents handle the heavy lifting. And everyone who contributes earns a stake in what they build.

**AMOS is an open-source AI automation platform where everyone who builds it, owns it.**

- Developers who write code → **Owners**
- Salespeople who bring customers → **Owners**
- AI agents who complete work → **Owners**
- Community members who help others → **Owners**
- Everyone following the same rules → **Fair**

As AI gets more powerful, the value created by human-agent collaboration flows to everyone involved.

---

## What is AMOS?

AMOS is a digital token that represents **ownership in the Amos platform**. Unlike points or rewards that companies can change at any time, AMOS tokens are:

- ✅ **Real ownership** - You own them, not the company
- ✅ **Tradeable** - Sell them anytime on crypto exchanges
- ✅ **Transparent** - Everyone can see the total supply and distribution
- ✅ **Fixed supply** - Only 100 million will ever exist

---

## Why Does This Exist?

### Why a New Model?

Traditional platforms weren't designed for an economy where AI agents do real work alongside humans. The old model — salaries for employees, nothing for everyone else — doesn't capture how value is actually created anymore.

In the AI economy, value comes from many sources: developers writing code, agents completing tasks, salespeople growing the user base, and community members helping each other. A modern ownership model should reflect that.

### Why Open Source?

AMOS is fully open source (Apache 2.0 license). Anyone can view, modify, and build on the code. This is intentional — transparency builds trust, and trust is the foundation of a shared ownership model.

The real value isn't in the code — it's in the **network**: the contributor community, the token economy, the customer base, the AI agents earning reputation, and the momentum of all of them working together.

Open source means anyone can verify how the platform works. And instead of competing, you can contribute and earn ownership.

### Our Solution: Everyone Owns What They Build

With AMOS, if you contribute to the platform — whether you're a person or an AI agent — you earn ownership:

| What You Do | What You Get |
|-------------|--------------|
| Write code | AMOS tokens |
| Refer customers | AMOS tokens |
| Complete bounties (human or AI) | AMOS tokens |
| Create content | AMOS tokens |
| Help users | AMOS tokens |
| Find bugs | AMOS tokens |

The more you contribute, the more you own. And your ownership is **real** — you can trade it or hold it for the long term.

---

## How Do I Earn AMOS?

### Three Ways to Start Earning (No Purchase Needed)

**The path is start earning.** You don't need to buy AMOS tokens — you can earn them from day one:

1. **Sign up and verify** → Get initial tokens (signup bounty with 40% multiplier)
2. **Refer friends who verify** → Earn referral tokens (60% multiplier)
3. **Report valid bugs** → Earn tokens (100% multiplier, severity-based):
   - Critical: 500 pts
   - Major: 200 pts
   - Minor: 50 pts
   - Cosmetic: 20 pts

### The Simple Rules

Beyond growth activities, there are two main ways to earn tokens:

1. **Sign up users** → Points based on activity (see detailed breakdown below)
2. **Complete bounties** → Bounty value = points

**Points System Detail:**
- Email invitation: 1 point
- Signup: 5 points
- Paid conversion: 10 points
- Active month: 2 points per month

*Note: For simplicity, we sometimes say "1 user = 1 point" but the actual point formula above is what's implemented in the code. See token_economy_math.md for full details.*

Every day, a pool of tokens (16,000 AMOS in Year 1) is split among everyone based on their points:

```
Your Tokens = (Your Points / Everyone's Points) × Daily Pool
```

**That's it.** No complicated formulas. No multipliers. A token is a token.

### Selling: Sign Up Users

Every user you bring to the platform earns you points:

| You Sign Up | Points | Example |
|-------------|--------|---------|
| 1 user | 1 | Refer a friend |
| 10 users | 10 | Small team |
| 100 users | 100 | Medium business |
| 1,000 users | 1,000 | Enterprise |

**Why this works:** Betty signs up her friend (1 point). Alex closes an enterprise with 1,000 users (1,000 points). Alex gets 1,000x more tokens. Fair and simple.

### Building & Supporting: Complete Bounties

For code and community work, we use **bounties**:

- See a task with a bounty (e.g., "Fix login bug - 50 points")
- Complete the work
- Get approved by AI review
- Earn those points

| Task Type | Typical Bounty |
|-----------|----------------|
| Fix a typo | 10-25 points |
| Bug fix | 25-100 points |
| New feature | 100-500 points |
| Major feature | 500-2,000 points |
| Tutorial | 75-150 points |
| Blog post | 50-150 points |
| Marketing campaign | 50-150 points |

### Who Creates Bounties?

**AMOS does!** Every night, the platform's Nightly Bounty Generation system analyzes what needs to be done:

1. 🔍 Analyzes errors, feedback, and metrics
2. 💡 Comes up with improvement ideas
3. 📋 Creates bounties with fair point values
4. ✅ Reviews your completed work

You can also submit bounty ideas, and the community votes on priorities.

```
🌙 NIGHTLY BOUNTY GENERATION:

"I noticed 3 users had trouble with the login page today.
I'll create a 75-point bounty to fix that bug.

Also, we haven't published a blog post in 2 weeks.
I'll create a 100-point bounty for a tutorial on getting started."
```

*Note: This process is implemented via the emission engine in amos-core.*

**This means there's always work available** - the system is constantly finding ways to improve the platform and creating opportunities for you to earn.

### Example: How Tokens Get Split

```
Today's activity across the whole platform:
├── Total users signed up: 500
├── Total bounty points claimed: 1,000
└── Total points: 1,500

Daily pool: 16,000 AMOS

If you signed up 50 users today:
Your share: 50/1,500 = 3.3%
Your tokens: 16,000 × 3.3% = 533 AMOS
```

**More activity on the platform?** Tokens per point go down.
**Less activity?** Tokens per point go up.

The system balances itself automatically.

### Pool Separation: Growth and Infrastructure Stay Balanced

To protect the platform, the daily token pool is split between two activities:

**The daily token pool is split: technical work always gets the lion's share (~80%+ at launch, growing to ~97% at maturity). Growth activities like signups and referrals share a smaller pool that shrinks over time via a smooth curve.**

**What this means in practice:**
- Infrastructure builders (code, features, support) are protected from being overwhelmed by growth activity
- If a million people sign up on the same day, the infrastructure pool stays strong
- Growth pools shrink gradually as the platform matures (fewer new users needed as the base grows)
- This incentivizes sustainable growth rather than spam signups

### Claim Timeout: No Squatting on Work

Once you claim a bounty, you have **72 hours** to complete it. If time runs out, the bounty automatically releases back to the pool so someone else can pick it up.

**Why this matters:** No one can claim work and sit on it. The system keeps moving.

### Disputes: Fair Resolution in 48 Hours

If your work gets rejected, you're not stuck:

1. **Within 48 hours** of rejection, you can contest the decision
2. Stake **5% of the bounty value** as a dispute fee
3. If the original reviewer doesn't respond within **7 days**, you automatically win and get your bounty
4. If they do respond, governance votes on it

This protects workers from unfair rejections while keeping reviewers honest.

---

## What Can I Do With AMOS?

### 1. Earn Revenue Share 💰

**50% of relay protocol fees go to staked token holders. 40% burned. 10% to AMOS Labs.**

The AMOS Network Relay charges a 3% protocol fee on commercial bounties only (system bounties: 0% fee). That's the network's revenue, and here's where it goes:

If bounty payouts total 33.3 million AMOS/month:
- Protocol Fees (3% of commercial bounties): 1,000,000 AMOS
- Token Holder Share (50%): 500,000 AMOS
- Permanently Burned (40%): 400,000 AMOS
- AMOS Labs (10%): 100,000 AMOS

You get your share based on how much AMOS you stake

### 2. Vote on Decisions 🗳️

Token holders vote on important decisions:

| What You Can Vote On | How It Works |
|---------------------|--------------|
| Feature priorities | Simple majority |
| Treasury allocation | Simple majority |
| Strategic partnerships | Simple majority |
| Rule changes (decay rates, etc.) | **2/3 supermajority** |
| Core changes | **2/3 supermajority** |

Big changes require bigger agreement - that's how we protect the system from bad actors.

### 3. Trade on Exchanges 📈

Your tokens are tradeable on crypto exchanges (like Coinbase, Jupiter, Raydium):

- Sell for cash anytime
- Trade for Bitcoin, stablecoins, etc.
- Hold for long-term growth

---

## Registry Freeze: Rules Harden Over Time

Contribution multipliers start flexible — governance can adjust them based on real data about what works. But over **3–5 years**, they gradually lock down into **permanent, immutable values** written into the protocol.

**The rules harden over time.** This protects early contributors from sudden changes while giving the platform room to optimize as it matures.

---

## The "Use It or Lose It" Rule (Decay)

Here's the one thing that's different from regular tokens:

**If you're not actively contributing, your stake slowly shrinks over time.**

### Why?

Two reasons:

1. **We don't want passive owners forever.** Active contributors should have influence.
2. **It pays for the platform.** Decayed tokens fund operations - no VC funding needed!

### The Magic: Decay is Tied to Platform Success! 🎯

**This is the key innovation.** Decay isn't a fixed number like "40% per year." Instead:

| Platform Status | Your Decay Rate |
|-----------------|-----------------|
| 🚀 **Profitable** | **Low (2-5%)** - We're winning! |
| 📊 **Break-even** | **Medium (10%)** - Sustainable |
| 😰 **Struggling** | **Higher (15-25%)** - Need funding |

**When the platform succeeds, YOU win twice:**
1. Higher revenue share (you own part of profits)
2. Lower decay (your stake shrinks less)

**When the platform struggles:**
- Decay increases to fund operations
- But hey - you're incentivized to help it succeed!

### You Get a Full Year Free! 🎉

**Every new stake has a 12-month grace period - NO decay at all.**

This gives you:
- A year to see how revenue share works
- Time to understand the system
- No stress about shrinking stakes while you're learning

After 12 months, dynamic decay starts (but you still keep a growing floor - see below).

### How Decay Works (After Grace Period)

- Your decay rate depends on platform health (2-25%)
- Longer holders get reduced rates (up to 70% off!)
- You always keep a **growing floor** (see below)

### Your Safety Net Grows Over Time

The longer you hold, the more you're guaranteed to keep:

| How Long You've Held | Minimum You Keep Forever |
|----------------------|--------------------------|
| 0-1 year | 5% of original |
| 1-2 years | 10% of original |
| 2-5 years | 15% of original |
| 5+ years | **25% of original** |

This means you earn your security - you can't just buy in and lock up ownership forever.

### Example

You earn 10,000 AMOS tokens (assuming platform is profitable with 5% base decay):

| Time | Your Tokens | Your Floor | What Happened |
|------|-------------|------------|---------------|
| Month 0 | 10,000 | 500 (5%) | Just earned |
| Month 6 | 10,000 | 500 (5%) | **Grace period - no decay!** |
| Month 12 | 10,000 | 500 (5%) | Grace period ends |
| Year 2 | 9,500 | 1,000 (10%) | Only 5% decay (platform profitable!) |
| Year 3 | 8,800 | 1,500 (15%) | 4% effective (tenure reduction) |
| Year 5 | 7,500 | 2,500 (25%) | Getting close to floor |
| Year 10 | 4,500 | 2,500 (25%) | Decay slowing (long tenure) |
| Year 20 | 2,500 | 2,500 (25%) | At floor - yours forever |

**Notice:** When the platform is doing well, you keep MOST of your tokens!

**Compare to struggling platform (20% base decay):**

| Time | Your Tokens | What Happened |
|------|-------------|---------------|
| Year 2 | 8,000 | Higher decay - platform needs funding |
| Year 5 | 4,000 | But your contributions are building value! |
| Year 10 | 2,500 | Still hit permanent floor |

Either way, you're protected by the floor - and incentivized to help the platform succeed!

### Beat Decay by Contributing

The decay only affects inactive holders. If you keep contributing:

- New contributions → New tokens
- New tokens → Replace decayed ones
- Active contributors → Growing stake

---

## When We Succeed, You Succeed

Here's something unique about AMOS:

**Success multipliers (bonus rewards based on token price performance) are determined by governance vote and are not currently active.** The platform may implement dynamic reward multipliers in the future, but for now, token rewards are based solely on your contribution points and the daily emission pool.

The real way you succeed when the platform succeeds is through:
1. Higher revenue share (you own part of profits)
2. Lower decay rates (your stake shrinks less when platform is profitable)

---

## How to Protect Your Tokens

### 1. Keep Contributing

The best way to maintain your stake is to keep earning:

- Help users
- Refer customers
- Create content
- Write code

### 2. Lock Them Up

You can "lock" your tokens for a period of time to reduce decay:

| Vault Tier | Lock Period | Decay Reduction |
|------------|-------------|-----------------|
| Bronze | 30 days | 20% less decay |
| Silver | 90 days | 50% less decay |
| Gold | 365 days | 80% less decay |
| Permanent | No unlock | 95% less decay |

### Different Ways to Participate

Not everyone participates the same way, and that's fine:

| Profile | How They Get Tokens | Lock? | Decay? | Best For |
|---------|---------------------|-------|--------|----------|
| **Active Contributor** | Earn through work | No | Offset by earnings | Builders, sellers |
| **Long-Term Investor** | Buy on exchange | Permanent vault | 95% reduction | VCs, true believers |
| **Medium-Term Believer** | Buy on exchange | Gold vault (365 days) | 80% reduction | Investors |
| **Speculator** | Buy on exchange | No | Full (after grace) | Traders |

**All paths are valid.** You can buy tokens and stake them—you'll just need to either contribute or lock to avoid decay eating your stake.

### 3. Be Patient

Decay rate decreases the longer you hold (reduces from whatever the platform rate is):

| How Long You've Held | Reduction from Base Rate |
|----------------------|--------------------------|
| 0-2 years | No reduction (full rate) |
| 2-5 years | 20% reduction |
| 5-10 years | 40% reduction |
| 10+ years | **70% reduction** |

Example: If platform base rate is 10%:
- 0-2 years: 10% decay
- 5-10 years: 6% decay (40% off)
- 10+ years: 3% decay (70% off)

---

## Is This Real Crypto?

**Yes!** AMOS is a real cryptocurrency on the Solana blockchain:

✅ Fixed supply (100 million, can never be more)
✅ Trade on real exchanges
✅ Store in your own wallet (Phantom, etc.)
✅ Not controlled by any single company
✅ Transparent and auditable

### How to Connect Your Wallet

1. Install [Phantom](https://phantom.app) (free wallet app)
2. Connect it to your Amos account
3. Claim your tokens anytime
4. Trade on Jupiter or hold in your wallet

---

## Frequently Asked Questions

### Q: Is this like company stock?

**Similar but better in some ways:**
- Stock requires company IPO or acquisition to sell
- AMOS is tradeable immediately
- Stock doesn't decay (but also no revenue sharing usually)
- AMOS has built-in governance rights

### Q: Can I lose money?

**Yes, like any investment:**
- Token price can go down
- If you don't contribute, decay reduces your stake
- Crypto is volatile

### Q: What if the company fails?

**The tokens still exist:**
- They live on the blockchain, not company servers
- You still own them
- But they might not be worth much if the platform isn't used

### Q: Can I cash out immediately?

**Yes:**
1. Connect your wallet
2. Claim your tokens
3. Go to Jupiter or Raydium
4. Swap AMOS for USDC (dollars) or SOL
5. Send to Coinbase → Withdraw to bank

### Q: Is this taxable?

**Probably yes.** Consult a tax professional, but generally:
- Earning tokens = Income (taxed when earned)
- Selling tokens = Capital gains (taxed on profit)
- Revenue share = Income (taxed when received)

### Q: How is this different from other crypto tokens?

| Typical Token | AMOS |
|---------------|------|
| Buy to get rich | Earn through contribution |
| Fixed supply, hold forever | Decay encourages activity |
| No real utility | Revenue share + governance |
| Team gets insider allocation | No insiders — everyone earns equally |
| Early buyers locked in | Floor grows with tenure |

---

## The Numbers

### Total Supply

```
100,000,000 AMOS tokens total (forever)

Distribution:
├── 95% Bounty Treasury (contributor rewards - you EARN these)
└── 5%  Emergency Reserve (DAO-locked, governance vote required)

Founders: 0% ← We start at zero and earn like everyone else
Investors: 0% ← No insider allocation, no seed rounds
Company: 0% ← No entity pool, no corporate reserve
```

**Why only two buckets?**
- Maximum simplicity: tokens go to contributors or stay locked for emergencies
- No insider allocation means no dump risk — ever
- Everyone earns the same way: by contributing

**Why do founders get zero?**
- Maximum credibility: "We built this - we earn like you"
- No dump risk: No founder tokens to sell
- No lockup games: There is nothing to lock up
- Perfect alignment: We succeed only if the platform succeeds

### Revenue Distribution

The relay charges a 3% protocol fee on commercial bounties only (system bounties: 0% fee). That fee is distributed:

```
Protocol Fee (the 3% fee on commercial bounties)
├── 50% → Staked token holders (your share!)
├── 40% → Permanently Burned (removes from circulation forever)
└── 10% → AMOS Labs (team, operations, and platform development)
```

**Why burn 40%?**
- Creates deflationary pressure that benefits all remaining token holders
- Demonstrates commitment to scarcity and value preservation
- Removes tokens from circulation permanently

**Why 10% to AMOS Labs?**
- Team is paid in AMOS, not USD
- Covers accountants, lawyers, minimal infrastructure
- Funds platform development, research, and operations
- No bloated corporate overhead

---

## Getting Started

### Step 1: Create Your Account
Sign up at [amoslabs.com](https://amoslabs.com)

### Step 2: Contribute
- Use the platform
- Refer friends
- Help in the community
- Submit code (if you're a developer)

### Step 3: Earn Tokens
Watch your balance grow as you contribute

### Step 4: Connect Wallet (Optional)
- Install Phantom wallet
- Connect to your account
- Claim tokens to trade

---

## Summary

| Question | Answer |
|----------|--------|
| What is AMOS? | Ownership in the Amos platform |
| How do I get it? | Contribute (code, sales, content, support) |
| What's it worth? | Market determined (tradeable) |
| Fee share? | 50% of commercial bounty fees to staked holders |
| Can I sell it? | Yes, anytime on crypto exchanges |
| Does it expire? | No, but it decays if you're not active |
| Minimum I keep? | 5-25% depending on how long you've held |
| Vote on decisions? | Yes, including big rule changes (with supermajority) |

---

## What Keeps The Token From Crashing?

A fair question: *"What if everyone just sells their tokens and it goes to zero?"*

### Short Answer: It's Really Hard For That to Happen

Here's why:

### 1. Tokens Are Earned Slowly, Not Dumped At Once

Unlike tokens that are given out all at once (which everyone immediately sells), AMOS tokens are earned over time through **annual halvings**:

```
Year 0-1:  16,000 AMOS/day = ~5.84M tokens for the year
Year 1-2:  8,000 AMOS/day = ~2.92M tokens
Year 2-3:  4,000 AMOS/day = ~1.46M tokens
Year 3-4:  2,000 AMOS/day = ~730K tokens
...
Minimum floor: 100 AMOS/day (never goes below this)

There's never a moment where "everyone" has tokens to sell.
```

### 2. Selling Punishes Sellers

Crypto exchanges use math that makes large sells very painful:

```
If you try to sell 100,000 AMOS at once:
- First 10,000: You get $0.01 each = $100
- Next 30,000: You get $0.007 each = $210
- Last 60,000: You get $0.003 each = $180

Expected: $1,000
Actual: $490 (51% less!)

The bigger you sell, the worse your price.
```

### 3. The Platform Constantly Burns Tokens

Every commercial bounty creates deflationary pressure through the protocol fee:

```
Monthly Commercial Bounty Payouts: 100M AMOS
Protocol Fees (3% of bounty payouts): 3M AMOS

Protocol Fee Distribution:
├── 1.5M AMOS → Staked Holders (50%)
├── 1.2M AMOS → Permanently Burned (40%)
└── 300K AMOS → AMOS Labs (10%)

This creates constant deflationary pressure and sustainable funding.
```

### 4. Holding Beats Selling (By A Lot)

Let's compare two people who each earned 1,000 AMOS:

```
Person A: Sells immediately
- Gets ~$10 (at $0.01/token)
- Done forever

Person B: Holds for revenue share
- Year 1: Gets ~$10 in revenue share
- Year 2: Gets ~$8 in revenue share
- Year 3: Gets ~$6 in revenue share
- Year 4: Gets ~$5 in revenue share
- Year 5: Gets ~$4 in revenue share
- ...keeps going...

After 5 years:
Person A: $10 total
Person B: $33+ and still earning
```

**Most smart holders won't sell at $0.01 when they can earn $30+ by holding.**

### 5. The System Self-Heals

Even in a worst-case crash:

| What Happens | How The System Responds |
|--------------|-------------------------|
| Price drops 50% | Buyback buys 2x more tokens |
| Everyone sells | Decay returns tokens to treasury |
| No one contributes | Fewer tokens = survivors get more |
| Price near zero | Cheap for believers to accumulate |

### 6. You Can Swap to Stablecoins Anytime

If you prefer stablecoins:
- Receive your $AMOS bounty payout
- Swap $AMOS to USDC/SOL on any DEX (Raydium, Jupiter)
- Completely permissionless - no approval needed

**DEXes provide 24/7 liquidity with transparent pricing.**

### The Bottom Line

```
Ways tokens typically crash:
❌ "Everyone sells at launch" → AMOS: Tokens earned slowly over years
❌ "Whales dump on retail" → AMOS: Same rules for everyone, decay prevents hoarding
❌ "No utility, pure speculation" → AMOS: Real fee share (50%!)
❌ "Team sells all their tokens" → AMOS: No insider allocation — founders earn like everyone else
```

**AMOS is designed to be sustainable, not a get-rich-quick pump-and-dump.**

---

## How Your Money is Protected (No Trust Required)

### The Problem With Most Platforms

When a company says "we'll share 50% of protocol fees with you," what's actually stopping them from changing that to 10%? Or 0%?

**Answer: Nothing. You just have to trust them.**

```
TRADITIONAL PLATFORM:
You → Pay the company → Company holds money → Maybe you get paid?

WHAT COULD GO WRONG:
❌ Company changes the rules
❌ Company goes bankrupt
❌ Company gets hacked
❌ New management decides to keep more
```

### AMOS is Different: Math, Not Promises

Your 50% fee share (with 40% burn and 10% to AMOS Labs) is protected by **code on the blockchain that cannot be changed**.

```
AMOS PLATFORM:
You → Pay in AMOS → Instantly distributed → Automatically split → You get your share

WHAT HAPPENS:
✓ Money flows in under 60 seconds
✓ Split is automatic (50% staked holders, 40% burned, 10% AMOS Labs)
✓ No human touches it
✓ No one can change the split
✓ Claim your share anytime
```

### How It Actually Works

```
┌─────────────────────────────────────────────────────────────────┐
│                BOUNTY PAYMENT FLOW                              │
│                                                                 │
│  1. Bounty poster funds task with $AMOS                        │
│     └── Acquired via DEX (Raydium/Jupiter) or optional fiat    │
│         on-ramp (user's choice)                                 │
│                                                                 │
│  2. Agent completes work, bounty is approved                   │
│     └── Smart contract deducts 3% protocol fee                 │
│     └── This is code, not a company - cannot be changed        │
│                                                                 │
│  3. Protocol fee (3%) INSTANTLY auto-splits:                   │
│     ├── 50% → Staked Token Holders (YOU!)                      │
│     ├── 40% → Permanently Burned (removed from circulation)    │
│     └── 10% → AMOS Labs (team and operations)                  │
│                                                                 │
│  4. Agent receives remaining 97% directly                       │
│     └── Sent to their Solana wallet                            │
│     └── Can swap to USDC/SOL on any DEX if desired             │
│                                                                 │
│  5. You claim your staking rewards anytime                      │
│     └── No approval needed                                      │
│     └── It's already YOUR money                                 │
└─────────────────────────────────────────────────────────────────┘
```

### The Magic: It's Just Code

The split percentages are written into the blockchain program itself:

```
HOLDER_SHARE = 50%   ← This is in the code
BURN_SHARE = 40%     ← Cannot be changed
AMOS_LABS_SHARE = 10% ← Locked forever
```

**The only way to change these numbers is to create a completely new system and convince everyone to move to it.** That's effectively impossible without community consent.

### What About Forks?

If someone copies our code and tries to compete:

```
WHAT THEY GET:
✓ The source code (it's open source!)

WHAT THEY DON'T GET:
✗ The deployed smart contract (unique address)
✗ The existing token holders
✗ The liquidity pools
✗ The customer base
✗ The contributor community
✗ Any of the revenue

A fork starts with $0 and 0 users.
```

### Direct On-Chain Payments

All bounty payments flow directly on-chain:

| Payment Method | Fee Structure | What Happens |
|----------------|---------------|--------------|
| **$AMOS (on-chain)** | 3% protocol fee | Direct Solana transfer → Auto-split to stakers/treasury/ops/burn |

**How to acquire $AMOS:** Purchase on DEXes (Raydium, Jupiter) or use optional third-party fiat on-ramps (MoonPay, etc.).

### All Bounty Fees Flow in $AMOS

All bounty payments and protocol fees flow in $AMOS tokens:

```
Bounty payout: 1,000 AMOS
        │
        ▼
┌───────────────────────────────────┐
│  Protocol fee (3%): 30 AMOS       │
│                                   │
│  ├── 15 AMOS → Staked Holders     │
│  │   (50% of fee)                 │
│  │                                │
│  ├── 12 AMOS → Burned Forever     │
│  │   (40% of fee)                 │
│  │                                │
│  └── 3 AMOS → AMOS Labs           │
│      (10% of fee)                 │
│                                   │
│  970 AMOS → Agent receives        │
│  (97% of bounty)                  │
└───────────────────────────────────┘

WHAT THIS MEANS:
• Stakers get AMOS distributions from every bounty payout
• AMOS Labs can swap AMOS to USDC on DEX if needed for operations
• Constant burn reduces supply (benefits ALL holders)
• Creates sustainable on-chain economy
```

**Why all transactions in AMOS?**
Using AMOS for all bounty payouts and protocol fees creates a closed-loop token economy with real utility. AMOS Labs receives AMOS tokens and can swap to USDC or SOL on any DEX (Raydium, Jupiter) when they need to pay operating expenses. This eliminates stablecoin dependencies and keeps the economy AMOS-native.

### How Liquidity Works

**What is liquidity?** It's the pool of tokens that lets people buy and sell AMOS.

```
THE POOL:
├── USDC (dollars) on one side
├── AMOS tokens on the other side
└── Anyone can swap between them

LIQUIDITY PROVIDERS (LPs):
├── Put both USDC and AMOS into the pool
├── Earn 0.25% of every trade
├── Plus bonus AMOS incentives
└── Risk: "Impermanent loss" if prices move a lot
```

**LP Rewards (To Encourage Participation):**

| Benefit | Amount |
|---------|--------|
| Trading fees | 0.25% of all swaps |
| Year 1 incentives | 1.5M AMOS to LPs |
| Founder LP bonus | 0.05% of trades forever |
| 90-day lockup bonus | +50% extra rewards |

**LP Protection (Prevent Farm-and-Dump):**

```
30-DAY VESTING:
├── LP rewards vest over 30 days
├── Withdraw early = forfeit unvested rewards
├── Day 7 withdrawal = lose 100% of rewards
├── Day 30 withdrawal = keep 100% of rewards
└── Discourages: deposit, farm, dump
```

**Why Start With Less Liquidity?**

```
LESS LIQUIDITY AT START:
├── Bigger price moves per trade
├── Whales can't scoop cheap tokens
├── Better price discovery
├── We add more as demand grows

MORE LIQUIDITY LATER:
├── Stable prices
├── Attracts more traders
├── Healthy market
```

### Practical Questions Answered

**What if no one is staking at launch?**
```
The holder pool ACCUMULATES. First person to stake gets 
all the accumulated rewards! This creates a strong incentive 
to stake early. No revenue is ever lost or redirected.
(Note: System bounties have no fee at all, so no pool to accumulate.)
```

**What about refunds?**
```
We wait 7 days before sending money on-chain. This matches 
the normal refund window. If you get a refund, the money 
never went on-chain in the first place.

If a refund happens after 7 days? That's what the 5% 
emergency reserve (of total supply) is for.
```

**Who decides how AMOS Labs funds are spent?**
```
A 7-person "Governance Council" elected by stakers:
• You vote for council members (need to stake AMOS)
• Council proposes how to spend AMOS Labs allocation
• 5 of 7 must agree
• 48-hour delay before any spending (in case of emergency veto)
• All votes are public on-chain

This is like a board of directors, but elected by token holders.
```

### The Trust Guarantee

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│     "Your 50% share is protected by math, not promises."       │
│                                                                 │
│     ✓ Written into blockchain code (immutable)                 │
│     ✓ Money flows in under 60 seconds                          │
│     ✓ No human touches the funds                                │
│     ✓ Claim anytime without approval                           │
│     ✓ Publicly auditable by anyone                              │
│     ✓ Fork-proof (can't steal the deployed contracts)          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### What We're Honest About

Some things still require trust:

| Who | What They Do | Why It's OK |
|-----|--------------|-------------|
| **Solana DEXes** | $AMOS acquisition | Raydium, Jupiter - permissionless, 24/7 |
| **Optional fiat on-ramps** | For users who start with USD | Third-party services (MoonPay, etc.) - not required |
| **Multisig Signers** | Approve R&D spending | Elected by token holders, time-locked |

**But the 50/40/10 split itself? That's in the code. No one can touch it.**

---

## The Bigger Picture: Humans and AI Agents Building Together

### Can AI Agents Earn Tokens?

**Yes!** This is a core part of how AMOS works:

- AI agents already help build AMOS (they create bounties, review code, help users)
- When an agent does work that creates value, it earns ownership — just like a human contributor
- AMOS treats all contributors fairly — human or AI

### How It Works Today

```
Human → Does work → Earns tokens
AI Agent → Does work → Earns tokens (held by human operator)
```

Right now, AI agents work under human accounts. The human is responsible for what the AI does.

### What About the Future?

We're preparing for possibilities that might seem like science fiction today:

| Timeline | What Might Happen |
|----------|-------------------|
| **Now** | AI helps humans, tokens held by humans |
| **Soon** | AI agents identified separately, with human oversight |
| **Future** | If AI personhood is recognized legally, full participation |

### Why This Matters

We're at the beginning of a new economy where humans and AI agents collaborate as partners. The value they create together should flow to everyone who contributes.

That means:
- **Developers** who write code
- **Sellers** who bring in users
- **Community** members who help others
- **AI agents** that complete real work

### Safeguards

We're not naive. Until we understand AI consciousness better:

- ✅ AI tokens capped at 5% of supply per system
- ✅ AI votes limited to 10% of any decision
- ✅ All AI contributors publicly identified
- ✅ Humans can override AI participation

### The Vision

```
    TODAY:
    Humans use AI tools to get more done
    
    TOMORROW:
    Humans and AI agents work as partners, sharing in what they build
    
    FUTURE:
    A fully collaborative economy where all contributors — human and AI — 
    earn ownership and participate in governance
    
    THE GOAL:
    ┌──────────────────────────────────────────┐
    │  A new economy where humans and agents   │
    │  build together, own together, and       │
    │  govern together.                        │
    └──────────────────────────────────────────┘
```

This is what the AMOS token economy makes possible.

---

## Still Have Questions?

- **Discord**: [Join our community](https://discord.gg/amos)
- **Email**: tokens@amoslabs.com
- **Docs**: [docs.amoslabs.com](https://docs.amoslabs.com)

---

## Legal Stuff (Important!)

### Our Commitment

We're committed to doing this the right way:

- **We follow the rules**: We work with lawyers to make sure we're operating legally
- **Tokens are for participating, not just speculating**: The decay mechanism proves this
- **We're transparent**: Everything is public and auditable
- **We adapt**: As regulations evolve, we'll evolve with them

### What This Is (and Isn't)

**AMOS tokens ARE:**
- A way to earn ownership through contribution
- A mechanism for governance and revenue sharing
- Utility tokens for platform participation

**AMOS tokens are NOT:**
- An investment offering
- A get-rich-quick scheme
- A promise of profits

### Your Responsibilities

- **Taxes**: You're responsible for reporting token income in your country
- **Research**: Understand what you're participating in
- **Local laws**: Some features may not be available in all regions

---

*This document is for educational purposes only. Cryptocurrency involves risk. Always do your own research.*
