-- 0040_ssh_access — who else may open a sandbox's SSH door, and for how long.
--
-- Until now the only way in was the one key pair the control plane generated.
-- Two more ways, both administered per sandbox:
--
-- * `sandbox_user_keys` — public keys an operator adds (their own laptop key), each
--   with a label, listed and removable;
-- * `sandbox_temp_credentials` — a generated user name and password that works for
--   a set time and can be revoked, for a client that has no key (PuTTY, a phone).
--   Only a salted hash is stored; the password is shown once, at creation.

CREATE TABLE sandbox_user_keys (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    label          text NOT NULL,
    public_key     text NOT NULL,
    fingerprint    text NOT NULL,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sandbox_user_keys_unique UNIQUE (sandbox_id, fingerprint),
    CONSTRAINT sandbox_user_keys_label_sane CHECK (length(label) BETWEEN 1 AND 80)
);

CREATE TABLE sandbox_temp_credentials (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    username       text NOT NULL,
    salt           text NOT NULL,
    password_hash  text NOT NULL,
    expires_at     timestamptz NOT NULL,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    revoked_at     timestamptz,
    CONSTRAINT sandbox_temp_credentials_user_unique UNIQUE (username)
);

CREATE INDEX sandbox_user_keys_sandbox_idx ON sandbox_user_keys (sandbox_id);
CREATE INDEX sandbox_temp_credentials_sandbox_idx ON sandbox_temp_credentials (sandbox_id);

ALTER TABLE sandbox_user_keys ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON sandbox_user_keys
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE sandbox_temp_credentials ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON sandbox_temp_credentials
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

COMMENT ON COLUMN sandbox_temp_credentials.password_hash IS
  'sha256(salt || password), hex. The password is shown once and never stored.';
