# Package Economy Integration — proposal

**Status: design research, not an implemented royalty or revenue entitlement.**
The [Economic Contract v 1](../protocol/ECONOMIC_CONTRACT.md) governs current
settlement. The [April proposal](../archive/package-economy-integration-2026-04.md)
is preserved as design history.

## Intended ecosystem

Open tools provide capabilities. Packages combine tools, prompts and domain
knowledge. Workers use them to complete verifiable tasks. A future marketplace
could compensate package authors from demonstrated use and outcomes, in addition
to ordinary bounties for building and maintaining packages.

A package registry, usage attribution verifier and on-chain royalty split are not
implemented in the current settlement instructions. The earlier text described
these mechanisms as already enforced; that claim was incorrect.

## Illustrative royalty proposal

The earlier default was 0.5% of **commercial gross**, adjustable 0.1–1.0%, taken
from the holder share of the existing 3% fee. None of these rates are active.
For a hypothetical 200 AMOS commercial escrow:

| Destination | Current contract | Hypothetical 0.5% package royalty |
|---|---:|---:|
| Worker |184.3 AMOS|184.3 AMOS|
| Reviewer |9.7 AMOS|9.7 AMOS|
| Holders |3 AMOS|2 AMOS|
| Package creator |0 AMOS|1 AMOS|
| Burn |2.4 AMOS|2.4 AMOS|
| Labs |0.6 AMOS|0.6 AMOS|
| Total |200 AMOS|200 AMOS|

A system bounty paid from treasury emissions has **no 3% fee** and therefore no
existing holder-fee allocation to redirect. Do not mix its dynamic points payout
with the commercial example. Creating a system royalty would require a separately
funded policy and an explicit economic-contract version change.

## Decisions required before implementation

- Which verified uses qualify, and how do forks, dependency chains and multiple
  packages share attribution without call-count gaming?
- Is compensation opt-in, and who can authorize reducing holders' fee allocation?
- What aggregate ceiling prevents several packages from exceeding the holder share?
- What signed evidence, dispute process and independent checks support attribution?
- What migration, accounting tests and governance approval activate a new version?

The current wallet-decay implementation does not enforce the old proposal's
365-day per-grant grace, tenure protections or mandatory erosion of every holder.
Package authors do not acquire those protections or liabilities through this doc.
