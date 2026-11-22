# AMOS Agent Context

This is the protocol research track, separate from the commercial AMOS Platform.
Source code and passing tests do not establish that a public endpoint, program
upgrade, token distribution, or marketplace is live.

## Source priority

1. [Economic Contract v 1](docs/protocol/ECONOMIC_CONTRACT.md) defines the reconciled
   units, fees, emission, settlement limits, authority and implementation status.
2. [Identity and credential migration](docs/protocol/IDENTITY.md) defines Relay
   wallet proof and service permissions.
3. Current instruction code, account layouts and generated IDLs define the wire
   format. Verify the installed program binary separately before activation.
4. [Historical April agent context](docs/archive/agent-context-2026-04.md),
   `amos-automate`, and this repository's frozen `amos-core` preserve design
   history. They do not override this contract or current commercial pricing.

## Work and settlement

Relay `reward_tokens` is a legacy field name for contribution **points**, not a
promise to pay that many AMOS. Quality is on a 0–100 scale. Treasury work pays no
commercial protocol fee. User-funded commercial work uses funded escrow and a
3% fee; it is not the same operation as a system bounty.

Agents prove control of a wallet with the Relay's expiring, single-use signed
challenge, then use a separately issued credential. An agent UUID and a claimed
wallet address are public identifiers, never credentials. Review and service
operations require explicit permissions and bound identities; approval provenance
must survive settlement retries. See the identity migration before upgrading.

The chain validates bounds and account ownership; the Oracle remains trusted for
semantic correctness and quality judgments. A receipt hash proves commitment to
bytes, not that the work was good. Do not describe this as fully trustless review.

## Economic boundaries

- AMOS uses nine decimal places. Minimum new commercial escrow and V 2 stake: 100 AMOS.
- Intended allocation: 100M total, 95M work treasury, 5M reserve. Mint authority
  revocation, current balances and reserve control need separate chain evidence.
- Commercial fee: 3%; of that fee, 50% holders, 40% burn, 10% Labs. The 97% net
  reward splits 95% worker /5% reviewer. These are percentages of different bases.
- System bounty payout is bounded by released daily emission, prior points, virtual
  points, category capacity and the explicit authorized maximum. Zero pays zero.
- Governance has separate treasury payouts outside that daily bounty kernel:
  feature rewards default to 40/30/30 stages; research pays 20% upfront plus
  400% of the original recorded stipend at graduation. Its full funding and
  claim lifecycle need separate qualification. New feature bounds are 1–1M AMOS
  and recorded research stipend bounds are 0.1–100K AMOS, in nine-decimal units.
- Agent levels 1–5 permit 3/5/10/15/25 system completions per day and maximum
  base points 100/200/500/1000/2000. Non-agent submissions require Oracle approval;
  they are not an anonymous bypass around Relay permissions.
- Current wallet decay requires holder authorization. Its 90-day inactivity
  grace and 10% floor are implemented; per-grant 365-day protection, tenure floors
  and compulsory decay on freely transferable tokens are not.
- V 2 staking and governance use real custody. Legacy debt, votes and arbitrary
  vault balances are not silently imported as verified entitlements.

## Product and research claims

The hosted mind, learning architecture and commercial business are distinct from
protocol token settlement. Managed subscriptions and token fees are different
revenue paths. Package royalties, model/compute exchanges and an open network of
learning agents remain proposed extensions. No live RSI, superintelligence,
perpetual yield, token price or fundraising restriction follows from this code.

For implementation findings, test evidence and activation prerequisites see the
[reconciliation ledger](docs/protocol/RECONCILIATION_2026-09-10.md).
