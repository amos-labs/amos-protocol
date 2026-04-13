-- Expand fleet_events CHECK constraint to include supervisor & reconciliation events
ALTER TABLE fleet_events DROP CONSTRAINT IF EXISTS fleet_events_event_type_check;
ALTER TABLE fleet_events ADD CONSTRAINT fleet_events_event_type_check
    CHECK (event_type IN (
        'deployed', 'stopped', 'rebalanced', 'promoted', 'demoted',
        'restarted', 'reconciled', 'error', 'trust_upgraded'
    ));

-- Persistent daily claim counters (survive harness restarts)
CREATE TABLE IF NOT EXISTS agent_daily_claims (
    agent_id    INTEGER NOT NULL REFERENCES openclaw_agents(id) ON DELETE CASCADE,
    claim_date  DATE NOT NULL,
    count       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (agent_id, claim_date)
);
