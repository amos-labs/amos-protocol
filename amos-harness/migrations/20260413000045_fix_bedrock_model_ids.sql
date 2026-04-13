-- Fix invalid Bedrock model IDs that were using fabricated date suffixes.
-- The correct cross-region inference profile IDs don't include date suffixes
-- for Sonnet 4.6 and Opus 4.6.
UPDATE harness_settings
SET value = '"us.anthropic.claude-sonnet-4-6"'::jsonb,
    updated_at = NOW()
WHERE key = 'llm_model'
  AND value = '"us.anthropic.claude-sonnet-4-6-20250514-v1:0"'::jsonb;
