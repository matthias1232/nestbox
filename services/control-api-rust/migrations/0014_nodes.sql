-- 0014_nodes — a sandbox and a model run on a node, not "in the system".
--
-- `nodes` already exists from 0001 as the host a gateway runs on: a global row
-- with a name, zone, region, and free-form capabilities. This migration makes
-- it the execution host for sandboxes and model installs as well, so there is
-- one notion of "node", not two.
--
-- A node is a machine: the one the control plane runs on today, a VM, a WSL2
-- instance, a bare-metal GPU box, later a Kubernetes cluster. Each node has its
-- own runner, its own storage, and its own hardware. The control plane
-- coordinates nodes; it is not one of them.
--
-- Nodes stay global rather than tenant-scoped: a machine is shared by every
-- environment on it, the way a gateway's host already is. What is tenant-scoped
-- is what runs on a node (sandboxes, installs, hardware profiles), and those
-- tables carry their own environment_id.
--
-- Phase 1 has exactly one node, `local`, seeded in 0008. It cannot be deleted
-- and there is no write endpoint for nodes yet: a node without a runner behind
-- it would be a row that can do nothing.

ALTER TABLE nodes
    ADD COLUMN display_name  text,
    -- local: the host the control plane runs on. vm, wsl, bare_metal: a
    -- remote host with its own runner. kubernetes: declared, no adapter yet.
    ADD COLUMN kind          text NOT NULL DEFAULT 'local',
    ADD COLUMN status        text NOT NULL DEFAULT 'unknown',
    ADD COLUMN is_local      boolean NOT NULL DEFAULT false,
    -- Where this node's runner answers. Empty for the local node, whose runner
    -- URL is the control plane's own configuration.
    ADD COLUMN runner_url    text NOT NULL DEFAULT '',
    -- Whether containers on this node can be given a GPU (NVIDIA Container
    -- Toolkit registered). Measured by the runner, never assumed from the host.
    ADD COLUMN container_gpu boolean NOT NULL DEFAULT false,
    ADD COLUMN description   text,
    ADD COLUMN updated_at    timestamptz NOT NULL DEFAULT now(),
    ADD CONSTRAINT nodes_kind_valid CHECK (kind IN (
        'local', 'vm', 'wsl', 'bare_metal', 'kubernetes'
    )),
    ADD CONSTRAINT nodes_status_valid CHECK (status IN (
        'online', 'offline', 'degraded', 'unknown'
    )),
    ADD CONSTRAINT nodes_runner_url_scheme
        CHECK (runner_url = '' OR runner_url ~ '^https?://');

-- Exactly one local node.
CREATE UNIQUE INDEX nodes_one_local ON nodes ((true)) WHERE is_local;

UPDATE nodes
   SET display_name = 'This machine', kind = 'local', status = 'online', is_local = true
 WHERE name = 'local';

UPDATE nodes SET display_name = name WHERE display_name IS NULL;
ALTER TABLE nodes ALTER COLUMN display_name SET NOT NULL;

COMMENT ON COLUMN nodes.container_gpu IS
  'True when the runner reports an NVIDIA container runtime. GPU models are no_gpu on a node without it, whatever the host sees.';

-- ---------------------------------------------------------------------------
-- Sandboxes and hardware profiles belong to a node.
-- ---------------------------------------------------------------------------

ALTER TABLE sandboxes ADD COLUMN node_id uuid REFERENCES nodes(id) ON DELETE RESTRICT;

UPDATE sandboxes
   SET node_id = (SELECT id FROM nodes WHERE is_local LIMIT 1)
 WHERE node_id IS NULL;

ALTER TABLE sandboxes ALTER COLUMN node_id SET NOT NULL;
CREATE INDEX sandboxes_node_idx ON sandboxes (node_id);

ALTER TABLE hardware_profiles ADD COLUMN node_id uuid REFERENCES nodes(id) ON DELETE CASCADE;

UPDATE hardware_profiles
   SET node_id = (SELECT id FROM nodes WHERE is_local LIMIT 1)
 WHERE node_id IS NULL;

ALTER TABLE hardware_profiles ALTER COLUMN node_id SET NOT NULL;
CREATE INDEX hardware_profiles_node_idx ON hardware_profiles (node_id, detected_at DESC);
