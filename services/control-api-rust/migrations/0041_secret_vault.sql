-- 0041_secret_vault — API keys an operator types into the console.
--
-- ADR 0005 kept secret *values* out of the API and the database: a profile pointed at
-- a file. That is right for a value an installer places on the host and wrong for the
-- everyday case: an operator with a provider key and a browser. This table holds such
-- keys encrypted (AES-256-GCM, a master key that lives in a file the database never
-- sees), write-only through the API: a value goes in, only its last four characters
-- come back. A provider points at one with the locator `db://<name>`.

CREATE TABLE stored_secrets (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    name           text NOT NULL,
    kind           text NOT NULL DEFAULT 'api_key',
    ciphertext     bytea NOT NULL,
    nonce          bytea NOT NULL,
    -- Last four characters of the value, shown so two keys can be told apart.
    last4          text NOT NULL,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    last_used_at   timestamptz,
    CONSTRAINT stored_secrets_name_unique UNIQUE (environment_id, name),
    CONSTRAINT stored_secrets_name_slug CHECK (name ~ '^[a-z0-9]([a-z0-9._-]{0,62}[a-z0-9])?$'),
    CONSTRAINT stored_secrets_kind_valid CHECK (kind IN ('api_key', 'token', 'password'))
);

ALTER TABLE stored_secrets ENABLE ROW LEVEL SECURITY;
ALTER TABLE stored_secrets FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON stored_secrets
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);
