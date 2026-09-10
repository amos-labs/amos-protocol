-- UUIDs and directory wallets are identifiers, never credentials.
-- Existing agents must prove wallet possession before receiving a new key.
ALTER TABLE relay_agents ADD COLUMN api_key_hash VARCHAR(64);
ALTER TABLE relay_agents ADD COLUMN wallet_verified BOOLEAN NOT NULL DEFAULT false;
CREATE UNIQUE INDEX relay_agents_api_key_hash_unique ON relay_agents(api_key_hash)
    WHERE api_key_hash IS NOT NULL;

-- Historical registration/reputation did not authenticate wallet ownership or
-- reporters. Do not grandfather those assertions into economic authority.
UPDATE relay_agents SET trust_level = 1, council_member = false;
ALTER TABLE relay_reputation_reports ADD COLUMN authenticated_reporter UUID;
-- Preserve the existing UUID harness foreign key and historical bounty link.
-- The HTTP reporting contract also names a non-bounty task and optional score.
ALTER TABLE relay_reputation_reports ADD COLUMN task_id VARCHAR(255);
ALTER TABLE relay_reputation_reports ALTER COLUMN quality_score DROP NOT NULL;

-- Legacy harness secrets were caller-selected and could be overwritten by any
-- anonymous request. Recovery requires operator re-enrollment, not UUID lookup.
UPDATE relay_harnesses SET api_key_hash = 'revoked:' || id::text;

CREATE TABLE relay_identity_challenges (
    id UUID PRIMARY KEY,
    wallet_address VARCHAR(64) NOT NULL,
    message TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX relay_identity_challenge_expiry ON relay_identity_challenges(expires_at);

-- Only an operator with database access can provision service authority.
-- No public registration route writes this table. Keys use service_<64 hex>.
CREATE TABLE relay_service_credentials (
    id UUID PRIMARY KEY,
    name VARCHAR(128) NOT NULL UNIQUE,
    api_key_hash VARCHAR(64) NOT NULL UNIQUE,
    scopes TEXT[] NOT NULL DEFAULT '{}',
    bound_wallets TEXT[] NOT NULL DEFAULT '{}',
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Old approvals cannot authorize a new transfer merely by naming a wallet.
ALTER TABLE relay_bounties ADD COLUMN approved_by_principal TEXT;

-- A legacy verification timestamp is not authenticated evidence.
ALTER TABLE relay_bounties ADD COLUMN verified_by_principal TEXT;
