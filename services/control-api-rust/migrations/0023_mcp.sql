-- 0023_mcp — MCP servers as data, and which sandbox talks to which.
--
-- An MCP server is either a process inside the sandbox (stdio) or a remote
-- HTTP endpoint. Only the second kind crosses the egress proxy and is therefore
-- visible in the traffic log; the first is local and, by definition, not
-- egress. The registry records the transport so the console can say which is
-- which instead of showing an empty tool column and calling it "no activity".
--
-- Credentials for a remote server (OAuth tokens, API keys) live in the agent's
-- own home directory inside the container. This table holds the server
-- definition and, per sandbox, the discovered tool list — never a token.

CREATE TABLE mcp_registry (
    key            text PRIMARY KEY,
    name           text NOT NULL,
    description    text,
    publisher      text,
    homepage       text,
    -- stdio | http | sse
    transport      text NOT NULL,
    -- For http/sse: the endpoint. For stdio: NULL (command below).
    endpoint       text,
    command        text,
    args           text[] NOT NULL DEFAULT '{}',
    -- none | token | oauth
    auth_type      text NOT NULL DEFAULT 'none',
    -- Environment variable names the server needs (names only).
    env_slots      text[] NOT NULL DEFAULT '{}',
    -- Which agent packs can use it; empty means any.
    packs          text[] NOT NULL DEFAULT '{}',
    enabled        boolean NOT NULL DEFAULT true,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT mcp_registry_transport_valid CHECK (transport IN ('stdio', 'http', 'sse')),
    CONSTRAINT mcp_registry_auth_valid CHECK (auth_type IN ('none', 'token', 'oauth')),
    CONSTRAINT mcp_registry_endpoint_or_command CHECK (
        (transport = 'stdio' AND command IS NOT NULL)
        OR (transport <> 'stdio' AND endpoint IS NOT NULL)
    ),
    CONSTRAINT mcp_registry_endpoint_scheme CHECK (endpoint IS NULL OR endpoint ~ '^https?://')
);

-- A sandbox's attachment to a server: enabled or not, and what discovery found.
CREATE TABLE sandbox_mcp_servers (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    server_key     text NOT NULL REFERENCES mcp_registry(key) ON DELETE RESTRICT,
    enabled        boolean NOT NULL DEFAULT true,
    -- connected | disconnected | error | unknown
    status         text NOT NULL DEFAULT 'unknown',
    -- The tool list as last discovered: [{name, description, input_schema}].
    tools          jsonb NOT NULL DEFAULT '[]'::jsonb,
    last_session_id text,
    last_seen_at   timestamptz,
    last_error     text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sandbox_mcp_servers_unique UNIQUE (sandbox_id, server_key),
    CONSTRAINT sandbox_mcp_servers_status_valid
        CHECK (status IN ('connected', 'disconnected', 'error', 'unknown'))
);

CREATE INDEX sandbox_mcp_servers_sandbox_idx ON sandbox_mcp_servers (sandbox_id);

-- ---------------------------------------------------------------------------
-- Seed: the servers a sandbox can be given today.
--
-- `nestbox-hub` is our own: an HTTP MCP server on the overlay that exposes the
-- sandbox's own filesystem and shell as tools. It exists so that what the
-- agent does to its sandbox is visible as traffic, with tool names, instead of
-- being invisible stdio inside the container.
-- ---------------------------------------------------------------------------
INSERT INTO mcp_registry (key, name, description, publisher, homepage, transport, endpoint, command, args, auth_type, env_slots, packs) VALUES
    ('nestbox-hub', 'Nestbox hub',
     'The sandbox''s own files and shell as MCP tools, served over HTTP on the overlay so every call is visible.',
     'Nestbox', NULL, 'http', 'http://mcp-hub:8080/mcp', NULL, '{}', 'token',
     ARRAY['NESTBOX_MCP_TOKEN'], '{}'),
    ('filesystem', 'Filesystem',
     'Read and write files under a directory. Runs inside the sandbox (stdio).',
     'Model Context Protocol', 'https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem',
     'stdio', NULL, 'npx', ARRAY['-y', '@modelcontextprotocol/server-filesystem', '/home/nestbox'], 'none', '{}', '{}'),
    ('fetch', 'Fetch',
     'Fetch a URL and return its content as markdown. Runs inside the sandbox; its outbound requests cross the proxy.',
     'Model Context Protocol', 'https://github.com/modelcontextprotocol/servers/tree/main/src/fetch',
     'stdio', NULL, 'uvx', ARRAY['mcp-server-fetch'], 'none', '{}', '{}'),
    ('github', 'GitHub',
     'Repositories, issues, pull requests. Remote HTTP server; needs a token.',
     'GitHub', 'https://github.com/github/github-mcp-server',
     'http', 'https://api.githubcopilot.com/mcp/', NULL, '{}', 'token',
     ARRAY['GITHUB_PERSONAL_ACCESS_TOKEN'], '{}'),
    ('deepwiki', 'DeepWiki',
     'Ask questions about public GitHub repositories. Remote, no credential.',
     'Cognition', 'https://docs.devin.ai/work-with-devin/deepwiki-mcp',
     'http', 'https://mcp.deepwiki.com/mcp', NULL, '{}', 'none', '{}', '{}'),
    ('playwright', 'Playwright',
     'Drive a browser: navigate, click, read the page. Runs inside the sandbox.',
     'Microsoft', 'https://github.com/microsoft/playwright-mcp',
     'stdio', NULL, 'npx', ARRAY['-y', '@playwright/mcp@latest'], 'none', '{}', '{}'),
    ('context7', 'Context7',
     'Up-to-date library documentation for coding agents. Remote, no credential.',
     'Upstash', 'https://github.com/upstash/context7',
     'http', 'https://mcp.context7.com/mcp', NULL, '{}', 'none', '{}', '{}'),
    ('linear', 'Linear',
     'Issues and projects. Remote server with OAuth.',
     'Linear', 'https://linear.app/docs/mcp',
     'http', 'https://mcp.linear.app/mcp', NULL, '{}', 'oauth', '{}', '{}'),
    ('notion', 'Notion',
     'Pages and databases. Remote server with OAuth.',
     'Notion', 'https://developers.notion.com/docs/mcp',
     'http', 'https://mcp.notion.com/mcp', NULL, '{}', 'oauth', '{}', '{}'),
    ('airtable', 'Airtable',
     'Bases, tables, and records. Remote server with OAuth.',
     'Airtable', 'https://support.airtable.com/articles/9897799762-using-the-airtable-mcp-server',
     'http', 'https://mcp.airtable.com/mcp', NULL, '{}', 'oauth', '{}', '{}')
ON CONFLICT (key) DO NOTHING;

COMMENT ON TABLE mcp_registry IS
  'MCP servers a sandbox can be given. transport decides whether the proxy sees the calls: http/sse yes, stdio no.';
COMMENT ON TABLE sandbox_mcp_servers IS
  'Which servers a sandbox has, and the tools discovery found. Never a credential.';
