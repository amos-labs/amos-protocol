-- AMOS-META-007 phase 2: proof-carrying bounty receipts.
--
-- Adds two additive JSONB columns to relay_bounties:
--
-- 1. `policy` — set at bounty creation time. Constraints the submission
--    must respect: forbidden_paths, required_paths_subset, scope_constraint_ids,
--    minimum_coverage_pct, max_file_size_bytes, plus a top-level
--    `self_modifying` boolean for RSI-class bounties. Auto-populated by Oracle
--    commissioning; manual posters may supply it explicitly.
--
-- 2. `proof_receipt` — set at submission time by the worker. The canonical
--    proof-carrying contract: intent, policy, validation plan, execution
--    evidence, github metadata, result summary. Shape-validated by relay;
--    semantic content judged by Oracle / council.
--
-- Both are nullable for back-compat. Receipts only become required for
-- code-bounty approval in META-007 phase 5.

ALTER TABLE relay_bounties
    ADD COLUMN IF NOT EXISTS policy JSONB,
    ADD COLUMN IF NOT EXISTS proof_receipt JSONB;

-- GIN indexes on both — queries like "find bounties with self_modifying=true"
-- or "find receipts that ran a specific check id" will be common in drift
-- monitoring and Oracle review.
CREATE INDEX IF NOT EXISTS idx_relay_bounties_policy_gin
    ON relay_bounties USING GIN (policy jsonb_path_ops);

CREATE INDEX IF NOT EXISTS idx_relay_bounties_proof_receipt_gin
    ON relay_bounties USING GIN (proof_receipt jsonb_path_ops);
