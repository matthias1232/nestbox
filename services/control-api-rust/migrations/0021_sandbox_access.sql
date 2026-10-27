-- 0021_sandbox_access — how an operator reaches a sandbox from outside.
--
-- Three things an "Access" tab needs and the schema did not have:
--
--   * an SSH key pair per sandbox. The private key is a file on the node
--     (0600); this table holds a locator and a fingerprint, never the key
--     (ADR 0005). It can be downloaded once, then only rotated.
--   * a port pool per node, so a published port is unique on that node and a
--     collision is a constraint error rather than a runtime surprise.
--   * an inventory snapshot: what is installed and listening, taken by a fixed
--     script in the runner, so "which Hermes version is in there" has an answer.

CREATE TABLE sandbox_ssh_keys (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id  uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id      uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    public_key      text NOT NULL,
    -- A locator such as file:///etc/nestbox/secrets/ssh/<sandbox>.key.
    private_key_ref text NOT NULL,
    fingerprint     text NOT NULL,
    -- Set on the first download. A second download answers 410 until rotation.
    downloaded_at   timestamptz,
    downloaded_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    rotated_at      timestamptz,
    CONSTRAINT sandbox_ssh_keys_one_per_sandbox UNIQUE (sandbox_id),
    CONSTRAINT sandbox_ssh_keys_ref_is_locator CHECK (private_key_ref ~ '^[a-z]+://')
);

COMMENT ON COLUMN sandbox_ssh_keys.private_key_ref IS
  'Locator only. The private key is a 0600 file on the node; it never enters this database.';

-- Ports a node has handed out. The pool range is a deployment setting; the
-- table is what makes two sandboxes unable to claim the same port.
CREATE TABLE node_port_pool (
    node_id     uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    port        integer NOT NULL,
    sandbox_id  uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    forward_id  uuid REFERENCES forwards(id) ON DELETE SET NULL,
    -- ssh | agent | custom: what the port is for, for the Access table.
    purpose     text NOT NULL DEFAULT 'custom',
    target_port integer NOT NULL,
    claimed_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (node_id, port),
    CONSTRAINT node_port_pool_port_sane CHECK (port BETWEEN 1024 AND 65535),
    CONSTRAINT node_port_pool_target_sane CHECK (target_port BETWEEN 1 AND 65535)
);

CREATE INDEX node_port_pool_sandbox_idx ON node_port_pool (sandbox_id);

-- The address a client outside the node uses. Defaults to the runner host; an
-- operator can override it for a node behind NAT.
ALTER TABLE nodes ADD COLUMN IF NOT EXISTS public_address text;
ALTER TABLE nodes ADD COLUMN IF NOT EXISTS port_pool_start integer NOT NULL DEFAULT 22000;
ALTER TABLE nodes ADD COLUMN IF NOT EXISTS port_pool_end   integer NOT NULL DEFAULT 22999;
ALTER TABLE nodes ADD CONSTRAINT nodes_port_pool_sane
    CHECK (port_pool_start >= 1024 AND port_pool_end <= 65535 AND port_pool_start < port_pool_end);

-- What is inside the container, as a snapshot. Values are facts the fixed
-- inventory script reports; env_keys holds names only, never values.
CREATE TABLE sandbox_inventory (
    sandbox_id     uuid PRIMARY KEY REFERENCES sandboxes(id) ON DELETE CASCADE,
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    taken_at       timestamptz NOT NULL DEFAULT now(),
    os             jsonb NOT NULL DEFAULT '{}'::jsonb,
    packages       jsonb NOT NULL DEFAULT '[]'::jsonb,
    agents         jsonb NOT NULL DEFAULT '{}'::jsonb,
    tool_versions  jsonb NOT NULL DEFAULT '{}'::jsonb,
    listening      jsonb NOT NULL DEFAULT '[]'::jsonb,
    env_keys       text[] NOT NULL DEFAULT '{}',
    mounts         jsonb NOT NULL DEFAULT '[]'::jsonb,
    image          jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- Why the snapshot is partial, if it is: container stopped, script timed out.
    note           text
);

COMMENT ON COLUMN sandbox_inventory.env_keys IS
  'Environment variable names only. A value never enters this table.';
