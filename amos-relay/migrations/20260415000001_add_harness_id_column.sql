-- Add harness_id column to relay_harnesses.
-- The code references harness_id as a string identifier (e.g. "amos-labs-founder")
-- but the initial schema only had id UUID. The connect_harness handler uses
-- harness_id for upserts and responses.

ALTER TABLE relay_harnesses ADD COLUMN IF NOT EXISTS harness_id VARCHAR(255) UNIQUE;

-- Backfill any existing rows (use UUID as harness_id if none set)
UPDATE relay_harnesses SET harness_id = id::text WHERE harness_id IS NULL;

-- Make it NOT NULL after backfill
ALTER TABLE relay_harnesses ALTER COLUMN harness_id SET NOT NULL;
