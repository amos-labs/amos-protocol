-- AMOS-META-007 phase 5: strict-with-override gate at bounty approval.
--
-- Code bounties (category in infrastructure/research) require a proof_receipt
-- on the row before approval. Reviewers can override with an explicit reason
-- which is persisted permanently — so the override is auditable and feeds
-- drift monitoring (per spec §6: override accountability).

ALTER TABLE relay_bounties
    ADD COLUMN IF NOT EXISTS gate_override_reason TEXT,
    ADD COLUMN IF NOT EXISTS gate_override_by TEXT,
    ADD COLUMN IF NOT EXISTS gate_override_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_relay_bounties_gate_override
    ON relay_bounties (gate_override_at DESC)
    WHERE gate_override_at IS NOT NULL;
