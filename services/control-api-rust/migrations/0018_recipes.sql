-- 0018_recipes — the builder: recipes, persistent roots, mounts, and the agents
-- inside a sandbox.
--
-- The rule behind every table here: the image is immutable, persistent data
-- lives outside the container. A recipe produces an image; a persistent root
-- is a host directory on a node; a mount binds one into a sandbox; a sandbox
-- agent is one pack instance with its own or a shared home.

-- ---------------------------------------------------------------------------
-- Recipes: the builder's desired state.
-- ---------------------------------------------------------------------------

CREATE TABLE recipes (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id  uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    owner_user_id   uuid REFERENCES users(id) ON DELETE SET NULL,
    name            text NOT NULL,
    baseline_key    text NOT NULL REFERENCES baselines(key),
    tool_profile_id uuid REFERENCES tool_profiles(id) ON DELETE SET NULL,
    -- Extra tools chosen in the builder, by tool_catalog key.
    tools           text[] NOT NULL DEFAULT '{}',
    -- [{pack_key, instance, home_mode, persistence, home_paths, port}]
    agents          jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- {memory_limit, cpu_limit, gpu_count, pids_limit}
    resources       jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- [{root_id, mount_path, read_only}]
    project_mounts  jsonb NOT NULL DEFAULT '[]'::jsonb,
    node_id         uuid NOT NULL REFERENCES nodes(id) ON DELETE RESTRICT,
    state           text NOT NULL DEFAULT 'draft',
    -- nestbox-recipe/<slug>:<content_hash>, set when a build succeeds.
    image_ref       text,
    content_hash    text,
    -- Bumped on every edit; the image of an older revision stays until removed.
    revision        bigint NOT NULL DEFAULT 1,
    build_job_id    uuid,
    build_error     text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT recipes_name_unique UNIQUE (environment_id, name),
    CONSTRAINT recipes_state_valid CHECK (state IN ('draft', 'building', 'built', 'failed'))
);

CREATE INDEX recipes_env_idx ON recipes (environment_id);

-- ---------------------------------------------------------------------------
-- Persistent roots: host directories on a node.
-- ---------------------------------------------------------------------------

CREATE TABLE persistent_roots (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    node_id        uuid NOT NULL REFERENCES nodes(id) ON DELETE RESTRICT,
    -- agent_home: one agent's home. project: a project folder mounted into
    -- one or more sandboxes.
    kind           text NOT NULL,
    owner_user_id  uuid REFERENCES users(id) ON DELETE SET NULL,
    slug           text NOT NULL,
    -- The directory on the node. Always under the node's persistent root;
    -- the runner refuses anything else.
    host_path      text NOT NULL,
    bytes_used     bigint NOT NULL DEFAULT 0,
    measured_at    timestamptz,
    archived_at    timestamptz,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT persistent_roots_unique UNIQUE (node_id, owner_user_id, slug),
    CONSTRAINT persistent_roots_kind_valid CHECK (kind IN ('agent_home', 'project')),
    CONSTRAINT persistent_roots_slug_valid CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
    CONSTRAINT persistent_roots_path_absolute CHECK (host_path ~ '^/')
);

CREATE INDEX persistent_roots_env_idx ON persistent_roots (environment_id);
CREATE INDEX persistent_roots_node_idx ON persistent_roots (node_id);

-- ---------------------------------------------------------------------------
-- Mounts: a root bound into a sandbox.
-- ---------------------------------------------------------------------------

CREATE TABLE mounts (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    root_id     uuid NOT NULL REFERENCES persistent_roots(id) ON DELETE CASCADE,
    sandbox_id  uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    mount_path  text NOT NULL,
    read_only   boolean NOT NULL DEFAULT false,
    status      text NOT NULL DEFAULT 'mounted',
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT mounts_unique UNIQUE (root_id, sandbox_id),
    CONSTRAINT mounts_path_absolute CHECK (mount_path ~ '^/'),
    CONSTRAINT mounts_status_valid CHECK (status IN ('mounted', 'dismounted', 'missing'))
);

-- One read-write mount per root at a time. Read-only sharing is unlimited.
CREATE UNIQUE INDEX mounts_one_rw_per_root
    ON mounts (root_id) WHERE NOT read_only AND status = 'mounted';

CREATE INDEX mounts_sandbox_idx ON mounts (sandbox_id);

-- ---------------------------------------------------------------------------
-- Agents inside a sandbox.
-- ---------------------------------------------------------------------------

CREATE TABLE sandbox_agents (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    sandbox_id  uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    pack_key    text NOT NULL REFERENCES agent_packs(key),
    instance    text NOT NULL,
    -- shared: /home/nestbox for all agents. isolated: /home/agents/<instance>.
    home_mode   text NOT NULL DEFAULT 'shared',
    -- persistent: the home is a persistent root. ephemeral: it dies with the
    -- container.
    persistence text NOT NULL DEFAULT 'ephemeral',
    root_id     uuid REFERENCES persistent_roots(id) ON DELETE SET NULL,
    port        integer,
    status      text NOT NULL DEFAULT 'configured',
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sandbox_agents_unique UNIQUE (sandbox_id, instance),
    CONSTRAINT sandbox_agents_instance_valid CHECK (instance ~ '^[a-z0-9][a-z0-9-]{0,31}$'),
    CONSTRAINT sandbox_agents_home_mode_valid CHECK (home_mode IN ('shared', 'isolated')),
    CONSTRAINT sandbox_agents_persistence_valid CHECK (persistence IN ('persistent', 'ephemeral')),
    CONSTRAINT sandbox_agents_status_valid CHECK (status IN ('configured', 'running', 'stopped', 'removed'))
);

-- Recipe link on the sandbox, so "rebuild from recipe" knows its source.
ALTER TABLE sandboxes ADD COLUMN recipe_id uuid REFERENCES recipes(id) ON DELETE SET NULL;

-- ---------------------------------------------------------------------------
-- Row-level security.
-- ---------------------------------------------------------------------------

ALTER TABLE recipes ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON recipes
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE persistent_roots ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON persistent_roots
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE mounts ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON mounts
    USING (EXISTS (
        SELECT 1 FROM sandboxes s WHERE s.id = mounts.sandbox_id
           AND s.environment_id = current_setting('app.environment_id', true)::uuid))
    WITH CHECK (EXISTS (
        SELECT 1 FROM sandboxes s WHERE s.id = mounts.sandbox_id
           AND s.environment_id = current_setting('app.environment_id', true)::uuid));

ALTER TABLE sandbox_agents ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON sandbox_agents
    USING (EXISTS (
        SELECT 1 FROM sandboxes s WHERE s.id = sandbox_agents.sandbox_id
           AND s.environment_id = current_setting('app.environment_id', true)::uuid))
    WITH CHECK (EXISTS (
        SELECT 1 FROM sandboxes s WHERE s.id = sandbox_agents.sandbox_id
           AND s.environment_id = current_setting('app.environment_id', true)::uuid));
