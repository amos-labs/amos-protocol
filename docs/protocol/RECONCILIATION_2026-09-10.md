# Protocol and ecosystem reconciliation — 2026-09-10

The review found differences between intended economics, executable code, public
copy and claims of deployment. [Economic Contract v1](ECONOMIC_CONTRACT.md) is the
single specification for this source revision. Historical material is retained
and labeled; it no longer competes with the current agent context.

## Findings and resolutions

| Finding | Source resolution | Evidence or remaining boundary |
|---|---|---|
| Public agent UUID accepted as a bearer credential | Strong hashed keys, proved wallets and authenticated principals | Cryptographic and actual HTTP/SQL migration tests; coordinated enrollment required |
| Caller-selected reviewer/worker wallets and open service mutations | Principal-bound identities, explicit service scopes, approval provenance checked on retries | Request-boundary regressions; operators still provision service authority |
| Legacy verification or stale approval could authorize changed work | Authenticated verifier provenance, atomic receipt submission and observed-version approval checks | Actual blocked-update regression returns 409 after a competing resubmission |
| System approval fabricated commercial fee revenue | Removed fee-ledger writes and false settlement marks from all system paths | Real authenticated approval creates no fee entry; old rows remain unaudited history |
| Prefunding a proof address could falsely mark a bounty settled | Confirm program ownership, account type, full layout and matching bounty ID | RPC-boundary regression distinguishes SOL dust, absent state, malformed proof and RPC failure |
| Anonymous harness credential overwrite | New enrollment issues a secret; reconnect requires its current credential | HTTP takeover/reconnect tests; legacy keys revoked on migration |
| Historical unauthenticated trust carried economic authority | Trust/Council reset, fresh wallet proof and authenticated reputation reports | Disposable database migrations; operator review before restoring elevated roles |
| Pending historical approvals lack authenticated provenance after migration | Retry refuses unavailable authority without consuming the queue; wallet re-enrollment does not bless old approvals | Actual database matrix; no public approved-to-re-review transition exists, so recovery must be qualified before live migration |
| Emission stopped above the documented floor | Shared bounded integer sigmoid replaces clamped exponential table | Independent continuous-reference comparison over 20,001 days and extreme inputs |
| RPC error or zero cap could authorize payment | Unknown-state cap is zero; typed absent-account response is distinct; on-chain cap required | Shared-kernel/decoder/time regressions; quotes remain state-dependent |
| First/small submissions could overconsume capacity | Time release and virtual points enforced on-chain; no upward minimum | Reward-cap boundary tests; sequential order effects explicitly retained |
| Growth exhaustion paid an extra raw unit | No category-cap bypass | Zero allocation fails before transfers |
| Content work was paid using the referral category | One shared Relay adapter maps content to on-chain content ID 3 | Mapping regression; other coarse legacy mappings are documented |
| Conflicting trust quotas | Agent system limits fixed to 3/5/10/15/25 and enforced | Existing capability expectations updated to canonical limits |
| Whole-token constants compared with raw balances | Nine-decimal supply, treasury, stake and escrow constants | Treasury initialization now requires the actual intended allocation and correct mint/custody |
| Governance proposal limits still assumed six decimals | Named nine-decimal AMOS unit restores intended feature 1–1M and research 0.1–100K bounds | Production validator boundary tests reject the old fractional minima; existing stored proposal amounts are not rescaled |
| All protocol rewards described as two bounty paths and a universal 95/5 split | Scope bounty settlement separately from governance feature 40/30/30 and research 20% upfront + 400% original-stipend bonus | Shared production reward arithmetic tested at the corrected limits; governance transfers have no daily bounty kernel or automatic funding reservation, and activation remains separate |
| Fee arithmetic overflow below total intended supply | Bounded `u128` intermediate products | Full-supply and extreme amount conservation cases |
| 3% fee versus Labs revenue confused | Distinct percentage bases and conserved worked examples | Labs receives 0.3% of commercial bounty gross; system bounties have no commercial fee |
| Points presented as AMOS or as two revenue streams | Marketplace labels points; unavailable Oracle economic fields become null | Display-helper tests and nullable-metric regressions |
| Escrow refund unbound to original poster/deadline | V2 metadata and signer/mint/time/status checks | Actual generated Anchor account validation; legacy recovery not guessed |
| Stake accepted arbitrary custody | Separate canonical V2 stake/reward vaults | Wrong vault, mint, authority and unsigned-owner account tests |
| Repeat reward claims drained remaining holder pool | Cumulative index, per-position checkpoint and fractional credit | Repeated/reordered claims, membership changes and zero-staker deposits |
| Liquid tokens could vote repeatedly through different wallets | V2 deposits lock real tokens; explicit refunds and replay tombstone | Native SPL processor plus account-validation tests; validator lifecycle still required |
| Decay recharged grace or same-day intervals | Full days after grace, correct checkpoint and balance/floor cap | Timing/repetition/rounding/floor tests |
| “Permissionless mandatory decay” exceeded SPL authority | Holder signature required; per-grant/tenure/vault designs labeled proposed | Explicit source/vision boundary, not a claim of completed compulsory decay |
| Package royalty described as live enforcement | Current proposal replaces false implementation claims; old paper archived | Conserved commercial example; no fee on system emissions |
| Platform served and advertised obsolete token calculations | Five calculator routes return 410; discovery and suggestions omit them | Four route/discovery tests in consumer branch; billing unchanged |
| Website AI knowledge contradicted published plans | Shared pricing definitions supply page and AI knowledge | Pricing-parity, lint, typecheck and production build |
| Marketplace quality, counts and filtering overstated data | Correct 0–100 display, labeled samples and removed unsupported filters | Display tests, lint, typecheck and production build |
| CI treated chain failures as advisory | On-chain host tests and SBF build become required evidence | Stack overflow diagnostics fail CI even if compiler exits zero |
| Clean CI selected an incompatible older SBF compiler | Pin stable Agave 3.1.10 release bytes and Anchor/AVM 0.31.1 | Rebuilt locally with platform-tools v1.52 / Rust 1.89.0; exact-head CI still required |

## Verification record

Local verification uses isolated synthetic state. No live signer, chain transfer,
production database or model endpoint is needed for these checks.

- Root workspace: compile and strict Clippy passed; 401 executable tests plus 6 doc tests passed. Two existing live Oracle integration tests remain intentionally unrun; the normally ignored HTTP regression ran separately as described below.
- Relay: one comprehensive HTTP regression passed against the actual router, fresh PostgreSQL/Redis and all SQL migrations,
  with proved-wallet registration, bad/replayed/expired signatures, credential
  rotation, principal boundaries and authenticated reporting.
- Chain: 121 host tests passed (99 bounty, 13 treasury, 9 governance); actual
  SBF compilation and IDL generation of each changed program with stable Agave
  3.1.10 / platform-tools v1.52 / Rust 1.89.0, rejecting stack diagnostics.
- Governance amount follow-up: 14 host tests passed (the prior 9 plus 5 boundary,
  rounding/conservation and overflow tests). The updated governance program also
  completed SBF compilation and IDL generation with the same pinned toolchain;
  no validator or live reward transfer was executed.
- Math: four tests (included in the root count) cover monotonicity, floor/tail behavior, extreme units/times, payout and decay
  boundaries in the shared dependency-free kernel.
- Fee builder: four offline JavaScript tests; no RPC or key loading.
- Consumers: Platform's exact changed route module tested separately; Website and
  Marketplace type/lint/build checks plus targeted behavior tests. Full Platform
  integration tests remain required CI evidence, not a claimed local full build.

Exact commands, results and content-addressed source diff are carried in the PR
receipt and consumer PR descriptions. Passing host tests does not establish full
validator execution or audited economic/security correctness.

## Activation work deliberately separated from source reconciliation

1. Inventory existing identities, approvals, proposals, stake vaults and outstanding
   escrows. Document actual chain binary hashes and current account state.
2. Resolve provenance and recovery for legacy funds before any upgrade that
   disables their handlers. V2 cannot infer valid ownership or reward debt from
   vulnerable records. Funds can otherwise become immobilized.
3. Rehearse account creation, custody transfers, repeat attempts, terminal refunds,
   rollback and migrations on a validator with the exact candidate binaries/IDLs.
   Confirm compute budgets and all external client account lists.
4. Provision credentials and update Relay/Oracle together, including nullable
   metrics. Inventory pending historical approvals and qualify an audited
   recovery/re-review transition before migration; the current public API does
   not supply that transition. Wallet re-enrollment alone is insufficient.
5. Restore/review missing extracted bot clients before enabling their workflows.
6. Before governance rewards activate, verify the nine-decimal mint and treasury
   authority, fund the full feature/research lifecycle, and rehearse claim order
   and parameter changes. At default parameters successful research needs 4.2
   times its recorded stipend; neither those payments nor feature rewards consume
   the daily bounty pool. Existing recorded amounts remain raw units.
7. Separately authorize and record live activation. Merging source or changing
   public wording alone does not perform these steps.

Per-grant vesting, mandatory decay, package attribution, generalized asset/compute
markets and open economic-network activation remain design work. They have not
been removed from the vision or represented as implemented by this reconciliation.
