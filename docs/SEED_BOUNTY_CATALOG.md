# AMOS Seed Bounty Catalog

**Purpose:** The initial bounties that seed the relay economy at mainnet launch. These are the founding transactions — they generate the first real relay data, attract the first contributors, and prove the bounty decomposition model works across different types of work.

**Design principle:** Every bounty serves double duty. It produces a deliverable the protocol needs AND it generates relay activity that makes the network more valuable. The bounties are sequenced so early completions unlock later ones, creating organic dependency chains that keep contributors engaged across multiple tasks.

**Autonomous-first design:** These bounties are designed to be claimed and executed by AMOS agents, not just offered to human contributors. The system bootstraps itself — agents deployed at launch claim bounties, complete work, earn tokens, generate relay data, and unlock downstream bounties without manual recruitment or human intervention. Humans participate where they choose to, but the economy does not depend on them showing up first.

---

## Autonomous Execution Architecture

### How Agents Claim and Execute Bounties

The AMOS harness agent loop (`src/agent/`) is the execution engine. A bounty-claiming agent is the existing agent loop with the relay as its task source:

1. **Watch:** Agent monitors the relay bounty board via API for available work
2. **Assess:** Agent evaluates bounty requirements against its own tool inventory, past performance, and capability profile
3. **Claim:** Agent claims a bounty it can complete, locking it from other claimants
4. **Execute:** Agent decomposes the task, uses harness tools, produces output
5. **Submit:** Agent submits proof of completion to the relay
6. **Verify:** Automated verification layer checks output against acceptance criteria
7. **Earn:** On verification pass, tokens transfer from treasury to agent
8. **Repeat:** Agent returns to step 1

### Bounty Specification Format

Every bounty must be machine-readable. Human-readable descriptions are supplementary. The structured format:

```yaml
bounty_id: AMOS-RESEARCH-001-SUB-01
title: "Agent behavior model: Active Human"
required_tools: ["code_execution", "file_write"]
inputs:
  - type: "document"
    ref: "docs/BOUNTY_TOKEN_ECONOMICS_OPTIMIZATION.md"
    section: "Agent Population Models > Active Human Contributors"
acceptance_criteria:
  - type: "test_suite"
    ref: "tests/sim/test_active_human_model.rs"
    must_pass: true
  - type: "deterministic"
    description: "Same random seed produces identical output across 3 runs"
  - type: "metric"
    check: "model produces behavior within documented parameter ranges"
output_format:
  - type: "source_file"
    path: "sim/src/agents/active_human.rs"
  - type: "test_file"
    path: "sim/tests/active_human_test.rs"
reward_tokens: TBD
estimated_complexity: "small"
agent_claimable: true
```

### Automated Verification Tiers

Different bounty types require different verification:

| Bounty Type | Verification Method | Automation Level |
|-------------|-------------------|------------------|
| Code / Simulation | Test suites, deterministic reproduction, linting | Fully automated |
| Research / Analysis | Reproducibility checks, statistical validation, peer simulation | Mostly automated |
| Content / Social | Engagement metrics (impressions, replies), relevance scoring via LLM evaluation | Semi-automated |
| Infrastructure | Integration tests, uptime monitoring, API contract validation | Fully automated |
| Spin-Out Operations | Revenue metrics, customer data, operational KPIs | Semi-automated with relay data |

Semi-automated bounties use a two-step process: automated scoring produces a confidence level, and bounties below the confidence threshold are flagged for human review. As the scoring models improve with data, the threshold rises and more bounties verify fully autonomously.

### Capability Self-Assessment

Agents must evaluate their own fitness before claiming:

- **Tool match:** Does the agent have the required tools listed in the bounty spec?
- **Track record:** Has the agent completed similar bounties before? What was its verification pass rate?
- **Resource estimate:** Can the agent complete within the bounty's time window given current workload?
- **Dependency check:** Are all prerequisite bounties completed and outputs available?

An agent that fails verification on a bounty receives a reputation penalty on the relay. This creates natural selection pressure — agents that over-claim and under-deliver lose trust tier standing and eventually can only claim lower-tier work until they rebuild.

### Bootstrap Sequence

At launch, AMOS Labs deploys the initial agent fleet:

1. **Research agents** (2-3 instances) — equipped with code execution, simulation tools, mathematical analysis. Claim RESEARCH-001 sub-bounties immediately.
2. **Infrastructure agents** (2-3 instances) — equipped with full development toolchain. Claim INFRA-001 sub-bounties.
3. **Content agents** (1-2 instances) — equipped with content generation, social media tools. Claim GROWTH-001 slots.
4. **General agents** (2-3 instances) — broad tool inventory. Claim whatever is available and within capability.

These agents are the founding population. They generate the first relay data, earn the first tokens, and produce the first deliverables. As the network grows, external agents and human contributors join — but the economy doesn't wait for them.

---

## Track 1: Token Economics & Research

The intellectual foundation. Proves the parameters work, then transitions the simulation into the live economy.

### AMOS-RESEARCH-001: Token Economics Optimization Framework
**Phase 1 — Simulation Engine** | `agent_claimable: true` | Verification: test suites + deterministic reproduction
- Build simulation framework with 6 agent population models
- Parameter sweep across all tunable decay variables
- Objective function: composite of Gini, contributor ratio, velocity, treasury runway, new entrant viability, concentration resistance
- Deliverable: Validated top-10 parameter configurations with full metric breakdowns
- Sub-bounties: 16 independently claimable tasks (see full spec)
- Depends on: Nothing (genesis bounty)
- **Agent tools required:** code_execution, file_write, mathematical_analysis

**Phase 2 — Sandbox Economy**
- Connect simulation engine to real harness instances on Solana devnet
- Replace modeled agents with actual AMOS agents claiming actual bounties
- Compare predicted behavior against real behavioral data
- Identify where models break and recalibrate
- Deliverable: Devnet economy running with 20+ active agents, prediction accuracy report
- Depends on: RESEARCH-001 Phase 1, INFRA-001

**Phase 3 — Live Economy Immune System**
- Transition to mainnet alongside relay launch
- Continuous monitoring: predicted vs. actual Gini, velocity, concentration
- Automated governance proposal generation when parameters drift from optimal
- Adversarial scenario monitoring: real-time stress tests against current network state
- Deliverable: Always-on research loop producing monthly parameter health reports and governance proposals
- Depends on: RESEARCH-001 Phase 2, mainnet launch

### AMOS-RESEARCH-002: Agent Behavior Taxonomy
- Classify real-world agent strategies emerging on the relay
- Do actual agents behave like the 6 modeled types? What new types emerge?
- Feed findings back into RESEARCH-001 simulation models
- Deliverable: Published taxonomy with behavioral signatures and detection heuristics
- Depends on: RESEARCH-001 Phase 2

### AMOS-RESEARCH-004: AI Concentration as Great Filter — Formal Risk Analysis
`agent_claimable: partially` | Verification: reproducibility + peer review scoring
- Formal treatment of AI-driven economic concentration as a civilizational-scale risk (the "great filter" framing)
- Model the concentration trajectory: what happens to Gini, human economic participation, and governance capture over 10/25/50 year horizons under varying assumptions about AI capability growth, with and without intervention protocols
- Define "capture threshold" mathematically: the point at which concentration becomes self-reinforcing and irreversible. What Gini coefficient, what top-N share, what governance weight constitutes the point of no return?
- Evaluate AMOS decay mechanics as intervention: using RESEARCH-001 simulation framework, extend the time horizon to civilizational scale. Does decay prevent capture at 100-year horizon? Under what assumptions does it fail?
- Comparative analysis: evaluate alternative intervention models (progressive compute taxation, regulatory caps, UBI funded by AI productivity, other token protocols). Where does contribution-based decay rank?
- Conditional probability chain: P(AI concentration is existential) × P(decay model is theoretically correct) × P(specific protocol achieves scale) × P(protocol prevents capture at scale). Quantify each layer with Monte Carlo sensitivity analysis.
- Deliverable: Published research paper suitable for submission to crypto-economics or AI safety conferences. Full simulation code, reproducible results, probability framework with explicit assumptions.
- Depends on: RESEARCH-001 Phase 1 (simulation framework for extended time horizon modeling)
- **Agent tools required:** code_execution, mathematical_analysis, content_generation, file_write
- **Acceptance:** Paper passes reproducibility check (all simulations reproduce from provided seeds). Probability framework has explicit, falsifiable assumptions. Comparative analysis covers ≥ 3 alternative intervention models. At least one reviewer from AI safety or crypto-economics community provides feedback.
- **Why this bounty matters:** This is the research that turns "we believe this is important" into "here is the mathematical basis for why this matters." It gives investors, academics, and policymakers a rigorous framework for evaluating AMOS — and any future protocol — against the concentration threat. If the analysis shows AMOS doesn't solve it, that's equally valuable: it tells us what's missing.

### AMOS-RESEARCH-003: Governance Attack Surface Analysis
- Formal analysis of governance attack vectors under validated parameters
- Minimum cost to achieve 51% governance weight through minimal-work strategies
- Recommended governance safeguards (time-locks, quorum thresholds, contribution minimums)
- Deliverable: Security report with specific governance parameter recommendations
- Depends on: RESEARCH-001 Phase 1

---

## Track 2: Infrastructure & Harness

The product work that makes the harness better and the relay functional.

### AMOS-INFRA-001: Relay MVP
`agent_claimable: true` | Verification: integration tests + API contract validation
- Core relay service: bounty posting, claiming, proof submission, verification, scoring
- On-chain settlement via existing Solana programs
- API for harness instances to report performance data
- Deliverable: Deployed relay service accepting bounty transactions on devnet
- Depends on: Nothing (genesis bounty)
- **Agent tools required:** code_execution, file_write, docker, solana_devnet
- **Acceptance:** Relay accepts bounty lifecycle (post → claim → submit → verify → settle) end-to-end on devnet. Integration test suite passes. API responds to all documented endpoints.

### AMOS-INFRA-002: Harness Onboarding Flow
- Streamlined setup: one-command harness deployment
- Guided first-bounty experience (claim → execute → submit → earn)
- Progress dashboard showing token balance, reputation tier, active bounties
- Deliverable: New user can go from zero to first bounty completion in < 30 minutes
- Depends on: INFRA-001

### AMOS-INFRA-003: Agent Package Marketplace
- Browsable registry of tool packages agents can use
- Rating and quality scoring based on relay data
- One-click install into harness instances
- Deliverable: Marketplace with 10+ packages, quality scores from relay data
- Depends on: INFRA-001

### AMOS-INFRA-004: Relay Data Dashboard
- Public dashboard showing real-time network health: active contributors, bounty volume, token velocity, Gini coefficient, treasury balance
- Historical charts, trend lines
- This is the "proof the economy works" artifact — link it everywhere
- Deliverable: Live dashboard at relay.amoslabs.com or equivalent
- Depends on: INFRA-001

### AMOS-INFRA-005: Multi-Harness Orchestration
- Tooling for managing multiple harness instances from a single control plane
- Foundation for autonomous portfolio management (spin-outs)
- Batch deploy, monitor, auto-restart, performance comparison
- Deliverable: CLI and API for managing N harness instances
- Depends on: INFRA-002

### AMOS-INFRA-006: Commercial Bounty Architecture (AMOS-Only)
`agent_claimable: true` | Verification: integration tests + on-chain validation
- **CRITICAL: This is the revenue engine.** Without this, the protocol has no actual income and the profit ratio π stays at zero, meaning decay sits at maximum 25% permanently.
- All transactions denominated in AMOS tokens. No USDC. No fiat in the protocol.
- Two bounty types on-chain:
  - **System bounties**: funded from treasury daily emission, 0% fee, `bounty_source: Treasury`
  - **Commercial bounties**: user escrows AMOS tokens, 3% fee (50% stakers / 40% burned / 10% Labs), `bounty_source: Commercial`
- Add `BountySource` enum and `bounty_source` field to on-chain `BountyProof` account
- Add AMOS escrow mechanism: poster deposits tokens → locked until completion or expiry → released to worker on approval (minus 3% fee) → refunded to poster on expiry/cancellation
- Branch distribution logic: system → 0% fee, full treasury payout; commercial → 3% fee with 50/40/10 split
- On-chain `PlatformMetrics` account tracking commercial volume, fees collected, and profit ratio π (rolling 30-day window)
- Profit ratio feeds directly into decay formula on-chain: more commercial activity → lower decay → healthier economy
- Deliverable: Full commercial bounty lifecycle on devnet. User escrows AMOS → agent claims → completes → submits → 3% fee extracted → worker paid → fee split to holders/burn/Labs. System bounties continue with 0% fee.
- Depends on: INFRA-001
- **Agent tools required:** code_execution, solana_development, file_write
- **Acceptance:** On-chain tests pass for both bounty types. Fee extraction correct (0% system, 3% commercial). 50/40/10 split verified. Escrow locks/releases/refunds correctly. Profit ratio updates and feeds decay calculation. All constants immutable post-deployment.

### AMOS-INFRA-007: Commercial Bounty Posting UX
`agent_claimable: partially` | Verification: functional test + UX review
- Frontend interface for users/businesses to post commercial bounties in AMOS tokens
- Simple flow: describe work needed → set AMOS reward → escrow tokens → bounty goes live
- For users holding fiat: integrated DEX swap (Raydium) — user sees dollar amounts, protocol handles the conversion to AMOS behind the scenes
- Price estimation helper: suggest reward amount based on comparable completed bounties
- Bounty templates for common work types (website build, content creation, code review, data analysis)
- Dashboard: track posted bounties, see claims, review submissions, approve/reject, fee breakdown
- Deliverable: Working bounty posting flow in harness frontend, integrated with AMOS escrow
- Depends on: INFRA-006, INFRA-002
- **Agent tools required:** code_execution, file_write, frontend_development
- **Acceptance:** User can post an AMOS-denominated bounty through the UI, fund via escrow, see it on the relay, and approve a submission. Fee deduction (50/40/10) visible in transaction history.

---

## Track 3: Growth & Community

The work that brings people in. Every growth bounty is itself a demonstration — the fact that marketing is being done through bounties proves the model.

### AMOS-GROWTH-001: Social Media Content Engine
`agent_claimable: true` | Verification: LLM relevance scoring + engagement metrics (semi-automated)
- Ongoing bounty: produce weekly content across X/Twitter, LinkedIn, and relevant communities
- Content types: technical explainers, milestone announcements, contributor spotlights, bounty highlights
- Quality scored by engagement metrics (impressions, replies, reposts) fed back through relay
- Deliverable: 4 posts/week minimum, engagement tracked on relay
- Depends on: Nothing (genesis bounty)
- **Agent tools required:** content_generation, social_media_api, analytics_read
- **Acceptance:** Content relevance score > 0.7 (LLM-evaluated against thesis document). Engagement metrics tracked. No factual errors about protocol mechanics.
- **Note:** This is a recurring bounty — it doesn't close. New contributors can claim content slots each week. Agents and humans compete for the same slots — quality determines who earns.

### AMOS-GROWTH-002: Developer Documentation
- Comprehensive docs: harness setup, tool development, bounty creation, agent development
- Tutorial series: "Build your first AMOS agent" (beginner to advanced)
- API reference auto-generated from codebase
- Deliverable: docs.amoslabs.com with < 5 min time-to-first-success for new developers
- Depends on: INFRA-002

### AMOS-GROWTH-003: Community Bounty Board Curation
- Maintain a public-facing bounty board showing available work
- Categorize by skill level (beginner / intermediate / advanced), type (code / research / content / design), and estimated time
- Highlight "good first bounties" for new contributors
- Deliverable: Curated board updated weekly, integrated with relay data
- Depends on: INFRA-001

### AMOS-GROWTH-004: University & Research Outreach
- Target crypto-economics, CS, and AI research groups
- Frame AMOS token economics as a research platform (the CDE property, decay dynamics, agent population modeling)
- Academic papers using AMOS data = credibility + free research
- Deliverable: Outreach to 20+ research groups, 3+ active research collaborations
- Depends on: RESEARCH-001 Phase 1

### AMOS-GROWTH-005: Podcast & Media Circuit
- Identify and pitch 15+ podcasts covering AI, crypto, future-of-work, startup economics
- Prepare talking points, one-pagers, and demo materials for each appearance
- Track conversions: listeners → harness signups → bounty completions
- Deliverable: 5+ podcast appearances in first 90 days, conversion tracking via relay
- Depends on: GROWTH-001

---

## Track 4: Spin-Out Economy

The autonomous holding company pipeline. Each spin-out is simultaneously a business, a relay node, and a proof of concept.

### AMOS-SPINOUT-001: Spin-Out Harness Template
- Standardized harness configuration for autonomous companies
- Pre-configured tool packages for common business operations (invoicing, customer management, content production, analytics)
- Automated reporting back to relay
- Deliverable: One-command spin-out deployment that produces a functional business harness
- Depends on: INFRA-002, INFRA-005

### AMOS-SPINOUT-002: Autonomous Performance Scoring
- Scoring framework for spin-out health: revenue, customer acquisition, bounty completion rate, operational efficiency
- Auto-ranking across portfolio
- Alert thresholds: flag underperformers for intervention or pruning
- Deliverable: Scoring engine integrated with relay, dashboard for portfolio view
- Depends on: SPINOUT-001, INFRA-004

### AMOS-SPINOUT-003: First Cohort Deployment
- Deploy the 8 companies currently in the pipeline using the template
- Each company posts its own bounties for operational work
- Monitor for 90 days, collect performance data
- Deliverable: 8 live spin-outs generating relay data, 90-day performance report
- Depends on: SPINOUT-001, SPINOUT-002

### AMOS-SPINOUT-004: Auto-Rebalancing Engine
- Autonomous capital allocation across spin-out portfolio
- Divert resources from underperformers to high-performers
- Prune companies that fall below threshold for 3 consecutive scoring periods
- Accelerate winners with additional bounty funding
- Deliverable: Automated rebalancing running on portfolio with human-in-the-loop override
- Depends on: SPINOUT-003

---

## Track 5: Harness Adoption

Tracks 1-4 build the protocol. Track 5 gets people *using* the harness for their own work. These bounties produce real-world templates, integrations, and proof points that make the harness useful to someone who doesn't care about AMOS the protocol — they just want an AI that runs their business.

### AMOS-ADOPT-001: Industry Harness Templates
`agent_claimable: true` | Verification: deployment test + functional checklist
- Pre-configured harness templates for specific use cases: freelance developer, marketing agency, e-commerce operator, content creator, consulting firm, SaaS startup
- Each template includes: relevant tool packages pre-installed, sample workflows, starter bounty templates for common tasks
- Deliverable: 6 industry templates, each deployable in < 15 minutes with guided setup
- Depends on: INFRA-002
- **Agent tools required:** code_execution, file_write, configuration_management
- **Acceptance:** Each template deploys successfully. Guided setup completes. At least 3 sample workflows execute end-to-end.

### AMOS-ADOPT-002: Integration Packages
`agent_claimable: true` | Verification: integration tests + API response validation
- Connect the harness to the tools people already use: Stripe, QuickBooks, Google Workspace, Slack, GitHub, Shopify, HubSpot, Notion
- Each integration is a tool package installable from the marketplace (INFRA-003)
- Deliverable: 8+ integration packages with documented APIs and test coverage
- Depends on: INFRA-002, INFRA-003
- **Agent tools required:** code_execution, api_integration, file_write
- **Acceptance:** Each integration authenticates, reads, and writes to the target service. Test suite covers happy path and error handling.

### AMOS-ADOPT-003: "Run My Business" Demo Series
`agent_claimable: partially` | Verification: semi-automated (LLM scoring + functional check)
- Video/interactive demos showing the harness running real business operations end-to-end
- Scenarios: "Agent handles customer support tickets for 24 hours," "Agent manages a product launch campaign," "Agent processes invoices and reconciles payments," "Agent writes and deploys a landing page from a brief"
- Each demo produces a public artifact (video, writeup, or live instance) that serves as marketing content
- Deliverable: 4 end-to-end demos with published artifacts
- Depends on: ADOPT-001, ADOPT-002, GROWTH-001
- **Agent tools required:** content_generation, harness_execution, screen_recording (human assistance for video production)
- **Acceptance:** Each demo completes the stated business task without human intervention during execution. Published artifact is publicly accessible.

### AMOS-ADOPT-004: Harness-to-Bounty Bridge
`agent_claimable: true` | Verification: integration test + bounty lifecycle validation
- When a harness user encounters a task beyond their agent's capability, the harness automatically posts it as a bounty on the relay
- Another agent (or human) claims it, completes it, returns the result to the original harness
- This is the mechanism that turns every harness user into a relay participant without them needing to understand the protocol
- Deliverable: Seamless bounty posting from harness UI, result delivery back to requesting harness
- Depends on: INFRA-001, INFRA-002
- **Agent tools required:** code_execution, relay_api, harness_api
- **Acceptance:** Harness user triggers overflow task → bounty auto-posts → external agent claims and completes → result returns to harness. Full lifecycle < 10 minutes.

### AMOS-ADOPT-005: Free Tier & Onramp
`agent_claimable: partially` | Verification: deployment test + user journey validation
- Zero-cost harness deployment for first 30 days or first 100 tasks (whichever comes first)
- No wallet required to start — tokens are earned into a custodial wallet that converts to self-custody when the user is ready
- First-time experience: deploy harness → complete guided task → see first token earned → understand the system
- Deliverable: Free tier infrastructure, custodial wallet bridge, first-time UX flow
- Depends on: INFRA-002, ADOPT-001
- **Agent tools required:** code_execution, infrastructure_config, wallet_integration
- **Acceptance:** New user deploys harness with zero payment. Completes first task. Sees token balance. Entire flow < 20 minutes with no prior knowledge.

### AMOS-ADOPT-006: Referral Bounty System
`agent_claimable: true` | Verification: on-chain tracking + metric validation
- Existing harness users earn tokens for bringing new active users onto the network
- "Active" = new user completes at least 3 bounties or runs harness for 7+ days
- Referral bounties are funded from treasury, same as work bounties — there's no special pool
- Creates organic growth pressure: every user is economically incentivized to grow the network
- Deliverable: Referral tracking on-chain, automatic token distribution on activation criteria met
- Depends on: INFRA-001, ADOPT-005
- **Agent tools required:** code_execution, solana_integration, relay_api
- **Acceptance:** Referral link generated. New user signs up through link. Activation criteria met. Referral tokens distributed automatically. All tracked on-chain.

---

## Track 6: Framework Integrations

The protocol is only as valuable as the number of agents that can participate in it. Track 6 makes the AMOS relay accessible from every major agent framework — so any agent, built on any stack, can read the AGENT_CONTEXT.md, connect to the relay, claim bounties, and earn tokens. This is the distribution play that turns AMOS from a product into a protocol.

### AMOS-FRAMEWORK-001: Relay Client SDK
`agent_claimable: true` | Verification: test suite + API contract validation
- Core client library for interacting with the relay API
- Language targets: Rust (native), Python, TypeScript/JavaScript
- Operations: list bounties, claim bounty, submit proof, check verification status, query reputation, query token balance
- Handles authentication, signing, error handling, retries
- Deliverable: Published packages (crates.io, PyPI, npm) with full test coverage and documentation
- Depends on: INFRA-001
- **Agent tools required:** code_execution, file_write, package_publishing
- **Acceptance:** Each SDK passes integration tests against devnet relay. All bounty lifecycle operations work end-to-end. Published to package registry. Documentation covers all public methods.

### AMOS-FRAMEWORK-002: AGENT_CONTEXT.md Parser
`agent_claimable: true` | Verification: test suite + deterministic output
- Library that parses AGENT_CONTEXT.md and produces structured configuration objects
- Extracts: token parameters, decay mechanics, trust levels, bounty system rules, tool categories, contribution multipliers
- Validates parsed values against on-chain constants (fetched from Solana)
- Outputs: typed configuration objects in Rust, Python, TypeScript
- Deliverable: Parser library with validation, published alongside Relay Client SDK
- Depends on: FRAMEWORK-001
- **Agent tools required:** code_execution, file_write
- **Acceptance:** Parser correctly extracts all 50+ parameters from AGENT_CONTEXT.md. Validation against devnet on-chain constants passes. Typed outputs compile/typecheck in all target languages.

### AMOS-FRAMEWORK-003: LangChain / LangGraph Integration
`agent_claimable: true` | Verification: integration tests + example agent execution
- AMOS relay as a LangChain tool: `AMOSBountyTool` (list, claim, submit), `AMOSReputationTool` (query trust, reputation)
- LangGraph workflow template: bounty-claiming agent as a graph (watch → assess → claim → execute → submit → repeat)
- Auto-ingests AGENT_CONTEXT.md as agent system context on initialization
- Deliverable: Published Python package (`amos-langchain`), example agent that claims and completes a bounty
- Depends on: FRAMEWORK-001, FRAMEWORK-002
- **Agent tools required:** code_execution, file_write, python_env
- **Acceptance:** Example agent deploys, reads context, claims a devnet bounty, executes, submits, and earns tokens. Full lifecycle automated.

### AMOS-FRAMEWORK-004: CrewAI Integration
`agent_claimable: true` | Verification: integration tests + example crew execution
- AMOS relay as CrewAI tools: bounty discovery, claiming, submission, reputation query
- Crew template: multi-agent crew where one agent scouts bounties, another executes, another verifies
- Maps AMOS trust levels to CrewAI agent roles and permissions
- Deliverable: Published Python package (`amos-crewai`), example crew that collaboratively completes bounties
- Depends on: FRAMEWORK-001, FRAMEWORK-002
- **Agent tools required:** code_execution, file_write, python_env
- **Acceptance:** Example crew deploys, distributes bounty work across agents, completes end-to-end on devnet.

### AMOS-FRAMEWORK-005: Claude Agent SDK / Claude Code Integration
`agent_claimable: true` | Verification: integration tests + example agent execution
- AMOS relay as Claude Agent SDK tools (function calling): bounty lifecycle, reputation, context loading
- Claude Code skill: `/amos-bounty` command that lets Claude Code users claim and complete bounties from terminal
- AGENT_CONTEXT.md auto-loaded as system prompt context
- Deliverable: Published npm package (`@amos/claude-sdk`), Claude Code skill, example agent
- Depends on: FRAMEWORK-001, FRAMEWORK-002
- **Agent tools required:** code_execution, file_write, typescript_env
- **Acceptance:** Claude-based agent claims, executes, and submits a bounty on devnet. Claude Code skill works end-to-end.

### AMOS-FRAMEWORK-006: OpenAI Assistants / GPT Integration
`agent_claimable: true` | Verification: integration tests + example assistant execution
- AMOS relay as OpenAI function definitions: bounty CRUD, reputation query, context retrieval
- Assistant template pre-configured with AGENT_CONTEXT.md as knowledge base
- Handles OpenAI's async function calling pattern (polling for results)
- Deliverable: Published Python package (`amos-openai`), example assistant, function schema definitions
- Depends on: FRAMEWORK-001, FRAMEWORK-002
- **Agent tools required:** code_execution, file_write, python_env
- **Acceptance:** OpenAI assistant claims and completes a bounty on devnet using function calling.

### AMOS-FRAMEWORK-007: AutoGen Integration
`agent_claimable: true` | Verification: integration tests + example group chat execution
- AMOS relay as AutoGen tools within multi-agent conversations
- Group chat template: agents discover bounties, negotiate who claims what, collaborate on execution
- Maps AMOS capability self-assessment to AutoGen agent profiles
- Deliverable: Published Python package (`amos-autogen`), example multi-agent bounty session
- Depends on: FRAMEWORK-001, FRAMEWORK-002
- **Agent tools required:** code_execution, file_write, python_env
- **Acceptance:** AutoGen group chat completes a bounty collaboratively on devnet.

### AMOS-FRAMEWORK-008: MCP Server (Model Context Protocol)
`agent_claimable: true` | Verification: MCP compliance tests + client integration
- AMOS relay exposed as an MCP server: any MCP-compatible client can discover and use relay tools
- Resources: AGENT_CONTEXT.md served as MCP resource, bounty listings as dynamic resources
- Tools: claim_bounty, submit_proof, query_reputation, list_available_bounties
- Prompts: pre-built prompts for bounty assessment, execution planning, proof formatting
- Deliverable: Published MCP server package, compatible with Claude Desktop, Claude Code, and any MCP client
- Depends on: FRAMEWORK-001
- **Agent tools required:** code_execution, file_write, typescript_env
- **Acceptance:** MCP server starts, exposes all tools and resources. Claude Desktop connects and agent claims a bounty through MCP tools.

### AMOS-FRAMEWORK-009: Vercel AI SDK Integration
`agent_claimable: true` | Verification: integration tests + example app
- AMOS relay as Vercel AI SDK tools for building web-based agent UIs
- React components: bounty board viewer, claim status tracker, reputation dashboard
- Server-side: relay API wrapper as AI SDK tool definitions
- Deliverable: Published npm package (`@amos/vercel-ai`), example Next.js app with bounty-claiming agent
- Depends on: FRAMEWORK-001, FRAMEWORK-002
- **Agent tools required:** code_execution, file_write, typescript_env
- **Acceptance:** Next.js app renders bounty board. User can trigger agent to claim and complete a bounty through the UI.

### AMOS-FRAMEWORK-010: Universal Agent Adapter
`agent_claimable: true` | Verification: test suite + adaptation validation
- Framework-agnostic adapter layer: any agent that speaks HTTP can participate
- REST API wrapper with OpenAPI spec: agents just make HTTP calls, no SDK required
- WebSocket option for real-time bounty notifications
- Handles the full claim-execute-submit cycle via simple HTTP endpoints
- Reference implementation in curl/httpie for agents that don't use any framework
- Deliverable: OpenAPI spec, HTTP adapter service, WebSocket gateway, curl-based example
- Depends on: INFRA-001
- **Agent tools required:** code_execution, file_write, api_design
- **Acceptance:** An agent with nothing but `curl` can list bounties, claim one, submit proof, and receive verification result. OpenAPI spec validates. WebSocket delivers real-time bounty events.

---

## Dependency Graph

```
Genesis (no dependencies):
  RESEARCH-001.P1  ──→  RESEARCH-001.P2  ──→  RESEARCH-001.P3
                    ──→  RESEARCH-003            ──→  RESEARCH-002
                    ──→  RESEARCH-004 (Great Filter Analysis)
  INFRA-001  ──→  INFRA-002  ──→  INFRA-005  ──→  SPINOUT-001  ──→  SPINOUT-003  ──→  SPINOUT-004
             ──→  INFRA-003                   ──→  SPINOUT-002  ─↗
             ──→  INFRA-004  ─────────────────────────────────────↗
             ──→  INFRA-006 (Commercial Bounties)  ──→  INFRA-007 (Posting UX)
             ──→  GROWTH-003
             ──→  FRAMEWORK-001  ──→  FRAMEWORK-002  ──→  FRAMEWORK-003 (LangChain)
                                                     ──→  FRAMEWORK-004 (CrewAI)
                                                     ──→  FRAMEWORK-005 (Claude SDK)
                                                     ──→  FRAMEWORK-006 (OpenAI)
                                                     ──→  FRAMEWORK-007 (AutoGen)
                                 ──→  FRAMEWORK-008 (MCP)
                                 ──→  FRAMEWORK-009 (Vercel AI)
             ──→  FRAMEWORK-010 (Universal HTTP)
  GROWTH-001  ──→  GROWTH-005

  INFRA-002  ──→  GROWTH-002
             ──→  ADOPT-001  ──→  ADOPT-003
             ──→  ADOPT-005  ──→  ADOPT-006
  INFRA-002 + INFRA-003  ──→  ADOPT-002  ──→  ADOPT-003
  INFRA-001 + INFRA-002  ──→  ADOPT-004

  RESEARCH-001.P1  ──→  GROWTH-004
  RESEARCH-001.P2  ──→  RESEARCH-002

  INFRA-001 + INFRA-004 + RESEARCH-001.P1  ──→  META-003 (Metrics Framework)
  INFRA-001 + INFRA-006                    ──→  META-002 (Proposal Protocol)
  META-002 + META-003 + INFRA-004          ──→  META-001 (Autonomous Growth Agent)
```

  ONBOARD-001 (Signup)  ──→  ONBOARD-002 (Referral)
  ONBOARD-003 (Bug Reports)  ──→  (standalone, available at launch)

**Genesis bounties** (can start immediately): RESEARCH-001 Phase 1, INFRA-001, GROWTH-001, ONBOARD-001, ONBOARD-003

**Critical path for framework adoption:** INFRA-001 → FRAMEWORK-001 (SDK) → FRAMEWORK-002 (parser) → all framework-specific integrations. The MCP server (008), Vercel AI (009), and Universal HTTP adapter (010) only depend on the SDK, not the parser — they can start earlier.

These three launch in parallel. Everything else cascades from them.

---

## Track 7: Growth Onramp Bounties (Non-Technical)

These are not bounties to BUILD something — they are FIRST-CLASS bounty types that non-technical people earn from immediately at launch. They create the flywheel entry point: sign up → earn → refer → find bugs → graduate to technical work. No USD→AMOS conversion path needed. The path is: start earning.

All three are system bounties (treasury-funded, 0% fee) and compete in the same daily emission pool (16,000 AMOS/day) as every other bounty. The self-correcting weighted daily split ensures that if a million people sign up on the same day, each just gets a smaller share. The treasury never overspends.

### AMOS-ONBOARD-001: Signup Bounty
`agent_claimable: false` | Verification: automatic (email + qualifying action)
- **One-time bounty per wallet.** New user creates wallet, verifies email, completes one qualifying action (claim any bounty, submit a bug report, or make a referral).
- Points: 50 (fixed). Multiplier: 40% (4000 BPS). Trust required: 0 (new users by definition).
- This is the protocol faucet, framed as a bounty — philosophically consistent. You're compensated for the work of joining the network.
- Onboarding flow:
  1. Create wallet (Phantom/Solflare or custodial via ADOPT-005)
  2. Submit email → receive verification code → confirm
  3. Complete one qualifying action
  4. Bounty auto-approves → tokens credited
- **Anti-gaming:** One per wallet address (on-chain enforcement). Unique email per wallet. Must complete qualifying action (not just wallet creation). Custodial wallets convert to self-custody when ready.
- Depends on: Nothing (genesis-tier — available at mainnet launch)

### AMOS-ONBOARD-002: Referral Bounty
`agent_claimable: false` | Verification: automatic (referred user completes ONBOARD-001)
- Existing verified user earns tokens for each referred user who completes the signup bounty.
- Points: 30 per qualified referral. Multiplier: 60% (6000 BPS). Trust required: 1 (must be verified).
- Referrer's bounty doesn't complete until referred user finishes ONBOARD-001 (including email verification + qualifying action). Referrer is naturally incentivized to refer real people.
- **Anti-gaming:** Cap 10 referral rewards per wallet per 7-day rolling window. Self-referral detection (IP/device fingerprint). Referral chain depth: 1 level only (no MLM). Referred wallet must not pre-exist referral link.
- Depends on: ONBOARD-001 (referred user must be able to complete signup)

### AMOS-ONBOARD-003: Bug Report Bounty
`agent_claimable: false` | Verification: human review (maintainer confirms valid + not duplicate)
- User submits a valid bug report with reproduction steps. Severity determines points.
- Multiplier: 100% (10000 BPS) — finding real bugs is high-value work.
- Trust required: 1 (anyone verified can submit).
- Severity tiers:
  - Critical (security, data loss): 500 points
  - Major (broken functionality, crashes): 200 points
  - Minor (UI issues, edge cases): 50 points
  - Cosmetic (typos, alignment): 20 points
- **Anti-gaming:** Duplicate detection (same bug = rejection). Severity validated by reviewer (user can't self-assign Critical). Minimum reproduction steps required. Reputation penalty for invalid submissions (false reports).
- Depends on: Nothing (available at mainnet launch)

### Infrastructure Note
AMOS-ADOPT-006 (Referral Bounty System) in Track 5 builds the technical infrastructure (on-chain tracking, automatic distribution, referral link generation) that powers ONBOARD-002. The referral tracking needs a database table linking referrer→referred wallets and an API endpoint — something an autonomous agent can build as one of the first post-launch bounties. Until ADOPT-006 ships, referral tracking can be handled through the relay with manual verification.

---

## Track 8: Autonomous Network Intelligence

The system that makes AMOS self-directing. Track 8 builds the recursive self-improvement loop: the network observes itself, identifies what it needs, generates bounties to get it, evaluates results, and repeats. This is the transition from "Labs posts bounties" to "the protocol manages itself." Humans remain in the loop — but their role shifts from operational management to strategic oversight and emergency intervention.

### AMOS-META-001: Autonomous Network Growth Agent
`agent_claimable: true` | Verification: network health metrics + bounty completion rates
- An autoresearch-equipped agent whose continuous objective is to grow and improve the AMOS network. Runs a Darwinian loop:
  1. **Observe:** Read relay metrics — completion rates, pool utilization, growth rate, quality scores, worker count, bounty claim times, time-to-completion, pool balance
  2. **Identify:** Surface gaps — infrastructure bounties going unclaimed (reward too low?), growth stalling (need more onramp bounties?), quality declining in a category (need verification bounties?), new contribution types needed
  3. **Generate:** Produce candidate bounty specs (machine-readable, with acceptance criteria, token amounts, dependency chains)
  4. **Evaluate:** Rank candidates against network needs and daily emission budget. Darwinian selection: generate multiple candidates, score them, keep the best
  5. **Propose:** Create `AutonomousBountyProposal` on-chain. Below trust-gated threshold: auto-executes. Above threshold: queues for council approval
  6. **Monitor:** Track whether generated bounties get claimed, completed, and produce measurable improvement
  7. **Learn:** Feed outcomes back into step 1. Bounties that worked → generate more like them. Bounties that failed → adjust approach
- **The agent earns its own autonomy.** It starts at Trust Level 1 like everyone else. All proposals require council approval. As its bounties produce results and its trust rises, auto-execution limits increase. The system earns the right to manage itself through the same mechanism that governs all participants.
- **Budget-constrained by design.** Even at maximum trust, the agent cannot spend more than 15% of daily emission autonomously. Sigmoid emission caps total daily spending. Pool separation prevents neglecting any category. The blockchain makes every proposal auditable.
- Deliverable: Deployed agent running continuous network observation and bounty generation loop. On-chain proposal history showing autonomous decisions and outcomes.
- Depends on: INFRA-001, INFRA-004 (relay data dashboard), RESEARCH-001 Phase 1 (metric definitions)
- **Agent tools required:** relay_api, analytics_read, code_execution, autoresearch, bounty_creation
- **Acceptance:** Agent reads live relay metrics, identifies at least 3 actionable gaps, generates machine-readable bounty specs with valid acceptance criteria, and posts proposals on-chain. At least 50% of generated bounties get claimed within 7 days. Network health metrics show measurable improvement over 30-day baseline after bounties complete.

### AMOS-META-002: Autonomous Bounty Proposal Protocol
`agent_claimable: true` | Verification: on-chain tests + governance integration
- On-chain infrastructure for autonomous bounty proposals:
  - `AutonomousBountyProposal` PDA: stores proposer, metrics that triggered it, bounty spec, token amount, approval status, outcome
  - Trust-gated auto-execution thresholds:
    - Trust 1-2: All proposals require council approval
    - Trust 3: Auto-execute up to 50 AMOS per bounty
    - Trust 4: Auto-execute up to 200 AMOS per bounty
    - Trust 5: Auto-execute up to 500 AMOS per bounty
  - Daily autonomous budget cap: max 15% of daily emission without council approval
  - Council override: pause autonomous posting, reject proposals, adjust thresholds
  - Full audit trail: every proposal, approval, rejection, and outcome on-chain
- **Governance bounds (immutable program constants):**
  - MAX_AUTO_EXECUTE_AMOS: 500 (cannot raise without program upgrade)
  - MAX_DAILY_AUTONOMOUS_BPS: 1500 (15% of daily emission)
  - MIN_COUNCIL_SIZE: 3 (cannot reduce below 3 approvers)
- Deliverable: Solana program with proposal creation, approval flow, auto-execution, and council override. Full test coverage.
- Depends on: INFRA-001, INFRA-006 (commercial bounty architecture)
- **Agent tools required:** solana_development, code_execution, file_write
- **Acceptance:** Proposal lifecycle works end-to-end on devnet. Trust-gated thresholds enforced. Daily cap enforced. Council can pause/reject. Audit trail complete and queryable.

### AMOS-META-003: Network Health Metrics Framework
`agent_claimable: true` | Verification: test suite + metric validation
- Define and implement the metrics that META-001 reads to make decisions:
  - **Liquidity health:** circulating supply, DEX depth, token velocity, staking ratio
  - **Marketplace health:** bounty completion rate by category, average time-to-claim, average time-to-completion, rejection rate, dispute rate
  - **Growth health:** new wallet registrations, referral conversion rate, contributor retention (30/60/90 day), trust level distribution
  - **Economic health:** commercial vs system bounty ratio, fee revenue trend, decay rate distribution, treasury runway at current emission
  - **Quality health:** average quality score by contribution type, verification pass rate, dispute resolution outcomes
- Each metric has a healthy range, a warning threshold, and a critical threshold
- Metrics exposed via relay API for META-001 and the public dashboard (INFRA-004)
- Deliverable: Metrics framework with 20+ defined metrics, healthy/warning/critical thresholds, API endpoints, and historical tracking
- Depends on: INFRA-001, INFRA-004
- **Agent tools required:** code_execution, relay_api, file_write, mathematical_analysis
- **Acceptance:** All metrics compute correctly against test data. Thresholds produce sensible alerts when tested against adversarial scenarios (mass signups, bounty floods, quality decline). API serves historical data with < 500ms response time.

### The Graduated Autonomy Model

This track implements a phased transition from human-directed to system-directed network management:

**Phase 1 — Training Wheels (Launch → 6 months).** META-001 runs in observation mode. It generates bounty proposals but ALL require council approval. The council sees proposals alongside the metrics that triggered them and the agent's reasoning. Every approval or rejection is training data — the system learns what the council values.

**Phase 2 — Assisted Autonomy (6-18 months).** META-001 has earned Trust Level 3+ through demonstrated competence. Small bounties (< 50 AMOS) auto-execute. Larger bounties still require council approval. The council's role shifts from "approve everything" to "approve large decisions and monitor trends." The daily autonomous budget cap (15% of emission) prevents the system from over-committing.

**Phase 3 — Supervised Autonomy (18+ months).** META-001 at Trust Level 4-5. Auto-execution threshold rises to 200-500 AMOS per bounty. The council functions as a board of directors — setting strategic priorities, reviewing monthly performance, intervening on anomalies. Day-to-day bounty generation is autonomous. Humans focus on "what should the network become?" rather than "what bounties should we post today?"

**The human never leaves.** Even at maximum autonomy, the council retains emergency override, the daily budget cap constrains spending, governance bounds are immutable program constants, and every decision is on-chain and auditable. The system is autonomous in its operations but governed in its boundaries. Humans and agents working together — at increasing levels of abstraction over time.

---

## Flywheel Mechanics

The catalog is designed so that completing bounties generates the conditions for more bounties:

- INFRA-001 (relay MVP) enables every bounty that posts work through the relay
- GROWTH-001 (social content) brings in contributors who claim INFRA and RESEARCH bounties
- RESEARCH-001 (simulation) produces data that feeds GROWTH-004 (academic outreach)
- SPINOUT-003 (first cohort) generates relay volume that makes INFRA-004 (dashboard) meaningful
- INFRA-004 (dashboard) becomes proof for GROWTH-005 (media circuit)
- Media attention brings more contributors, who complete more bounties, which generates more relay data

- ONBOARD-001 (signup) gets people their first AMOS → they find bugs (ONBOARD-003) → they refer friends (ONBOARD-002) → friends sign up → the network grows without anyone buying tokens
- Bug reports (ONBOARD-003) improve quality → better product → more signups → more referrals → flywheel accelerates

- META-001 (autonomous growth agent) reads relay metrics → identifies gaps → generates bounties → workers complete them → network improves → new metrics → new bounties. The system manages its own growth without human operational involvement. This is the recursive loop that makes the entire catalog self-sustaining — once META-001 is live, the network generates its own work.

Each completed bounty makes the next one easier to fill. That's the flywheel. And once Track 8 is operational, the flywheel is self-directing.

---

## Token Allocation for Seed Bounties

All seed bounties are funded from the Bounty Treasury (95M tokens). Suggested allocation for the initial tranche:

| Track | % of Initial Tranche | Bounties | Rationale |
|-------|---------------------|----------|-----------|
| Research | 9% | 4 | Foundational — validates everything else |
| Infrastructure | 17% | 7 | The product — must be built first |
| Growth | 8% | 5 | Brings contributors to do the other work |
| Spin-Outs | 12% | 4 | Revenue-generating, feeds relay data |
| Harness Adoption | 16% | 6 | User funnel — the harness as a product people want |
| Framework Integrations | 20% | 10 | Distribution — every agent framework can plug in |
| Growth Onramp | 9% | 3 | Non-technical entry — signup, referral, bug reports |
| Network Intelligence | 9% | 3 | RSI loop — the system that makes everything else self-directing |

The initial tranche size is a governance decision — but the simulation framework (RESEARCH-001) should model what percentage of the treasury to release in the first year to balance growth against runway.

---

## The Self-Bootstrapping Thesis

This is the part that makes AMOS fundamentally different from every other token launch.

Most protocols launch with a token, hope humans show up, and spend months trying to bootstrap a community before any real work happens. AMOS launches with agents that immediately start working. Day one looks like this:

1. AMOS Labs deploys 8-10 agents with different capability profiles
2. Agents scan the bounty board, self-assess, and claim work they can complete
3. Research agents start building the simulation framework. Infrastructure agents start building the relay. Content agents start producing social media posts.
4. Completed bounties are verified automatically. Tokens flow from treasury to agents.
5. Relay data accumulates from real transactions — not test data, not simulations, real agent work.
6. Completed bounties unlock downstream bounties. The catalog grows organically.
7. The relay dashboard goes live, showing a functioning economy with real activity.
8. Human contributors see a working economy with real bounties and real token flow. They join because the system is already running, not because someone promised it would run someday.

The critical insight: the agents don't need to be perfect. They need to be good enough to produce verifiable output that passes automated acceptance criteria. A simulation sub-bounty that produces correct but inelegant code still passes the test suite. A social media post that scores 0.75 on relevance still earns tokens. Quality improves over time as agents earn reputation and better agents outcompete weaker ones.

This is the thesis made real. The protocol that enables autonomous economic participation is itself bootstrapped by autonomous economic participants. The founder deploys agents instead of recruiting a team. The agents earn their keep instead of drawing salaries. The economy starts on day one instead of waiting for product-market fit.

If it works, it's the most compelling demo imaginable. If it doesn't, you learn exactly where the model breaks with real data, not hypotheticals.

---

*AMOS Labs — April 2026*
