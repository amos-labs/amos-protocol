-- Harness-level settings for provider mode, model selection, etc.
CREATE TABLE IF NOT EXISTS harness_settings (
    key   VARCHAR(100) PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Default model: Sonnet 4.6
INSERT INTO harness_settings (key, value) VALUES
    ('llm_model', '"us.anthropic.claude-sonnet-4-6"')
ON CONFLICT DO NOTHING;

-- Provider mode: if the harness already has a BYOK provider configured,
-- respect that and default to 'byok'. Otherwise, default to 'shared_bedrock'.
INSERT INTO harness_settings (key, value)
SELECT 'llm_provider_mode',
    CASE WHEN EXISTS (SELECT 1 FROM llm_providers WHERE is_active = true)
         THEN '"byok"'::jsonb
         ELSE '"shared_bedrock"'::jsonb
    END
WHERE NOT EXISTS (SELECT 1 FROM harness_settings WHERE key = 'llm_provider_mode');
