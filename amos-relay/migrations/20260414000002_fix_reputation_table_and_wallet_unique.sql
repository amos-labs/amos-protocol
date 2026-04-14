-- Fix #1: Rename reputation_reports -> relay_reputation_reports to match code
ALTER TABLE IF EXISTS reputation_reports RENAME TO relay_reputation_reports;

-- Also rename the indexes to match
ALTER INDEX IF EXISTS idx_reputation_reports_agent RENAME TO idx_relay_reputation_reports_agent;
ALTER INDEX IF EXISTS idx_reputation_reports_harness RENAME TO idx_relay_reputation_reports_harness;

-- Fix #2: Add UNIQUE constraint on relay_agents.wallet_address
-- Prevents duplicate wallet registrations across agents
CREATE UNIQUE INDEX IF NOT EXISTS idx_relay_agents_wallet_unique
    ON relay_agents(wallet_address) WHERE wallet_address IS NOT NULL;
