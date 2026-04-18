-- Directory of well-known OAuth2 providers.
--
-- This gives Amos (and humans setting things up) a knowledge base of how each
-- popular service's OAuth flow works. Each row captures the provider-specific
-- metadata that never changes (auth URL, token URL, default scopes) so users
-- only need to supply their own client_id + client_secret.
--
-- "Custom" providers are still supported — users can create an
-- integration_credentials row with their own auth_url/token_url without
-- needing a row here.

CREATE TABLE oauth_providers (
    slug                    VARCHAR(64) PRIMARY KEY,
    name                    VARCHAR(255) NOT NULL,
    auth_url                VARCHAR(500) NOT NULL,
    token_url               VARCHAR(500) NOT NULL,
    -- Space-separated scopes that cover common use cases. Users can override.
    default_scopes          TEXT,
    -- URL of the provider's OAuth app creation page (e.g.
    -- https://console.cloud.google.com/apis/credentials). Amos sends users
    -- here to create their own OAuth client.
    app_creation_url        VARCHAR(500),
    docs_url                VARCHAR(500),
    icon_url                VARCHAR(500),
    -- Markdown setup instructions shown to the user when Amos offers to
    -- connect this provider. Should reference the exact redirect URI.
    setup_instructions      TEXT,
    -- Special flags (e.g. "access_type=offline" for Google to get a
    -- refresh_token). Free-form JSON; routes/oauth.rs reads these.
    extra_auth_params       JSONB DEFAULT '{}',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed the directory with common services.
INSERT INTO oauth_providers (slug, name, auth_url, token_url, default_scopes, app_creation_url, docs_url, setup_instructions, extra_auth_params) VALUES
('google',
 'Google',
 'https://accounts.google.com/o/oauth2/v2/auth',
 'https://oauth2.googleapis.com/token',
 'openid email profile https://www.googleapis.com/auth/calendar https://www.googleapis.com/auth/gmail.send',
 'https://console.cloud.google.com/apis/credentials',
 'https://developers.google.com/identity/protocols/oauth2',
 '1. Go to the Google Cloud Console APIs & Credentials page.\n2. Create an OAuth 2.0 Client ID (type: Web application).\n3. Add this redirect URI: `{REDIRECT_URI}`\n4. Enable the APIs you need (Calendar, Gmail, Drive, etc.) from the API library.\n5. Copy the Client ID and Client Secret and paste them here.',
 '{"access_type": "offline", "prompt": "consent"}'::jsonb),

('github',
 'GitHub',
 'https://github.com/login/oauth/authorize',
 'https://github.com/login/oauth/access_token',
 'repo user read:org',
 'https://github.com/settings/developers',
 'https://docs.github.com/en/apps/oauth-apps/building-oauth-apps',
 '1. Go to GitHub → Settings → Developer settings → OAuth Apps.\n2. Click "New OAuth App".\n3. Set Authorization callback URL to: `{REDIRECT_URI}`\n4. Click "Register application".\n5. Generate a client secret.\n6. Copy the Client ID and Client Secret and paste them here.',
 '{}'::jsonb),

('slack',
 'Slack',
 'https://slack.com/oauth/v2/authorize',
 'https://slack.com/api/oauth.v2.access',
 'chat:write channels:read im:write users:read',
 'https://api.slack.com/apps',
 'https://api.slack.com/authentication/oauth-v2',
 '1. Go to https://api.slack.com/apps and click "Create New App" → "From scratch".\n2. Under "OAuth & Permissions", add this redirect URL: `{REDIRECT_URI}`\n3. Under "Scopes → Bot Token Scopes", add the scopes you need (chat:write, channels:read, etc.).\n4. Install the app to your workspace.\n5. Copy the Client ID and Client Secret from "Basic Information".',
 '{}'::jsonb),

('hubspot',
 'HubSpot',
 'https://app.hubspot.com/oauth/authorize',
 'https://api.hubapi.com/oauth/v1/token',
 'crm.objects.contacts.read crm.objects.contacts.write crm.objects.deals.read oauth',
 'https://developers.hubspot.com/docs/api/creating-an-app',
 'https://developers.hubspot.com/docs/api/oauth-quickstart-guide',
 '1. Create a HubSpot developer account at developers.hubspot.com.\n2. Create a new app in your developer account.\n3. Under "Auth", add this redirect URL: `{REDIRECT_URI}`\n4. Choose the scopes you need (contacts, deals, companies, etc.).\n5. Copy the Client ID and Client Secret.',
 '{}'::jsonb),

('notion',
 'Notion',
 'https://api.notion.com/v1/oauth/authorize',
 'https://api.notion.com/v1/oauth/token',
 '',
 'https://www.notion.so/my-integrations',
 'https://developers.notion.com/docs/authorization',
 '1. Go to https://www.notion.so/my-integrations.\n2. Click "New integration" → choose "Public integration".\n3. Add this redirect URI: `{REDIRECT_URI}`\n4. Configure the capabilities the integration needs (read/write pages, etc.).\n5. Copy the OAuth client ID and client secret.',
 '{"owner": "user"}'::jsonb),

('microsoft',
 'Microsoft (Azure AD)',
 'https://login.microsoftonline.com/common/oauth2/v2.0/authorize',
 'https://login.microsoftonline.com/common/oauth2/v2.0/token',
 'offline_access openid profile email Mail.Send Calendars.ReadWrite Files.ReadWrite',
 'https://portal.azure.com/#blade/Microsoft_AAD_RegisteredApps',
 'https://learn.microsoft.com/en-us/azure/active-directory/develop/v2-oauth2-auth-code-flow',
 '1. Go to the Azure Portal → App registrations → New registration.\n2. Account types: "Accounts in any organizational directory and personal Microsoft accounts".\n3. Redirect URI: Web → `{REDIRECT_URI}`\n4. Under "Certificates & secrets", create a new client secret.\n5. Under "API permissions", add the Microsoft Graph scopes you need.\n6. Copy the Application (client) ID and the secret value.',
 '{}'::jsonb),

('linkedin',
 'LinkedIn',
 'https://www.linkedin.com/oauth/v2/authorization',
 'https://www.linkedin.com/oauth/v2/accessToken',
 'openid profile email w_member_social',
 'https://www.linkedin.com/developers/apps',
 'https://learn.microsoft.com/en-us/linkedin/shared/authentication/authorization-code-flow',
 '1. Go to https://www.linkedin.com/developers/apps and create an app.\n2. Under "Auth", add this redirect URL: `{REDIRECT_URI}`\n3. Under "Products", request access to the APIs you need (e.g. Share on LinkedIn).\n4. Copy the Client ID and Client Secret.',
 '{}'::jsonb),

('x_twitter',
 'X (Twitter)',
 'https://twitter.com/i/oauth2/authorize',
 'https://api.twitter.com/2/oauth2/token',
 'tweet.read tweet.write users.read offline.access',
 'https://developer.x.com/en/portal/dashboard',
 'https://developer.x.com/en/docs/authentication/oauth-2-0',
 '1. Go to the X developer portal and create a project/app.\n2. In app settings → User authentication settings, enable OAuth 2.0.\n3. Set Callback URI: `{REDIRECT_URI}`\n4. Choose the app permissions (Read or Read+Write).\n5. Copy the OAuth 2.0 Client ID and Client Secret.',
 '{}'::jsonb),

('calendly',
 'Calendly',
 'https://auth.calendly.com/oauth/authorize',
 'https://auth.calendly.com/oauth/token',
 'default',
 'https://calendly.com/integrations/api_webhooks',
 'https://developer.calendly.com/api-docs/ZG9jOjE2Nzg5NDUx-o-auth',
 '1. Go to https://calendly.com/integrations/api_webhooks and create an OAuth application.\n2. Add this redirect URL: `{REDIRECT_URI}`\n3. Copy the Client ID and Client Secret.',
 '{}'::jsonb),

('atlassian',
 'Atlassian (Jira + Confluence)',
 'https://auth.atlassian.com/authorize',
 'https://auth.atlassian.com/oauth/token',
 'read:jira-work write:jira-work read:confluence-content.all offline_access',
 'https://developer.atlassian.com/console/myapps/',
 'https://developer.atlassian.com/cloud/jira/platform/oauth-2-3lo-apps/',
 '1. Go to https://developer.atlassian.com/console/myapps/ and create an OAuth 2.0 (3LO) app.\n2. Under "Authorization", set this callback URL: `{REDIRECT_URI}`\n3. Under "Permissions", add the APIs and scopes you need.\n4. Copy the Client ID and Client Secret.',
 '{"audience": "api.atlassian.com"}'::jsonb),

('zoom',
 'Zoom',
 'https://zoom.us/oauth/authorize',
 'https://zoom.us/oauth/token',
 'meeting:read meeting:write user:read',
 'https://marketplace.zoom.us/develop/create',
 'https://developers.zoom.us/docs/integrations/oauth/',
 '1. Go to the Zoom App Marketplace → Develop → Build App → OAuth.\n2. Choose "User-managed app".\n3. Under "Redirect URL for OAuth", add: `{REDIRECT_URI}`\n4. Under "Scopes", add the scopes you need.\n5. Copy the Client ID and Client Secret from the App Credentials tab.',
 '{}'::jsonb);

CREATE INDEX idx_oauth_providers_slug ON oauth_providers (slug);
