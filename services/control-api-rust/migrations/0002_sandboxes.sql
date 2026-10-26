-- 0002_sandboxes — gateways, supervisors, workspaces, sandboxes.
--
-- The hierarchy is deliberate: a gateway is a control-plane process on a node,
-- a supervisor is the per-sandbox agent that enforces policy inside it, and a
-- sandbox is the workload. Modelling the supervisor as its own row (rather than
-- a field on the sandbox) is what makes "each sandbox has its own supervisor"
-- inspectable and gives drift a place to live.

CREATE TABLE gateways (
    id                       uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    cluster_id               uuid NOT NULL REFERENCES clusters(id) ON DELETE RESTRICT,
    node_id                  uuid REFERENCES nodes(id) ON DELETE SET NULL,
    name                     text NOT NULL,
    endpoint                 text NOT NULL,
    -- A locator, never a credential (ADR 0005).
    auth_ref                 text,
    status                   text NOT NULL DEFAULT 'unknown',
    version                  text,
    -- Operator configuration, versioned. Separate from policy (ADR 0012).
    settings                 jsonb NOT NULL DEFAULT '{}'::jsonb,
    settings_version         bigint NOT NULL DEFAULT 1,
    settings_applied_version bigint NOT NULL DEFAULT 0,
    capacity                 integer NOT NULL DEFAULT 100,
    used                     integer NOT NULL DEFAULT 0,
    last_seen                timestamptz,
    created_at               timestamptz NOT NULL DEFAULT now(),
    updated_at               timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT gateways_name_unique UNIQUE (cluster_id, name),
    CONSTRAINT gateways_status_valid
        CHECK (status IN ('unknown', 'healthy', 'degraded', 'unhealthy', 'offline')),
    CONSTRAINT gateways_capacity_sane CHECK (capacity > 0 AND used >= 0)
);

COMMENT ON COLUMN gateways.settings_applied_version IS
  'The settings version the gateway actually runs. Drift is desired <> applied.';

CREATE INDEX gateways_cluster_idx ON gateways (cluster_id);
CREATE INDEX gateways_status_idx ON gateways (status);

CREATE TABLE workspaces (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id  uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name            text NOT NULL,
    -- Only ephemeral is used now. The model allows a shared volume later
    -- without a migration of the sandbox table.
    storage_mode    text NOT NULL DEFAULT 'ephemeral',
    storage_ref     text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT workspaces_name_unique UNIQUE (environment_id, name),
    CONSTRAINT workspaces_storage_mode_valid
        CHECK (storage_mode IN ('ephemeral', 'shared_volume'))
);

CREATE TABLE supervisors (
    id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    gateway_id            uuid NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
    sandbox_id            uuid,
    name                  text NOT NULL,
    status                text NOT NULL DEFAULT 'unknown',
    version               text,
    -- Reported by the runtime: how many config deliveries are still pending.
    config_dirtiness      integer NOT NULL DEFAULT 0,
    last_policy_applied_at timestamptz,
    last_seen             timestamptz,
    created_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT supervisors_name_unique UNIQUE (gateway_id, name),
    CONSTRAINT supervisors_status_valid
        CHECK (status IN ('unknown', 'starting', 'running', 'degraded', 'stopped', 'failed')),
    CONSTRAINT supervisors_dirtiness_sane CHECK (config_dirtiness >= 0)
);

CREATE INDEX supervisors_gateway_idx ON supervisors (gateway_id);

CREATE TABLE sandboxes (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id      uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    cluster_id          uuid NOT NULL REFERENCES clusters(id) ON DELETE RESTRICT,
    gateway_id          uuid REFERENCES gateways(id) ON DELETE SET NULL,
    supervisor_id       uuid REFERENCES supervisors(id) ON DELETE SET NULL,
    workspace_id        uuid REFERENCES workspaces(id) ON DELETE SET NULL,
    name                text NOT NULL,
    pack_key            text NOT NULL DEFAULT 'ubuntu-dev',
    tool_profile_id     uuid,
    provider_profile_id uuid,
    image               text NOT NULL,
    -- Digest pinning is mandatory; `latest` is rejected by the image policy test.
    image_digest        text,
    -- The desired state: resources, environment slots, bindings. Kept as a
    -- document so a later migration between gateways has something to carry.
    spec                jsonb NOT NULL DEFAULT '{}'::jsonb,
    status              text NOT NULL DEFAULT 'pending',
    health              text NOT NULL DEFAULT 'unknown',
    -- Optimistic locking. Every mutation checks this and increments it.
    version             bigint NOT NULL DEFAULT 1,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sandboxes_name_unique UNIQUE (environment_id, name),
    CONSTRAINT sandboxes_status_valid CHECK (status IN (
        'pending', 'provisioning', 'starting', 'running', 'stopping',
        'stopped', 'failed', 'deleting', 'deleted'
    )),
    CONSTRAINT sandboxes_health_valid
        CHECK (health IN ('unknown', 'healthy', 'degraded', 'unhealthy')),
    CONSTRAINT sandboxes_version_positive CHECK (version > 0)
);

COMMENT ON COLUMN sandboxes.version IS
  'Optimistic lock. A write with a stale version is rejected rather than applied.';

-- Covers the common list query: filter by tenant, order by recency, keyset page.
CREATE INDEX sandboxes_env_updated_idx
    ON sandboxes (environment_id, updated_at DESC, id DESC);
CREATE INDEX sandboxes_env_status_idx ON sandboxes (environment_id, status);
CREATE INDEX sandboxes_env_cluster_idx ON sandboxes (environment_id, cluster_id);
CREATE INDEX sandboxes_env_name_idx ON sandboxes (environment_id, name);
-- Partial index for the rows that change most and are queried most.
CREATE INDEX sandboxes_active_idx ON sandboxes (environment_id, updated_at DESC)
    WHERE status IN ('pending', 'provisioning', 'starting', 'running');

ALTER TABLE supervisors
    ADD CONSTRAINT supervisors_sandbox_fk
    FOREIGN KEY (sandbox_id) REFERENCES sandboxes(id) ON DELETE SET NULL;

-- Reusable workload definitions. A template produces a sandbox; it is not a
-- running object.
CREATE TABLE sandbox_templates (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name           text NOT NULL,
    image          text NOT NULL,
    image_digest   text,
    spec           jsonb NOT NULL DEFAULT '{}'::jsonb,
    labels         jsonb NOT NULL DEFAULT '{}'::jsonb,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sandbox_templates_name_unique UNIQUE (environment_id, name)
);

-- Bulk actions run as jobs. Each target is recorded so partial failure is
-- visible per sandbox rather than as one opaque error.
CREATE TABLE job_targets (
    job_id    uuid NOT NULL,
    subject_id uuid NOT NULL,
    state     text NOT NULL DEFAULT 'pending',
    error     text,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (job_id, subject_id),
    CONSTRAINT job_targets_state_valid
        CHECK (state IN ('pending', 'running', 'succeeded', 'failed', 'skipped'))
);
