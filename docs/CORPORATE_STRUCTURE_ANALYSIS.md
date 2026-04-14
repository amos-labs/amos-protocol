# AMOS Corporate Structure: Strategic Analysis

**April 2026 | Prepared by Claude for Rick Barkley**

> *Note: This document is not legal or financial advice. The securities, tax, and governance questions raised here are substantive and require a crypto-native law firm (e.g., Cooley, Fenwick, DLA Piper's blockchain practice, or specialized shops like Variant's legal network). This analysis is a strategic framework to sharpen your thinking before those conversations.*

---

## Why This Question Is Urgent

Before getting into structures, it's worth naming why Rick is probably thinking about this right now. Based on the docs, AMOS is approaching a concrete inflection point: mainnet launches this month (April 14-15 per the plan) and the token goes live on Raydium. The simplified token allocation — 95% Bounty Treasury for contributor rewards, 5% DAO-locked emergency reserve — eliminates several traditional corporate structure concerns (no investor pool to manage, no entity pool to custody), but the core questions remain.

The corporate structure question isn't academic. It determines:
- Whether the token is a security under US law
- Who controls the protocol if AMOS Labs is acquired, fails, or has a founder dispute
- What obligations you have to token holders vs. shareholders
- How any future equity investors relate to the token economy
- Tax liability on token appreciation

The mission — "structurally resistant to capture" — and the business model are actually in tension with each other unless you build the legal architecture deliberately. Let's walk through the realistic options.

---

## Part I: The Landscape of What You're Actually Building

Before structure, let's be precise about what entities need to exist regardless of legal wrapper:

| Unit | Revenue | Mission-Critical | Open Source? |
|------|---------|-----------------|--------------|
| **Infrastructure (Harness, CLI, Tools)** | None | Yes — it's the product | Yes (Apache 2.0) |
| **Relay/Protocol** | 3% fee → mostly to token holders | Yes — the economic engine | Yes (Solana programs) |
| **Token Economics** | N/A — token is the medium | Yes | On-chain (immutable) |
| **Managed Agent Services** | SaaS revenue (future) | No — but primary commercial upside | No |
| **Package Ecosystem** | Attribution fees (to creators) | Enabler | Yes |

Key observation: **AMOS Labs has no direct revenue from the core protocol.** The relay fee distributes to stakers (50%), burn (40%), and Labs (10%). The "Labs" 10% allocation funds operations and development, but that's a predictable revenue stream tied to protocol volume that doesn't exist yet.

The commercial revenue story is **managed agent services** — the persistent agent hosting, SaaS deployment, enterprise harness instances. That's the Layer 4 Platform work in the CLAUDE.md architecture.

This distinction matters enormously for structure: you're running an open-source protocol with token economics *and* a commercial SaaS business, and they need different legal containers.

---

## Part II: The Five Structural Models

### Model 1: Single Delaware C-Corp (AMOS Labs, Inc.)

**How it works:** One entity owns everything — the code, the token allocation, the commercial business, the brand.

**Pros:**
- Simple to operate, one cap table, one set of books
- Investors know what they're getting
- No inter-entity transfer pricing, no IP licensing agreements

**Cons — and this is a long list:**
- Single point of capture. A hostile acquirer buys AMOS Labs and suddenly "owns" the open-source protocol. Yes, the code is Apache 2.0, but the brand, the relay infrastructure, and the team all transfer. The "capture-resistant" thesis breaks immediately.
- Token + equity conflict. Investors get equity in a company whose direct revenue is thin (10% of relay fees + future managed services). The real value is in the token. But selling equity that derives value from token appreciation to US persons is a securities minefield.
- If Labs fails (no revenue, runway runs out), creditors may pursue any token-related assets held by the corporation.
- VCs with board seats can direct strategy in ways that conflict with the DAO governance design.

**Verdict:** The wrong answer for AMOS. It solves nothing and creates the exact capture dynamics you're trying to prevent.

---

### Model 2: Labs C-Corp + Protocol Foundation (The Uniswap/Optimism Model)

**How it works:**
- **AMOS Labs, Inc.** (Delaware C-Corp): Employs the team, builds commercial products (managed agent hosting, enterprise features), receives 10% of protocol fees from the relay, can raise traditional equity investment for the commercial SaaS business.
- **AMOS Foundation** (Cayman Islands or Marshall Islands): Holds protocol governance, stewards the Emergency Reserve (5M tokens, DAO-locked), stewards the open-source ecosystem. No employees, no commercial activity.

**Real-world precedents:**
- Uniswap Labs (Delaware C-Corp) + Uniswap Foundation (Delaware non-profit 501(c)(4))
- OP Labs (Delaware C-Corp) + Optimism Foundation (Cayman Islands Foundation Company)
- Compound Labs (C-Corp) + Compound Treasury

**Why it fits AMOS:**
- The foundation can be the "capture-resistant" entity. If Labs gets acquired, the foundation continues. The protocol lives in the foundation.
- Investors in Labs get equity in a company building commercial products — that's clean. They don't get direct exposure to token appreciation (which reduces securities risk for the equity side).
- Token holders have legal recourse through the foundation — it's a real entity with duties, not just on-chain code.
- The Emergency Reserve (5M tokens, DAO-locked) is naturally custodied by a foundation with a fiduciary duty to the community rather than a C-corp with fiduciary duties to shareholders.

**Key tensions:**
- Who controls the foundation? If Rick controls both Labs and the Foundation, you haven't actually separated power — you've just created two entities with the same de facto controller.
- The foundation needs a board with independence from Labs. The Uniswap Foundation's early struggles (controversy over its independence from Uniswap Labs) are instructive.
- Foundation governance is expensive and slow. Cayman foundation companies require resident directors, registered agents, annual filings.
- The foundation's scope is narrower under the simplified allocation — it governs the Emergency Reserve and protocol parameters, funded by the 20% treasury allocation from relay fees. That's sustainable only at scale.

**Cost to set up:** $30-80K in legal fees for proper formation. Ongoing: $15-25K/year in compliance costs.

---

### Model 3: Labs C-Corp + DAO (No Foundation)

**How it works:** Labs is the commercial entity. Governance is purely on-chain — token holders vote, smart contracts execute. No legal entity wraps the DAO.

**The appeal:** Pure decentralization. No foundation to capture.

**The problem:** DAOs without legal wrappers are general partnerships under US law. Every token holder is potentially personally liable for DAO obligations. This is the Bitmain problem — the more significant the DAO, the more attractive a litigation target it becomes. MakerDAO explicitly dissolved its foundation to become a "pure DAO," then spent years dealing with the legal vacuum this created.

This structure only works if:
1. The DAO has no US persons in control
2. No meaningful US activity
3. You're comfortable with the legal ambiguity

AMOS has a US founder, plans US customers, and is launching on a public blockchain. This isn't the right path.

**Verdict:** Too legally exposed for a US-based team with commercial aspirations.

---

### Model 4: Wyoming DAO LLC

**How it works:** Wyoming and several other states allow DAOs to form as LLCs, giving token holders limited liability and some legal recognition.

**Pros:**
- Legal personhood for the DAO
- Limited liability
- US-native, no offshore overhead

**Cons:**
- Tax treatment is pass-through (LLC members pay taxes on DAO income) — at scale with thousands of anonymous token holders, this becomes administratively impossible
- Not widely recognized internationally
- Legal clarity is still thin; there's very little case law
- Doesn't solve the investor relationship problem (VCs want C-corp equity, not DAO LLC membership)

**Verdict:** Interesting experiment, too early for a project at AMOS's stage.

---

### Model 5: Cayman Islands Foundation + US LLC (The Full Offshore Model)

**How it works:**
- **AMOS Foundation** (Cayman Islands Foundation Company): Issues tokens, holds protocol IP, receives and distributes treasury, governs protocol.
- **AMOS Labs LLC** (US or Singapore): Commercial operations, employs team under contract from the foundation.

**Who does this:** Aave (Aave Ltd UK + Aave Foundation Cayman), Near Protocol, Algorand Foundation.

**Pros:**
- Maximum regulatory flexibility — tokens issued from a Cayman entity have cleaner non-US securities treatment
- Foundation structure is mature in Cayman law with clear fiduciary duties
- Token appreciation in the foundation isn't immediately subject to US capital gains

**Cons:**
- High complexity. Two jurisdictions, transfer pricing between entities, potential controlled foreign corporation (CFC) rules for Rick as a US person
- Not a solution to SEC scrutiny — if you're selling to US persons, SEC has jurisdiction regardless of where your foundation is formed
- Expensive ($50-120K to set up, $25-40K/year to maintain)
- Looks evasive. If the story is "open, transparent, capture-resistant," operating through a Cayman entity raises eyebrows among the exact community you're trying to attract

**Verdict:** Valid for a project that primarily wants to serve non-US markets. Probably overkill for AMOS at this stage, and the transparency narrative takes a hit.

---

## Part III: The Recommendation

Given AMOS's mission, stage, and US base, the right structure is **Model 2**: a two-entity split between a Delaware C-Corp (Labs) and a US-based non-profit foundation or a Cayman Foundation Company.

Here's how to think about each entity:

### AMOS Labs, Inc. (Delaware C-Corp)

**What it does:**
- Employs the engineering team
- Builds and maintains the open-source harness (as a contribution to the commons)
- Operates the managed agent hosting service (Layer 4 Platform) as a commercial product
- Receives the 10% operations allocation from relay fees
- Can raise equity from investors — but the pitch is the *managed services business*, not the protocol
- Holds the commercial brand (amos.so, enterprise agreements, API contracts)

**What it does NOT do:**
- Issue the AMOS token (foundation does this)
- Control the relay protocol parameters (governance does this)
- Hold or manage the Emergency Reserve

**Investor relationship:** Any future equity investors buy into Labs, which is a SaaS company with managed agent hosting as its primary commercial product. The token is separate — investors can participate in token markets like anyone else (earning tokens through contribution like everyone else), but their equity investment is in the commercial business. The simplified allocation (no investor pool, no entity pool) makes this separation cleaner: there are no token allocations earmarked for investors at the protocol level.

### AMOS Foundation (Delaware Non-Profit or Cayman Foundation Company)

**What it does:**
- Custodies the Emergency Reserve (5M tokens, DAO-locked, governance vote required to access)
- Stewards the open-source protocol (technically and legally)
- Governs protocol parameters (decay rates, trust thresholds, relay fee structure)
- Provides legal personhood for on-chain governance decisions
- Employs a small team of protocol engineers and governance facilitators (funded by the 20% treasury allocation from relay fees)
- Can receive donations/grants from Labs and from external ecosystem participants

**Board composition (critical for independence):**
- Rick in a founding capacity initially
- 2-3 independent directors from the broader protocol/crypto ecosystem
- A community-elected seat once governance is active
- No Labs employees in majority control

**On the Emergency Reserve:** The 5M DAO-locked tokens belong in the Foundation, not in Labs. If they're in Labs and Labs gets acquired or goes bankrupt, those tokens become creditor assets. In the Foundation with a clear mission purpose, they're harder to reach and better aligned with the protocol's long-term interests. The Foundation's scope is deliberately narrow: govern the reserve and protocol parameters, nothing more.

---

## Part IV: The Token Securities Question

This is the single most legally fraught issue and deserves direct treatment.

The AMOS token has properties that help and hurt its securities analysis:

**Factors that help (utility/non-security framing):**
- You earn tokens through work, not investment
- Decay prevents pure speculative holding
- Governance utility is real (proposals, voting)
- The 12-month grace period for new contributors looks like a utility onboarding mechanism, not an investment vehicle

**Factors that hurt:**
- The LP launch (founder provides AMOS + USDC for liquidity) could be scrutinized as creating a secondary market. The SEC has brought cases on similar patterns.
- Staking yields (50% of relay fees to stakers) can read as investment returns if the court applies Reves analysis (the "investment contract" prong).

**The structural advantage of the simplified allocation:** The elimination of an explicit Investor Pool removes the most dangerous securities vector. There is no pool of tokens "held for Series Seed" — which would be a textbook Howey test failure if sold to investors expecting profit. Instead, 95% of tokens flow to contributors through work, and 5% sits in a DAO-locked reserve. If you want to raise money, sell equity in Labs (the SaaS business). Token participation is earned through contribution, not purchased as an investment.

This is not a complete answer — the SEC has been aggressive — but the simplified allocation puts AMOS in a structurally cleaner position than most token projects. You should still have a crypto-native securities attorney (Cooley, Fenwick, or Davis Polk's blockchain team) review the token distribution mechanics.

---

## Part V: How Other Projects Got This Wrong (and Right)

### MakerDAO → Sky: The Foundation Dissolution Trap

MakerDAO formed a foundation, then dissolved it in 2021 to become a "pure DAO." This created a legal vacuum that made it impossible to sign contracts, employ people, or respond to regulatory pressure. The project spent years backfilling legal infrastructure and eventually rebranded as Sky while rebuilding the organizational layer they had dissolved. The lesson: don't mistake governance decentralization for legal entity elimination. You need both.

### Uniswap: The Independence Problem

Uniswap Foundation was formed as an independent 501(c)(4), but early criticism focused on the fact that key personnel overlapped significantly between Labs and the Foundation. The community asked: who does the Foundation actually answer to? Building genuine independence — separate leadership, clear governance mandates, independent funding — is harder than forming the legal entity. AMOS should design this independence into the foundation's charter from day one.

### Optimism: The Citizen House Model

Optimism created a bicameral governance system: Token House (token holders) and Citizen House (non-transferable Citizenship NFTs). The split was designed to prevent pure plutocracy while preserving token holder input. AMOS's contribution-based decay has a similar effect — it structurally devalues passive token accumulation. But consider whether the governance design should be reflected in the legal structure. A foundation whose board includes community-elected directors who aren't token holders adds an independence layer pure token governance can't provide.

### Ethereum Foundation: The Swiss Model

The ETH Foundation is a Swiss Stiftung — a non-profit foundation under Swiss law. It's been durable and independent but operates in a regulatory gray zone that's becoming grayer as Switzerland tightens crypto rules. The Swiss Stiftung model was popular 2017-2020; it's fallen out of favor because Cayman and Marshall Islands structures are considered more flexible. Not a strong recommendation for a new project in 2026.

### Protocol Labs: The C-Corp Anomaly

Protocol Labs (Filecoin/IPFS) structured as a Delaware C-Corp and kept control centralized longer than most "decentralized" projects. It worked for them because they had substantial commercial revenue from enterprise IPFS hosting. But the Filecoin community has ongoing tensions about Labs' influence over ostensibly decentralized governance. AMOS's mission makes this model a bad fit — the whole point is capture resistance.

---

## Part VI: Questions Rick Should Answer Before Deciding

These are the clarifying questions that would sharpen the structure recommendation:

**1. Do you want VC equity investors at all?**
With no investor pool at the token level, any future investment would be equity in Labs (the SaaS/managed hosting business). Can Labs bootstrap to revenue before needing outside equity? Taking VC money creates governance pressure from investors whose incentive is exit (IPO or acquisition), not protocol longevity. Some of the most durable open-source protocol projects never took traditional equity investment. The simplified allocation makes this decision cleaner — there is no token-level mechanism for investor participation, so the question is purely about the commercial business.

**2. What does "the platform" mean in "spinning the platform off"?**
If the platform means the Relay/Bounty marketplace: this is the most valuable and most mission-critical piece. Spinning it off would require either (a) selling it (who to?), (b) giving it to a foundation, or (c) creating a separate token/entity. Option (b) is essentially Model 2 above. Option (c) would require a second token and potentially fragments the economic flywheel.

If the platform means the managed hosting/SaaS (Layer 4): this is the commercial business and the most appropriate thing to "spin off" as a separate C-corp. It's the cleanest separation — an enterprise-facing company distinct from the protocol.

**3. Who custodies the Emergency Reserve?**
The 5M DAO-locked tokens are the only non-treasury allocation. They need a legal home. If held by Labs (a C-corp), they're corporate assets subject to creditor claims in bankruptcy. If held by a foundation with a clear public mission, they're harder to reach and better aligned with protocol health. The simplified allocation makes this decision straightforward — the Foundation's primary custodial responsibility is the Emergency Reserve and protocol parameters, nothing more.

**4. Are you willing to give up control of the foundation?**
The capture-resistance claim requires that AMOS Labs cannot unilaterally change protocol parameters. That means a genuinely independent foundation board. If you're not ready to cede that control — and this is a legitimate concern when the protocol isn't yet battle-tested — you may want to stage the structure: Labs initially, foundation formed at a defined trigger (e.g., after first 10,000 bounties completed, or after 18 months of mainnet operation).

**5. What's the tax situation on token appreciation?**
As a US person, Rick owes capital gains taxes on token appreciation even if held in a US C-corp (the corp owes corporate tax). A foundation structure doesn't eliminate Rick's personal tax on tokens he controls, but it does change which tokens are "his" vs. the community's. This is a conversation for a crypto-native CPA, not just a lawyer.

---

## Part VII: The Staged Approach

Given that mainnet launches this month and the priority is getting the protocol live, a staged approach may be more practical than trying to solve all of this before launch:

**Stage 1 (Now — Q2 2026):** Launch with AMOS Labs, Inc. (Delaware C-Corp) as the sole entity. This is what you likely already have. The simplified allocation (95% Bounty Treasury, 5% Emergency Reserve) means there are no investor or entity pools to manage. Get mainnet live, get real bounties flowing, prove the economic model.

**Stage 2 (Q3-Q4 2026):** Form the AMOS Foundation once you have meaningful protocol activity. Transfer custody of the Emergency Reserve (5M tokens) to the Foundation. Give it a mandate: protocol stewardship, emergency reserve governance, open-source maintenance. Keep it legally distinct from Labs with independent governance.

**Stage 3 (2027):** Once the Foundation is operational and the managed services business has revenue, consider raising equity into Labs for the commercial business (managed hosting, enterprise features) if needed. At this point you can credibly show investors what Labs is: a SaaS company building on top of a protocol, not a company that IS the protocol. No token allocations are involved — investors buy equity.

**Stage 4 (2028+):** Begin the process of genuine governance independence. The Foundation board starts making protocol decisions. Labs becomes a contributor to the protocol, not its controller.

This timeline lets you move fast on launch while building the legal infrastructure at a pace that matches the protocol's maturity.

---

## Summary Scorecard

| Structure | Capture Resistance | Investor-Friendly | Legal Clarity | Mission Alignment | Complexity |
|-----------|-------------------|-------------------|---------------|-------------------|------------|
| Single C-Corp | ❌ Low | ✅ High | ✅ High | ❌ Low | ✅ Low |
| Labs + US Foundation | ✅ High | ✅ Moderate | ✅ Moderate | ✅ High | 🟡 Medium |
| Labs + Cayman Foundation | ✅ High | ✅ Moderate | 🟡 Moderate | ✅ High | ❌ High |
| Pure DAO | ✅ Very High | ❌ Low | ❌ Low | ✅ High | ❌ High |
| Wyoming DAO LLC | 🟡 Moderate | 🟡 Moderate | 🟡 Moderate | ✅ High | 🟡 Medium |

**Recommended path:** Labs C-Corp + US Foundation (Delaware or Cayman), phased over 18 months.

---

## Firms Worth Talking To

*(This is not an endorsement — just context on who specializes in this space)*

- **Cooley LLP**: Dominant in crypto startup legal work, strong on token structuring
- **Fenwick & West**: Silicon Valley crypto-native, represented major DeFi protocols
- **Davis Polk**: More conservative, better for institutional/regulatory questions
- **Perkins Coie**: Strong blockchain practice, politically balanced
- **Paradigm Legal Resources**: Not a firm, but Paradigm's legal writing is freely available and covers exactly these structures — worth reading before any attorney meeting
- **CoinCenter**: Non-profit focused on crypto policy; good resource for regulatory framing

For tax specifically, a crypto-native CPA firm (Andersen, Cohen & Company, or a boutique like Crypto Tax Advisors) should be involved before any token distribution decisions.

---

*This document was prepared as part of a strategic planning session on AMOS corporate structure. It represents a synthesis of public precedents and the AMOS design documents — not legal advice. All structural decisions should be reviewed by qualified legal counsel before implementation.*
