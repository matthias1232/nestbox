-- 0001_core — the tenancy backbone.
--
-- Every other table hangs off an environment. An environment belongs to an
-- organization, and the chain is what makes "one tenant never sees another"
-- expressible as a database constraint rather than an application promise.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ---------------------------------------------------------------------------
-- Organizations and environments
-- ---------------------------------------------------------------------------

CREATE TABLE organizations (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL,
    slug        text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT organizations_slug_unique UNIQUE (slug),
    CONSTRAINT organizations_slug_format CHECK (slug ~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$')
);

COMMENT ON TABLE organizations IS
  'Top-level tenant. Holds environments; holds users.';

CREATE TABLE environments (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    name            text NOT NULL,
    slug            text NOT NULL,
    timezone        text NOT NULL DEFAULT 'UTC',
    -- Telemetry older than this is dropped by partition. Retention is a product
    -- requirement, so it is modelled rather than configured later.
    retention_days  integer NOT NULL DEFAULT 30,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT environments_slug_unique UNIQUE (organization_id, slug),
    CONSTRAINT environments_slug_format CHECK (slug ~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'),
    CONSTRAINT environments_retention_sane CHECK (retention_days BETWEEN 1 AND 3650)
);

COMMENT ON TABLE environments IS
  'The tenant boundary. Every tenant-scoped row carries environment_id.';

-- ---------------------------------------------------------------------------
-- Clusters and nodes
--
-- A node is a machine. A cluster is a group of gateways that can host the same
-- workloads. A sandbox is placed in a cluster and lands on a gateway.
-- ---------------------------------------------------------------------------

CREATE TABLE clusters (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id   uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name             text NOT NULL,
    slug             text NOT NULL,
    -- How placement chooses a gateway: least_loaded, zone_aware, pinned.
    placement_policy text NOT NULL DEFAULT 'least_loaded',
    created_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT clusters_slug_unique UNIQUE (environment_id, slug),
    CONSTRAINT clusters_placement_valid
        CHECK (placement_policy IN ('least_loaded', 'zone_aware', 'pinned'))
);

CREATE TABLE nodes (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name         text NOT NULL,
    zone         text NOT NULL DEFAULT 'default',
    region       text NOT NULL DEFAULT 'local',
    endpoint     text,
    -- Free-form facts a scheduler may need: gpu count, memory, labels.
    capabilities jsonb NOT NULL DEFAULT '{}'::jsonb,
    last_seen    timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT nodes_name_unique UNIQUE (name)
);

-- ---------------------------------------------------------------------------
-- Users, membership, sessions
-- ---------------------------------------------------------------------------

CREATE TABLE users (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    email           text NOT NULL,
    display_name    text NOT NULL,
    -- Argon2id hash. The plaintext is never stored, logged, or returned.
    password_hash   text NOT NULL,
    -- Organization-wide role. Per-environment overrides live in user_scopes.
    role            text NOT NULL DEFAULT 'viewer',
    status          text NOT NULL DEFAULT 'active',
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_email_unique UNIQUE (organization_id, email),
    CONSTRAINT users_role_valid CHECK (role IN ('admin', 'operator', 'viewer')),
    CONSTRAINT users_status_valid CHECK (status IN ('active', 'disabled'))
);

CREATE TABLE user_scopes (
    user_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    role           text NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, environment_id),
    CONSTRAINT user_scopes_role_valid CHECK (role IN ('admin', 'operator', 'viewer'))
);

COMMENT ON TABLE user_scopes IS
  'Per-environment role. Absence of a row means no access to that environment.';

CREATE TABLE sessions (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Only the hash of the token is stored, so a database dump does not yield
    -- usable sessions.
    token_hash    text NOT NULL,
    csrf_hash     text NOT NULL,
    expires_at    timestamptz NOT NULL,
    idle_deadline timestamptz NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sessions_token_unique UNIQUE (token_hash)
);

CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expiry_idx ON sessions (expires_at);

-- API tokens for automation. Separate from sessions because they do not expire
-- on idle and they are revocable individually.
CREATE TABLE api_tokens (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         text NOT NULL,
    token_hash   text NOT NULL,
    last_used_at timestamptz,
    expires_at   timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    revoked_at   timestamptz,
    CONSTRAINT api_tokens_hash_unique UNIQUE (token_hash)
);

CREATE INDEX api_tokens_user_idx ON api_tokens (user_id) WHERE revoked_at IS NULL;
