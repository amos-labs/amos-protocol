# Token Economy

The [Economic Contract v 1](ECONOMIC_CONTRACT.md) is the current specification.
It distinguishes implemented source from proposed economics and verified deployment.

- AMOS: 9 decimals; intended 100M allocation, 95M work treasury /5M reserve.
- Treasury work:contribution points, bounded dynamic payouts, no commercial fee.
- Funded commercial work: 3% gross fee; fee split 50% holders /40% burn /10% Labs;
  the remaining 97% splits 95% worker /5% reviewer.
- V 2 staking:canonical custody, cumulative reward accounting, no repeated claims
  against the remaining pool. V 2 governance:real locked voting tokens.
- Decay:holder-authorized wallet operation after 90 days of inactivity; per-grant
 365-day grace, tenure/vault protections and compulsory wallet decay remain proposals.

Do not infer current chain balances, a live exchange, audited safety, investment
returns or commercial subscription pricing from these parameters. Older equation
papers are preserved under `docs/archive/` and do not override the contract.
