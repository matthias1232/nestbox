-- 0024_agent_ports — the ports each agent listens on, so the Access tab can
-- say "OpenChamber is on 192.168.55.50:22003".
--
-- These are the ports the agent's own UI or API listens on inside the
-- container. The control plane publishes each on the node from the port pool
-- (0021) and records the mapping in forwards + node_port_pool. SSH (22) is
-- added for every sandbox regardless of pack.

UPDATE agent_packs SET ports = ARRAY[8640]  WHERE key = 'hermes'     AND ports = '{}';
UPDATE agent_packs SET ports = ARRAY[18789] WHERE key = 'openclaw'   AND ports = '{}';
UPDATE agent_packs SET ports = ARRAY[6000]  WHERE key = 'openchamber' AND ports = '{}';
UPDATE agent_packs SET ports = ARRAY[4096]  WHERE key = 'opencode'   AND ports = '{}';

-- A forward now records what it is for, so the Access table has a label.
ALTER TABLE forwards ADD COLUMN IF NOT EXISTS purpose text NOT NULL DEFAULT 'custom';
ALTER TABLE forwards ADD COLUMN IF NOT EXISTS label   text;
ALTER TABLE forwards ADD CONSTRAINT forwards_purpose_valid
    CHECK (purpose IN ('ssh', 'agent', 'custom'));

-- The sandbox's own terminal and SSH sessions, for the audit trail: who opened
-- what, when, and for how long. Never the content.
CREATE TABLE sandbox_sessions (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    user_id        uuid REFERENCES users(id) ON DELETE SET NULL,
    -- terminal | ssh
    kind           text NOT NULL,
    opened_at      timestamptz NOT NULL DEFAULT now(),
    closed_at      timestamptz,
    -- idle | user | error | sandbox_stopped
    close_reason   text,
    bytes_in       bigint NOT NULL DEFAULT 0,
    bytes_out      bigint NOT NULL DEFAULT 0,
    CONSTRAINT sandbox_sessions_kind_valid CHECK (kind IN ('terminal', 'ssh'))
);

CREATE INDEX sandbox_sessions_sandbox_idx ON sandbox_sessions (sandbox_id, opened_at DESC);
CREATE INDEX sandbox_sessions_open_idx ON sandbox_sessions (sandbox_id) WHERE closed_at IS NULL;

COMMENT ON TABLE sandbox_sessions IS
  'Terminal and SSH sessions: who, when, how long, how many bytes. Content is never recorded.';
