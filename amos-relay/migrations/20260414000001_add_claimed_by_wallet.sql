-- Add wallet_address identity to bounty claims for direct on-chain settlement
ALTER TABLE relay_bounties ADD COLUMN IF NOT EXISTS claimed_by_wallet VARCHAR(44);
CREATE INDEX IF NOT EXISTS idx_relay_bounties_claimed_wallet ON relay_bounties(claimed_by_wallet);
