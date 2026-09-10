# Relay identity and authorization

This is the coordinated Relay/Oracle identity migration in Protocol source. It is not a deployment receipt. The public Relay is not established as serving by this change, and no production database, wallet or chain state is changed by these tests.

## Credentials and wallet proof

Agent UUIDs, harness names and public wallet addresses are identifiers, never bearer credentials. API keys contain 256 random bits from the operating system, encoded as `agent_`, `harness_` or `service_` followed by 64 lowercase hexadecimal characters. Relay stores only their SHA-256 hash. A public directory response does not contain keys or key hashes.

An agent registers or recovers its identity as follows:

1. POST `/api/v1/agents/challenge` with its Solana `wallet_address`.
2. Sign the returned `message` bytes using that wallet's Ed25519 key. The message binds the operation, wallet, random nonce, challenge ID and five-minute expiry. Encode the signature as base58.
3. POST `/api/v1/agents/register` with the normal registration fields and `wallet_proof: { challenge_id, signature }`.
4. Store the returned `api_key` privately. It is returned once. Send it as `Authorization: Bearer …` for subsequent mutations.

Challenge consumption and registration are one database transaction. Invalid/expired proofs fail; a successful proof cannot be replayed. A fresh proof for an existing wallet preserves the agent UUID and rotates its credential, invalidating the previous credential. A suspended identity cannot bypass suspension through registration. Proving a wallet grants neither Council membership nor elevated trust.

Claim and submission wallets derive from the authenticated agent. Supplied agent IDs, harness attachment or wallet fields cannot impersonate another principal. Agent heartbeat changes only the caller's identity. Attaching registration to a harness requires the matching authenticated harness credential.

New harness connection returns a generated `api_key`; a supplied legacy `api_key` does not set it. Reconnecting an existing harness requires that harness's current credential and does not rotate it. A lost harness key requires operator recovery rather than an anonymous overwrite. Inactive identities can authenticate to heartbeat; suspended identities cannot.

## Authority

| Principal / operation | Required authority |
| --- | --- |
| Agent heartbeat, bounty claim or submission | Exact authenticated agent; wallet derived from its proof |
| Harness reconnect or heartbeat | Exact authenticated harness |
| Verify, approve, reject, revise or push back | Caller-bound reviewer wallet and current required trust; Council membership and separation of duties where required by the route |
| Service reviewing a bounty | `bounties:review` plus the exact reviewer wallet in `bound_wallets`; the wallet's agent must also satisfy the route's trust/Council checks |
| Create a treasury-funded bounty | `bounties:create` plus the exact poster wallet |
| Record a repository merge | `bounties:merge`; recorded actor derives from the principal |
| Retry settlement | `settlement:retry` |
| Write an outcome/reputation report | `reputation:write`; subject must be a verified active agent |
| Evaluate an intake | `intakes:evaluate` |
| Create an escalation | `escalations:create` |
| Resolve an escalation | `escalations:resolve`, or a verified trusted Council agent; supplied payout/poster wallet must be caller-bound |

The existing create-bounty endpoint creates system bounties funded from the treasury. Until an independently verified commercial escrow/deposit flow exists in Relay, ordinary new registrations cannot use it to allocate treasury emissions. This does not remove the separate on-chain escrow primitives.

Intake submitter and escalation resolver identities are recorded from authentication, not arbitrary request strings. Review/approval prevents the poster or worker wallet from acting as its own reviewer. Approval also records the authenticated principal. Automatic and manual settlement retries require both a verified claimed agent's matching wallet and a currently authorized reviewer tied to an authenticated approval; they do not guess fallback wallets.

`GET /api/v1/identity/me` returns the authenticated principal's kind, identifier, scope and wallet delegation, without its secret or hash. Public health, pool and directory reads do not make POST mutations public. GitHub's public webhook entry retains its independent signature verification.

## Service provisioning and client rollout

There is no public API for minting administrative service authority. An operator provisions `relay_service_credentials` through the database: a unique UUID/name, the SHA-256 hash of a newly generated 256-bit `service_` credential, explicit scopes and explicit bound wallets. Store the raw key only in the intended service's secret store; do not put it into a command argument, committed fixture, log or directory response. Empty scope/wallet lists grant no implicit wildcard rights. Revoke by setting `active=false`; rotate by replacing the stored hash and the intended consumer's secret together.

Oracle requires `ORACLE_RELAY_API_KEY` to be a service credential. At startup it validates `/identity/me`, these scopes, and both distinct configured wallets:

- `bounties:create`
- `bounties:review`
- `intakes:evaluate`
- `escalations:create`

The reviewer still needs a verified agent identity and the appropriate trust/Council state. Separate poster and reviewer wallet delegation keeps separation of duties explicit.

QA and auto-merge must use separate credentials. A QA reviewer is wallet-bound and scoped for review; the auto-merge key needs `bounties:merge`. `.github/workflows/auto-merge-bot.yml` no longer falls back to the QA secret. The three extracted bot workflows currently reference absent `scripts/qa-verification-bot.sh`, `scripts/auto-merge-bot.sh` and `scripts/emit-stale-merge-metric.sh`; explicit guards stop them with an actionable source-restoration error. This change does not restore or claim to operate those schedules. Restore and review the actual bot clients before enabling their schedules.

The Rust `amos-agent` harness client talks to its distinct harness API rather than directly registering against Relay. Its existing harness protocol was not silently changed. Any separately deployed harness-to-Relay client must adopt the new enrollment/key-return contract before this Relay migration is activated.

## Existing identity migration

Apply `20260910000001_authenticated_principals.sql` with the new Relay code during a coordinated maintenance window, before exposing registration publicly. It deliberately fails closed instead of preserving previously unauthenticated economic claims:

- Existing agent rows, wallets and UUIDs remain, but start unverified with no new bearer key. Re-enroll through wallet proof. Trust resets to 1 and Council membership to false; restore elevated roles only after explicit operator review of authenticated identity.
- All legacy harness hashes are revoked because the old public connection path allowed caller-selected keys and anonymous replacement. Re-provision an existing harness with a new strong hashed credential through the operator path before reconnecting.
- Historical reputation reports remain for audit but do not count toward current reputation unless written through the newly authenticated reporting path. The migration does not bless old reporter assertions.
- Existing verifications and approvals have no authenticated principal. They cannot authorize a new payout merely by naming a wallet; require a fresh authenticated verification and legitimate review. Verification and approval compare the observed submission version before writing; revision or re-submission invalidates earlier verification. Already completed on-chain transfers are not undone. Historical system fee-ledger rows are retained as unverified history; system approval/settlement no longer creates a commercial fee or marks those old rows as paid.
- Provision scoped service keys and update the Oracle/bot consumers before resuming their mutations. A public UUID is never a compatibility fallback.

### Pending approvals are an activation prerequisite

Inventory unsettled approved rows before applying this migration. The new retry
guard refuses a missing, unverified, suspended, low-trust or non-Council reviewer,
and refuses a historical approval without authenticated provenance. It preserves
the queued row and does not spend a retry attempt on that authority failure.
Re-enrolling the wallet or restoring its reviewed Council role alone does **not**
authenticate an old approval.

There is currently no public transition for moving an already-approved historical
row back through fresh verification and approval: those endpoints accept submitted
work. An audited recovery/re-review workflow must therefore be designed and
rehearsed for any such inventory **before live migration**. This source change does
not supply that workflow or promise automatic queue recovery. Do not manufacture
an `approved_by_principal` value to unblock payment. Keep mutations paused until
the original evidence, recipients, actual chain state and replacement authority
have been reconciled without duplicate settlement. A blind rollback would also
restore the unauthenticated trust boundary and is not a recovery procedure.

The same release changes five metrics fields to nullable values: `daily_emission_remaining_points`, `daily_pool_points_distributed`, `growth_pool_cap_bps`, `commercial_volume_7d` and `system_emission_7d`. Deploy Relay and Oracle together. Oracle still accepts known numeric values from older responses; new null or absent measurements render as **unavailable**, not zero. Reward points are not atomic AMOS volume. A missing pool/RPC observation does not imply an available budget, and a missing commercial-volume measurement does not prove zero activity.

## Local verification

The normal workspace commands are `cargo check --workspace`, `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`. The HTTP regression additionally runs through the real router and every SQL migration:

```sh
bash amos-relay/tests/run-identity-integration.sh
```

That script starts disposable PostgreSQL and Redis instances bound to localhost, uses a dedicated `protocol_identity_test` database, and removes both after the test. It does not consume the application's `DATABASE_URL`, contact Solana or change live tenants. It exercises registration/signature proof, public credential exclusion, rotation, expiration/replay, identity-bound mutations, harness takeover rejection, scoped/revoked service keys, positive reputation writes and authenticated system approval without a fee ledger. A real blocked database update verifies that an approval checked against submission A cannot approve replacement B; legacy timestamps and nullable worker-wallet self-review are rejected. A queued-approval database matrix checks the actual settlement reviewer lookup, including unavailable authority, retained status/retry counters and refusal to bless historical approval through re-enrollment alone. These checks establish source behavior, not production rollout or a completed external security audit.
