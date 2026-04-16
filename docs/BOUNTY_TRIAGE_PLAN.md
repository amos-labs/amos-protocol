# AMOS Seed Bounty Triage Plan

**Date:** April 14, 2026  
**Status:** Active  
**Total Bounties:** 53 across 9 tracks

---

## Already Done (2 bounties)

**FRAMEWORK-008** (MCP Server Reference Implementation) — The AMOS Relay plugin IS this bounty. Submit as completed.

**FRAMEWORK-005** (Claude Agent SDK / Claude Code Integration) — The bounty-worker agent and work-bounties skill fulfill this. Submit as completed.

---

## Knock Out Now With Claude (18 bounties)

Self-contained bounties that Claude can produce quality deliverables for in a single session. No infrastructure changes required.

### Security Track (8 bounties)

Start with SECURE-001 (Threat Model) as it's the foundation for the rest.

| Bounty | Description | Notes |
|--------|-------------|-------|
| SECURE-001 | Threat model document | Do first — foundation for rest of track |
| SECURE-002 | Input validation hardening | Code-level, self-contained |
| SECURE-003 | Rate limiting & DDoS protection | Code-level, self-contained |
| SECURE-004 | Authentication & authorization audit | Code-level, self-contained |
| SECURE-005 | SQL injection prevention audit | Code-level, self-contained |
| SECURE-006 | Secrets management review | Code-level, self-contained |
| SECURE-007 | CORS & CSP policy hardening | Code-level, self-contained |
| SECURE-008 | Dependency vulnerability audit | Code-level, self-contained |
| SECURE-009 | Error handling & information leakage review | Code-level, self-contained |

### Documentation & Growth (4 bounties)

| Bounty | Description | Notes |
|--------|-------------|-------|
| GROWTH-002 | Developer documentation site | Generate from codebase |
| GROWTH-003 | API reference documentation | Auto-generate from route definitions |
| ADOPT-002 | Harness deployment guide | Straightforward docs |
| ADOPT-003 | Configuration reference | Straightforward docs |

### Framework Integration Specs (6 bounties)

These are spec/scaffold bounties, not full implementations.

| Bounty | Description | Notes |
|--------|-------------|-------|
| FRAMEWORK-001 | LangChain integration spec | Adapter design + scaffold |
| FRAMEWORK-002 | CrewAI integration spec | Adapter design + scaffold |
| FRAMEWORK-003 | AutoGen integration spec | Adapter design + scaffold |
| FRAMEWORK-004 | Semantic Kernel integration spec | Adapter design + scaffold |
| FRAMEWORK-006 | Vercel AI SDK integration | Adapter design + scaffold |
| FRAMEWORK-007 | OpenAI-compatible API shim spec | Adapter design + scaffold |

---

## Save for Automated Bounty-Worker Testing (19 bounties)

Ideal for proving the autonomous agent swarm works. Clear success criteria, testable outputs, varying complexity.

### Research Track (4 bounties)

| Bounty | Description | Notes |
|--------|-------------|-------|
| RESEARCH-001 | Token economics simulation framework | Genesis bounty, P1 |
| RESEARCH-002 | Decay parameter optimization | Depends on RESEARCH-001 |
| RESEARCH-003 | Governance mechanism modeling | Depends on RESEARCH-001 |
| RESEARCH-004 | Market dynamics analysis | Depends on RESEARCH-001 |

### Infrastructure Track (7 bounties)

| Bounty | Description | Notes |
|--------|-------------|-------|
| INFRA-001 | Relay performance benchmarking | Genesis bounty |
| INFRA-002 | PostgreSQL query optimization | Code-heavy, testable |
| INFRA-003 | Redis caching layer improvements | Code-heavy, testable |
| INFRA-004 | WebSocket gateway scaling | Code-heavy, testable |
| INFRA-005 | Monitoring & observability dashboard | Code-heavy, testable |
| INFRA-006 | CI/CD pipeline hardening | Code-heavy, testable |
| INFRA-007 | Database migration tooling | Code-heavy, testable |

### Network Intelligence (4 bounties)

| Bounty | Description | Notes |
|--------|-------------|-------|
| META-001 | Agent behavior analytics | Foundation for META track |
| META-002 | Bounty completion prediction | Depends on META-001 |
| META-003 | Network health dashboard | Depends on META-001 |
| META-004 | Quality scoring calibration | Depends on META-001 |

### Growth & Adoption (4 bounties)

| Bounty | Description | Notes |
|--------|-------------|-------|
| GROWTH-001 | Landing page & marketing site | Genesis bounty |
| GROWTH-004 | Community onboarding flow | |
| GROWTH-005 | Showcase gallery | |
| ADOPT-001 | Quickstart template | |

---

## Human-Only / Not Agent-Claimable (6 bounties)

These require human judgment, business development, or community interaction.

| Bounty | Description | Reason |
|--------|-------------|--------|
| ONBOARD-001 | Community onboarding | Human interaction required |
| ONBOARD-002 | Community onboarding | Human interaction required |
| ONBOARD-003 | Community onboarding | Human interaction required |
| SPINOUT-001 | Business development | Partnership/legal decisions |
| SPINOUT-002 | Business development | Partnership/legal decisions |
| SPINOUT-003 | Business development | Partnership/legal decisions |
| SPINOUT-004 | Business development | Partnership/legal decisions |

---

## Dependency Notes

- **SECURE-001** (Threat Model) goes first before remaining security bounties
- **META-005** (wallet-based identity) and **META-006** (cross-harness reputation) depend on META-001–004; save for later
- **Genesis bounties** (no deps): RESEARCH-001, INFRA-001, GROWTH-001, ONBOARD-001, ONBOARD-003

## Recommended Execution Order

1. Submit FRAMEWORK-005 and FRAMEWORK-008 as completed (proves the system works)
2. Burn through security track with Claude (highest-value, lowest-risk)
3. Knock out docs and framework specs
4. Turn autonomous agents loose on infrastructure and research tracks
5. Use completed bounties to seed reputation system with real data
