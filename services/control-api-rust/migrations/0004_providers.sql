-- 0004_providers — tool profiles, provider profiles, secret references.
--
-- A provider profile is the shape that unifies local and cloud models
-- (ADR 0013): `kind` plus an `endpoint` plus a `secret_ref`. A local model is
-- the same row with a loopback endpoint.

CREATE TABLE tool_profiles (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name           text NOT NULL,
    base_image     text NOT NULL,
    packages       text[] NOT NULL DEFAULT '{}',
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tool_profiles_name_unique UNIQUE (environment_id, name)
);

CREATE TABLE provider_profiles (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name           text NOT NULL,
    -- nvidia, openai, anthropic, openai_compatible, omniRoute, local_vllm…
    kind           text NOT NULL,
    endpoint       text NOT NULL,
    model          text,
    -- Locator only. The value never enters this database (ADR 0005).
    secret_ref     text,
    labels         jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- Refresh metadata: strategy, expiry, non-secret material.
    refresh_config jsonb NOT NULL DEFAULT '{}'::jsonb,
    credential_expiry timestamptz,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT provider_profiles_name_unique UNIQUE (environment_id, name),
    CONSTRAINT provider_profiles_endpoint_scheme
        CHECK (endpoint ~ '^https?://')
);

CREATE INDEX provider_profiles_env_idx ON provider_profiles (environment_id);

COMMENT ON COLUMN provider_profiles.secret_ref IS
  'A locator such as file:///etc/nestbox/secrets/nvidia. Never a credential.';

CREATE TABLE secret_references (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    name           text NOT NULL,
    -- A URI: file://, vault://, k8s://. The scheme selects the resolver, so a
    -- future backend is additive rather than a schema change.
    locator        text NOT NULL,
    kind           text NOT NULL DEFAULT 'api_key',
    created_at     timestamptz NOT NULL DEFAULT now(),
    rotated_at     timestamptz,
    CONSTRAINT secret_references_name_unique UNIQUE (environment_id, name),
    CONSTRAINT secret_references_kind_valid
        CHECK (kind IN ('api_key', 'tls_key', 'password', 'token', 'ca')),
    CONSTRAINT secret_references_locator_is_uri
        CHECK (locator ~ '^[a-z][a-z0-9+.-]*://')
);

COMMENT ON TABLE secret_references IS
  'Locators only. A dump of this table yields paths, not credentials.';
