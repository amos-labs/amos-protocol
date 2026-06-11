-- AMOS-SECURE-008: indexes backing hot read paths on bots, external_agents,
-- integrations, and sites.
--
-- Each index is tied to a query in the codebase:
--   * integrations(name)  — list endpoints order by name
--     (src/routes/integrations.rs, src/tools/integration_tools.rs)
--   * sites(created_at DESC) — site listings order newest-first
--     (src/sites.rs list_sites, src/tools/workspace_tools.rs)
--   * external_agents(status, total_tasks_completed) — periodic reputation
--     report scans active agents with completed work (src/relay_sync.rs)
--   * bots(last_heartbeat_at) — liveness sweeps over bot heartbeats
--
-- Note: plain CREATE INDEX (not CONCURRENTLY) because sqlx runs migrations
-- inside a transaction. These tables are small at current scale, so the
-- brief write lock is acceptable.

CREATE INDEX IF NOT EXISTS idx_integrations_name
    ON integrations (name);

CREATE INDEX IF NOT EXISTS idx_sites_created
    ON sites (created_at DESC);

CREATE INDEX IF NOT EXISTS idx_external_agents_status_completed
    ON external_agents (status, total_tasks_completed);

CREATE INDEX IF NOT EXISTS idx_bots_last_heartbeat
    ON bots (last_heartbeat_at);
