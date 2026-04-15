-- Track on-chain transaction signatures for bounty listings and agent trust registration
ALTER TABLE relay_bounties
  ADD COLUMN IF NOT EXISTS onchain_listing_tx VARCHAR(100);

ALTER TABLE relay_agents
  ADD COLUMN IF NOT EXISTS onchain_trust_tx VARCHAR(100);
