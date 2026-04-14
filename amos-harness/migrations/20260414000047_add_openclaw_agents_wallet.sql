-- Add wallet_address to openclaw_agents so autonomous agents can participate in bounties
ALTER TABLE openclaw_agents ADD COLUMN IF NOT EXISTS wallet_address VARCHAR(44);
CREATE INDEX IF NOT EXISTS idx_openclaw_agents_wallet ON openclaw_agents(wallet_address);
