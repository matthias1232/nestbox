-- 0046_media_mcp — media rendering (image, video, music) as a governed capability.
--
-- A sandbox's agent asks for a picture, a clip or a track through an MCP tool.
-- The tool does not hold a provider credential and does not choose a model: it
-- asks the control plane, which resolves the effective choice (sandbox override,
-- else the environment default), checks that the choice is on the environment's
-- allow-list, and only then calls the provider. The credential stays in the
-- secret store, the sandbox never sees it, and every render is a row an operator
-- can audit.
--
-- Two settings tables, one job table:
--
--   * `media_settings`        — the environment default and the allow-list;
--   * `sandbox_media_settings`— a sandbox's own choice, one row per sandbox;
--   * `media_renders`         — what was asked, which model answered, where the
--                               asset is and how long it took.
--
-- A NULL target means "inherit": a sandbox with no row, or a row with a NULL
-- kind, uses the environment default. That is why the default cannot be
-- represented by an empty string.

CREATE TABLE media_settings (
    environment_id       uuid PRIMARY KEY REFERENCES environments(id) ON DELETE CASCADE,
    enabled              boolean NOT NULL DEFAULT true,
    -- The environment default per kind. NULL means "no default yet", which the
    -- console shows as unconfigured rather than pretending a model exists.
    image_provider_id    uuid REFERENCES provider_profiles(id) ON DELETE SET NULL,
    image_model          text,
    video_provider_id    uuid REFERENCES provider_profiles(id) ON DELETE SET NULL,
    video_model          text,
    music_provider_id    uuid REFERENCES provider_profiles(id) ON DELETE SET NULL,
    music_model          text,
    -- Which providers a sandbox owner may choose from. Empty means every
    -- provider profile in the environment, which is the least surprising
    -- default for a fresh appliance.
    allowed_provider_ids uuid[] NOT NULL DEFAULT '{}',
    updated_by           uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at           timestamptz NOT NULL DEFAULT now(),
    -- A provider without a model name cannot serve a media request; the pair is
    -- either complete or absent.
    CONSTRAINT media_settings_image_pair CHECK (
        (image_provider_id IS NULL AND image_model IS NULL)
        OR (image_provider_id IS NOT NULL AND image_model IS NOT NULL AND length(image_model) > 0)
    ),
    CONSTRAINT media_settings_video_pair CHECK (
        (video_provider_id IS NULL AND video_model IS NULL)
        OR (video_provider_id IS NOT NULL AND video_model IS NOT NULL AND length(video_model) > 0)
    ),
    CONSTRAINT media_settings_music_pair CHECK (
        (music_provider_id IS NULL AND music_model IS NULL)
        OR (music_provider_id IS NOT NULL AND music_model IS NOT NULL AND length(music_model) > 0)
    )
);

CREATE TABLE sandbox_media_settings (
    sandbox_id        uuid PRIMARY KEY REFERENCES sandboxes(id) ON DELETE CASCADE,
    environment_id    uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    image_provider_id uuid REFERENCES provider_profiles(id) ON DELETE SET NULL,
    image_model       text,
    video_provider_id uuid REFERENCES provider_profiles(id) ON DELETE SET NULL,
    video_model       text,
    music_provider_id uuid REFERENCES provider_profiles(id) ON DELETE SET NULL,
    music_model       text,
    updated_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sandbox_media_settings_image_pair CHECK (
        (image_provider_id IS NULL AND image_model IS NULL)
        OR (image_provider_id IS NOT NULL AND image_model IS NOT NULL AND length(image_model) > 0)
    ),
    CONSTRAINT sandbox_media_settings_video_pair CHECK (
        (video_provider_id IS NULL AND video_model IS NULL)
        OR (video_provider_id IS NOT NULL AND video_model IS NOT NULL AND length(video_model) > 0)
    ),
    CONSTRAINT sandbox_media_settings_music_pair CHECK (
        (music_provider_id IS NULL AND music_model IS NULL)
        OR (music_provider_id IS NOT NULL AND music_model IS NOT NULL AND length(music_model) > 0)
    )
);

CREATE INDEX sandbox_media_settings_env_idx ON sandbox_media_settings (environment_id);

CREATE TABLE media_renders (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    -- image | video | music
    kind           text NOT NULL,
    provider_id    uuid REFERENCES provider_profiles(id) ON DELETE SET NULL,
    provider_name  text,
    model          text NOT NULL,
    prompt         text NOT NULL,
    -- queued | running | done | error
    status         text NOT NULL DEFAULT 'queued',
    -- Relative name under the media directory. A locator, never an absolute
    -- host path, so the API can serve it without exposing the layout.
    asset_name     text,
    mime_type      text,
    bytes          bigint,
    error          text,
    duration_ms    integer,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    finished_at    timestamptz,
    CONSTRAINT media_renders_kind_valid CHECK (kind IN ('image', 'video', 'music')),
    CONSTRAINT media_renders_status_valid CHECK (status IN ('queued', 'running', 'done', 'error')),
    CONSTRAINT media_renders_prompt_len CHECK (length(prompt) BETWEEN 1 AND 4000)
);

CREATE INDEX media_renders_sandbox_idx ON media_renders (sandbox_id, created_at DESC);
CREATE INDEX media_renders_env_idx ON media_renders (environment_id, created_at DESC);

ALTER TABLE media_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE media_settings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON media_settings
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE sandbox_media_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE sandbox_media_settings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON sandbox_media_settings
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE media_renders ENABLE ROW LEVEL SECURITY;
ALTER TABLE media_renders FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON media_renders
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

-- ---------------------------------------------------------------------------
-- Registry: the two MCP servers this migration adds.
--
-- `graphify` is a stdio server: it runs inside the sandbox and answers from the
-- knowledge graph on the persistent home. stdio is not egress, so its calls are
-- not in the traffic log; the console already says so for every stdio server.
--
-- `nestbox-media` is HTTP on the sandbox network. Every call is therefore an
-- observable egress event with a tool name, and the server itself holds no
-- provider credential: it asks the control plane, which resolves the model.
-- ---------------------------------------------------------------------------

INSERT INTO mcp_registry (key, name, description, publisher, homepage, transport, command, args, auth_type, env_slots, packs)
VALUES (
    'graphify',
    'Graphify knowledge graph',
    'Query a local knowledge graph built from this project (code, docs, SQL, PDFs). Runs inside the sandbox against graphify-out/graph.json, so no repository or question leaves the machine.',
    'Graphify Labs',
    'https://github.com/Graphify-Labs/graphify',
    'stdio',
    'graphify',
    ARRAY['--mcp', '--graph', '/home/nestbox/brain/graphify-out/graph.json'],
    'none',
    ARRAY[]::text[],
    ARRAY[]::text[]
)
ON CONFLICT (key) DO NOTHING;

INSERT INTO mcp_registry (key, name, description, publisher, homepage, transport, endpoint, auth_type, env_slots, packs)
VALUES (
    'nestbox-media',
    'Nestbox media rendering',
    'Render an image, a short video or a music track through the model this environment (or this sandbox) selected. The provider credential stays in the control plane; every call is policy-checked and logged.',
    'Nestbox',
    NULL,
    'http',
    'http://media-mcp:8080/mcp',
    'token',
    ARRAY['NESTBOX_MEDIA_TOKEN'],
    ARRAY['hermes', 'claude-code', 'opencode', 'openchamber']
)
ON CONFLICT (key) DO NOTHING;
