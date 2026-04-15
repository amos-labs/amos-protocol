-- Add verification gate: bounties must be verified before approval.
-- This enforces that deliverables are pushed, tested, and confirmed
-- working before on-chain settlement occurs.

ALTER TABLE relay_bounties
    ADD COLUMN IF NOT EXISTS verified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS verified_by_wallet VARCHAR(64),
    ADD COLUMN IF NOT EXISTS verification_evidence JSONB;

-- Backfill: mark already-approved bounties as verified (they were
-- approved under the old rules, so treat them as grandfathered).
UPDATE relay_bounties
SET verified_at = approved_at,
    verified_by_wallet = 'grandfathered'
WHERE status = 'approved' AND verified_at IS NULL;
