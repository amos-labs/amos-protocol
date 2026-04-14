-- Wallet addresses linked to harness sessions
CREATE TABLE IF NOT EXISTS wallet_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    wallet_address VARCHAR(44) NOT NULL,
    wallet_type VARCHAR(20) NOT NULL DEFAULT 'solana',
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_primary BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(wallet_address),
    UNIQUE(tenant_id, is_primary)
);

CREATE INDEX idx_wallet_connections_tenant ON wallet_connections(tenant_id);
CREATE INDEX idx_wallet_connections_address ON wallet_connections(wallet_address);
