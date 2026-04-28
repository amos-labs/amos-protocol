-- AMOS-META-007 phase 3: structured failure capsules on revision requests.
--
-- When a reviewer requests revision, they emit a structured capsule
-- describing the failure (failing_command, exit_code, log_excerpt,
-- changed_files_implicated, suspected_cause, next_action_requested)
-- instead of free-form feedback. Workers consume this directly in their
-- rework prompt — strictly better than parsing log dumps.
--
-- Capsule lives on the bounty row, latest-only. History is queryable via
-- oracle_decisions / oracle_outcomes for drift monitoring.

ALTER TABLE relay_bounties
    ADD COLUMN IF NOT EXISTS failure_capsule JSONB;

CREATE INDEX IF NOT EXISTS idx_relay_bounties_failure_capsule_gin
    ON relay_bounties USING GIN (failure_capsule jsonb_path_ops);
