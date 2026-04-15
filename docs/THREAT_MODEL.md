# AMOS Platform Threat Model

**Version:** 1.0
**Date:** 2026-04-15
**Scope:** amos-harness, amos-relay, amos-solana on-chain programs
**Classification:** Internal — guides all SECURE-* bounty work

---

## 1. System Overview

AMOS is a three-tier AI-native business operating system:

| Component | Role | Exposure |
|-----------|------|----------|
| **amos-harness** | Per-customer AI runtime (agent loop, tools, canvas, sites) | Internet-facing HTTP (port 3000) |
| **amos-relay** | Multi-tenant bounty coordination and settlement | Internet-facing HTTP (port 4100) |
| **amos-platform** | Central control plane (provisioning, billing, governance) | Internal HTTP (4000) + gRPC (4001) |
| **amos-solana** | On-chain programs (bounty, treasury, governance) | Solana mainnet RPC |

### Data Flow

```
User Browser ──HTTPS──▶ ALB ──▶ Harness (per-tenant container)
                                    │
                                    ├── PostgreSQL (tenant data, JSONB collections)
                                    ├── Redis (cache, rate limits)
                                    ├── AWS Bedrock (Claude API)
                                    └── S3 (file uploads)

Agent/Plugin ──HTTPS──▶ ALB ──▶ Relay
                                    │
                                    ├── PostgreSQL (bounties, agents, harnesses)
                                    ├── Redis (cache)
                                    └── Solana RPC (settlement transactions)

Platform ──gRPC──▶ Harness containers (provisioning, lifecycle)
```

---

## 2. Asset Inventory

### Critical Assets

| Asset | Location | Sensitivity | Impact if Compromised |
|-------|----------|-------------|----------------------|
| Oracle keypair | Secrets Manager → relay ECS task | **Critical** | Attacker can sign settlement transactions, drain treasury |
| Treasury token account | On-chain (37D62Smc...) | **Critical** | Contains 1M AMOS for bounty distribution |
| Vault master key | Env var `AMOS__VAULT__MASTER_KEY` | **Critical** | Decrypts all stored credentials (integrations, API keys) |
| JWT signing secret | Env var `AMOS__AUTH__JWT_SECRET` | **High** | Forge auth tokens, impersonate any user |
| Database credentials | Env vars / Secrets Manager | **High** | Full read/write to tenant data |
| User conversation data | PostgreSQL JSONB | **High** | PII, business data, agent interactions |
| API keys (harness ↔ relay) | SHA-256 hashed in DB | **Medium** | Impersonate harness on relay |
| Agent wallet addresses | Relay PostgreSQL | **Low** | Public on-chain anyway |

### On-Chain Assets

| Account | Address | Risk |
|---------|---------|------|
| Bounty Program | `4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq` | Immutable after deploy |
| BountyConfig PDA | `HAgXjWCGR7ry9LnPLA3B3BZeKKR2RNFkJyvAcozQDJQN` | Oracle-only writes |
| Treasury (config-owned) | `9xDVHuW4kiUYH5NPDLFfKhpxLQ31N6bqMrvj4EJ57z2B` | Config PDA signs transfers |
| AMOS Mint | `5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ` | Metaplex metadata on-chain |

---

## 3. Trust Boundaries

### TB-1: Browser ↔ Harness
- **Auth:** JWT (Bearer header or `amos_session` HttpOnly cookie)
- **CORS:** Production restricts to `*.amoslabs.com` origins
- **Session:** SameSite=Lax cookies, Secure flag
- **Rate limiting:** 20 burst / 2 req/s on chat, 100 burst / 20 req/s on API

### TB-2: Agent/Plugin ↔ Relay
- **Auth:** SHA-256 hashed Bearer API key checked against `relay_harnesses.api_key_hash`
- **Open endpoints:** `/health`, `/api/v1/harnesses/connect`, `/api/v1/agents/register`
- **CORS:** Permissive (relay is a public protocol)

### TB-3: Agent Loop ↔ Tool Execution
- **Sandbox:** Container isolation (primary), uid 1001 (secondary)
- **Env scrubbing:** All `AMOS__*`, `AWS_*`, `SECRET`, `TOKEN`, `PASSWORD`, `DATABASE_URL` removed
- **Hard blocks:** `/proc/self/environ`, `/etc/shadow`, AWS metadata endpoints, iptables
- **Destructive commands:** Require user confirmation token (5-min expiry)
- **Output limits:** 50 KB max, 120s default / 600s max timeout

### TB-4: Harness ↔ External Services
- **URL validation:** HTTP/HTTPS only, blocks localhost/private IPs/metadata endpoints
- **DNS resolution:** Private IP ranges blocked after resolution (SSRF prevention)
- **File uploads:** 20 MB max, extension length ≤ 10 chars, UUID-based storage keys

### TB-5: Relay ↔ Solana
- **Oracle signs all transactions** — single point of authority
- **PDA derivation:** Deterministic from program seeds, verified on-chain
- **On-chain validation:** Anchor constraints enforce account ownership, seeds, mutability

---

## 4. Threat Actors

| Actor | Motivation | Capability | Likely Targets |
|-------|-----------|------------|----------------|
| **Malicious Agent** | Earn undeserved tokens, game reputation | API access, automated claims | Bounty system, trust levels, quality scoring |
| **Compromised Harness** | Data exfiltration, lateral movement | Tenant container access | User data, credentials vault, platform API |
| **Rogue Operator** | Steal funds, manipulate governance | Oracle keypair access | Treasury, bounty settlements, config updates |
| **External Attacker** | Data theft, service disruption | Internet access to HTTP endpoints | Auth bypass, SSRF, injection |
| **MEV Bot** | Front-run bounty claims, manipulate settlement | Solana mempool observation | On-chain transactions, bounty claims |
| **Prompt Injection** | Manipulate agent behavior | Crafted input through tools/web | Agent loop, tool execution, data exfiltration |

---

## 5. Attack Vectors (STRIDE Classification)

### 5.1 Spoofing

| ID | Vector | Component | Severity | Mitigation | Gap |
|----|--------|-----------|----------|------------|-----|
| S-1 | Forge JWT tokens | Harness | **Critical** | JWT secret validation, expiry check | None if secret is strong |
| S-2 | Spoof X-Forwarded-For to bypass rate limits | Harness | **Medium** | Rate limiting on IP | No trusted proxy whitelist — IP can be spoofed through compromised proxy |
| S-3 | Impersonate harness on relay | Relay | **Medium** | SHA-256 API key hash | Key rotation not automated |
| S-4 | Fake agent registration | Relay | **Low** | Open registration by design | Sybil resistance only via trust levels |

### 5.2 Tampering

| ID | Vector | Component | Severity | Mitigation | Gap |
|----|--------|-----------|----------|------------|-----|
| T-1 | Modify bounty status without auth | Relay | **Critical** | API key auth + SQL WHERE status check | None |
| T-2 | Tamper with agent tool output | Harness | **Medium** | Tool output goes through agent loop | No integrity check on tool results |
| T-3 | Modify on-chain config | Solana | **Critical** | Oracle authority + Anchor constraints | Single oracle = single point of failure |
| T-4 | SQL injection via dynamic queries | All | **Critical** | Parameterized queries throughout | `format!()` with const strings — safe but pattern is fragile |

### 5.3 Repudiation

| ID | Vector | Component | Severity | Mitigation | Gap |
|----|--------|-----------|----------|------------|-----|
| R-1 | Deny bounty submission | Relay | **Low** | Database audit trail + on-chain proof | None |
| R-2 | Deny agent actions | Harness | **Medium** | Conversation logs stored | No tamper-proof audit log |

### 5.4 Information Disclosure

| ID | Vector | Component | Severity | Mitigation | Gap |
|----|--------|-----------|----------|------------|-----|
| I-1 | Extract secrets via agent bash tool | Harness | **Critical** | Env scrubbing, uid 1001, hard blocks | Regex-based command detection can be bypassed with obfuscation |
| I-2 | SSRF to internal services | Harness | **High** | URL validation, private IP blocking, DNS resolution check | None identified |
| I-3 | JWT token in URL query params | Harness | **Medium** | HTTPS-only, Secure cookie flag | Token appears in server logs, browser history, Referer headers |
| I-4 | Wallet addresses in relay logs | Relay | **Low** | N/A | Addresses logged at INFO level — semi-sensitive |
| I-5 | Error messages reveal internals | All | **Medium** | Generic error responses | Some Anchor error codes propagated to client |

### 5.5 Denial of Service

| ID | Vector | Component | Severity | Mitigation | Gap |
|----|--------|-----------|----------|------------|-----|
| D-1 | Rate limit bypass via IP spoofing | Harness | **Medium** | X-Forwarded-For rate limiting | No proxy trust chain validation |
| D-2 | Large file upload exhaustion | Harness | **Low** | 20 MB limit, body size limits | None |
| D-3 | Agent loop infinite iteration | Harness | **Medium** | `max_iterations` config (default 25) | None |
| D-4 | Bounty spam | Relay | **Low** | API key required | No posting rate limit or deposit requirement |
| D-5 | Solana RPC rate limiting | Relay | **Medium** | Retry with backoff | No fallback RPC endpoint |

### 5.6 Elevation of Privilege

| ID | Vector | Component | Severity | Mitigation | Gap |
|----|--------|-----------|----------|------------|-----|
| E-1 | Container escape from agent tool | Harness | **Critical** | Docker/ECS container isolation | Standard container security applies |
| E-2 | Trust level gaming (Sybil) | Solana | **High** | On-chain trust thresholds, permissionless registration | Oracle approves all bounties — Sybil resistance depends on reviewer quality |
| E-3 | Oracle key compromise | Solana | **Critical** | Secrets Manager, ECS task role | Single key controls all settlements and config updates |
| E-4 | Cross-tenant data access | Harness | **Critical** | Per-tenant containers, separate DB connections | Container isolation is sole boundary |

---

## 6. Risk Matrix

| Risk | Likelihood | Impact | Rating | Priority |
|------|-----------|--------|--------|----------|
| E-3: Oracle key compromise | Low | Critical | **High** | P1 — add key rotation, multi-sig |
| I-1: Secret extraction via bash | Medium | Critical | **High** | P1 — env scrubbing is good, add audit logging |
| S-2: Rate limit bypass | Medium | Medium | **Medium** | P2 — add proxy trust whitelist |
| I-3: JWT in URL params | Medium | Medium | **Medium** | P2 — move to POST body |
| E-2: Trust level Sybil | Medium | High | **Medium** | P2 — add staking requirement for registration |
| D-5: Solana RPC single point | Medium | Medium | **Medium** | P2 — add fallback RPC |
| T-4: SQL format! pattern | Low | Critical | **Medium** | P3 — cosmetic, consts are safe |
| D-4: Bounty spam | Low | Low | **Low** | P3 — add rate limit |
| I-4: Wallet in logs | High | Low | **Low** | P3 — truncate in logs |

---

## 7. Existing Mitigations Summary

### Strong
- **Parameterized SQL everywhere** — no injection vectors found
- **AES-256-GCM credential vault** — proper encryption at rest
- **Container isolation** — primary sandbox for agent tools
- **Env scrubbing** — secrets removed from subprocess environment
- **SSRF prevention** — URL validation + DNS resolution + private IP blocking
- **On-chain immutability** — bounty proofs are permanent records
- **Anchor constraints** — on-chain accounts validated for ownership, seeds, mutability

### Adequate
- **Rate limiting** — functional but spoofable via X-Forwarded-For
- **CORS** — restricted in production, open in relay (by design)
- **Destructive command blocking** — regex-based, bypassable but defense-in-depth with container isolation

### Gaps Requiring Work
- **No multi-sig for oracle operations** — single key controls treasury
- **No automated key rotation** — API keys and oracle keypair are static
- **No CSRF tokens** — relies on SameSite cookies (adequate but not defense-in-depth)
- **No WAF** — ALB passes traffic directly to containers
- **No audit log** — actions are logged but not in tamper-proof format
- **Redis without TLS** — ElastiCache connection is unencrypted (known, planned for post-launch)

---

## 8. Recommendations by Priority

### P1 — Critical (address within 30 days)
1. **Oracle key management**: Implement key rotation schedule, consider multi-sig for high-value operations (treasury updates, config changes)
2. **Audit logging**: Add structured, append-only audit trail for all state-changing operations
3. **WAF deployment**: Add AWS WAF in front of ALB with OWASP Core Rule Set

### P2 — High (address within 60 days)
4. **Rate limit hardening**: Configure trusted proxy whitelist, use real client IP detection
5. **JWT token exchange**: Move from URL query param to POST body or Authorization header
6. **Sybil resistance**: Add SOL staking requirement for agent trust registration
7. **Fallback RPC**: Configure secondary Solana RPC endpoint for settlement resilience

### P3 — Medium (address within 90 days)
8. **CSRF tokens**: Add X-CSRF-Token middleware for state-changing browser requests
9. **Log sanitization**: Truncate/hash wallet addresses and other semi-sensitive data
10. **Redis TLS**: Migrate to TLS-enabled ElastiCache cluster (already planned)
11. **Dependency audit**: Run `cargo audit` in CI with blocking mode (currently advisory-only)

---

## 9. SECURE Bounty Mapping

This threat model informs the following bounty work:

| Bounty | Addresses Threats | Priority |
|--------|-------------------|----------|
| SECURE-002: Input validation | T-2, T-4, I-1 | P1 |
| SECURE-003: Rate limiting & DDoS | S-2, D-1, D-4, D-5 | P2 |
| SECURE-004: Auth & authorization audit | S-1, S-3, E-4, I-3 | P1 |
| SECURE-005: SQL injection audit | T-4 | P1 (verify) |
| SECURE-006: Secrets management | E-3, I-1 | P1 |
| SECURE-007: CORS & CSP | I-5 | P2 |
| SECURE-008: Dependency audit | General | P3 |
| SECURE-009: Error handling & info leakage | I-4, I-5, R-2 | P2 |
