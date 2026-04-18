-- OAuth2 state tokens for the authorization code + PKCE handshake.
--
-- Ephemeral rows created at /api/v1/oauth/start and consumed at
-- /api/v1/oauth/callback. Rows older than ~10 minutes are stale and can be
-- cleaned up by any periodic job.

CREATE TABLE oauth_states (
    state_token     VARCHAR(128) PRIMARY KEY,
    credential_id   UUID NOT NULL REFERENCES integration_credentials(id) ON DELETE CASCADE,
    code_verifier   VARCHAR(128) NOT NULL,
    redirect_to     TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '10 minutes')
);

CREATE INDEX idx_oauth_states_expires ON oauth_states (expires_at);
