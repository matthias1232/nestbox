-- 0038_pack_access — what each pack's ports are for, and how its sandboxes link.
--
-- Three changes, all catalog data (ADR 0013): no `match` arm learns a pack name.
--
-- 1. `agent_packs.role` says whether a pack is an agent (something an operator
--    talks to), a tool (a helper an agent uses) or a gateway (a front door).
--    Creating a tool or gateway sandbox without naming a lead joins it to the one
--    agent sandbox on its node, when there is exactly one (ADR 0032).
--
-- 2. `agent_packs.access` is the description the Access panel shows next to
--    every port and the line it offers for SSH:
--
--      {
--        "summary": "one sentence: what this sandbox is",
--        "ssh":   { "description": "...", "run": "claude" },
--        "ports": { "3000": { "label": "...", "kind": "web|api",
--                             "description": "...", "path": "/" } },
--        "agent": { "description": "...", "command": "opencode attach {url}" }
--      }
--
--    `run` is what to type after the ssh line to start the agent; `{url}` in an
--    agent command is the dial address of the first web/api port.
--
-- 3. The node's published range is now 100 ports, 22200-22299, not 1000. The
--    runner opens a listener per forward and compose publishes the range on the
--    node; publishing a thousand ports starts a thousand relay processes. A node
--    that needs more sets the range in compose and in `nodes` together.

ALTER TABLE agent_packs ADD COLUMN role text NOT NULL DEFAULT 'agent';
ALTER TABLE agent_packs ADD CONSTRAINT agent_packs_role_valid
    CHECK (role IN ('agent', 'tool', 'gateway'));
ALTER TABLE agent_packs ADD COLUMN access jsonb NOT NULL DEFAULT '{}'::jsonb;

-- `ubuntu-dev` stays an agent-role pack on purpose: it is the plain shell people
-- create for themselves, and it should not be pulled into another sandbox's stack.
UPDATE agent_packs SET role = 'tool'    WHERE key = 'git';
UPDATE agent_packs SET role = 'gateway' WHERE key = 'openclaw';

UPDATE agent_packs SET access = jsonb_build_object(
    'summary', 'Anthropic''s coding agent. It has no web interface: connect with SSH and run claude.',
    'ssh', jsonb_build_object(
        'description', 'A shell in the sandbox. Run claude to start the agent; it asks the model API through the egress proxy.',
        'run', 'claude')
) WHERE key = 'claude-code';

UPDATE agent_packs SET access = jsonb_build_object(
    'summary', 'Nous Research''s agent. Use it from a shell over SSH, or reach its endpoint on the published port.',
    'ssh', jsonb_build_object(
        'description', 'A shell in the sandbox. Run hermes for the interactive agent, or hermes -z "question" for one answer.',
        'run', 'hermes'),
    'ports', jsonb_build_object(
        '8640', jsonb_build_object(
            'label', 'Hermes endpoint',
            'kind', 'api',
            'description', 'The port the Hermes agent listens on. Nothing answers until the agent is started inside the sandbox.',
            'path', '/'))
) WHERE key = 'hermes';

UPDATE agent_packs SET access = jsonb_build_object(
    'summary', 'Web front end for OpenCode. Open the web port in a browser; SSH gives a shell next to it.',
    'ssh', jsonb_build_object(
        'description', 'A shell in the sandbox, for starting and inspecting OpenChamber.',
        'run', 'bash'),
    'ports', jsonb_build_object(
        '3000', jsonb_build_object(
            'label', 'OpenChamber web UI',
            'kind', 'web',
            'description', 'The OpenChamber browser interface. Nothing answers until OpenChamber is started inside the sandbox.',
            'path', '/'))
) WHERE key = 'openchamber';

UPDATE agent_packs SET access = jsonb_build_object(
    'summary', 'Terminal coding agent with provider routing. Run it over SSH, or attach a local client to its server port.',
    'ssh', jsonb_build_object(
        'description', 'A shell in the sandbox. Run opencode for the terminal UI.',
        'run', 'opencode'),
    'ports', jsonb_build_object(
        '4096', jsonb_build_object(
            'label', 'OpenCode server',
            'kind', 'api',
            'description', 'The OpenCode server port, for a client on your machine. Start it inside the sandbox with opencode serve.',
            'path', '/')),
    'agent', jsonb_build_object(
        'description', 'Attach the OpenCode terminal client on your machine to this sandbox.',
        'command', 'opencode attach {url}')
) WHERE key = 'opencode';

UPDATE agent_packs SET access = jsonb_build_object(
    'summary', 'Gateway that fronts agents. Its web port is the gateway''s own interface.',
    'ports', jsonb_build_object(
        '8080', jsonb_build_object(
            'label', 'OpenClaw gateway',
            'kind', 'web',
            'description', 'The gateway''s interface and health endpoint.',
            'path', '/'))
) WHERE key = 'openclaw';

UPDATE agent_packs SET access = jsonb_build_object(
    'summary', 'A plain shell with the baseline tools. Connect with SSH.',
    'ssh', jsonb_build_object('description', 'A shell in the sandbox.', 'run', 'bash')
) WHERE key IN ('ubuntu-dev', 'git') AND access = '{}'::jsonb;

-- Ports claimed before this change keep working: the standard node is only
-- narrowed when every port it has handed out is already inside the new range,
-- and the default for new nodes changes.
ALTER TABLE nodes ALTER COLUMN port_pool_start SET DEFAULT 22200;
ALTER TABLE nodes ALTER COLUMN port_pool_end   SET DEFAULT 22299;
UPDATE nodes SET port_pool_start = 22200, port_pool_end = 22299
 WHERE port_pool_start = 22000 AND port_pool_end = 22999
   AND NOT EXISTS (SELECT 1 FROM node_port_pool p
                    WHERE p.node_id = nodes.id AND p.port NOT BETWEEN 22200 AND 22299);

COMMENT ON COLUMN agent_packs.role IS
  'agent | tool | gateway. A tool or gateway sandbox created without a lead joins the node''s only agent.';
COMMENT ON COLUMN agent_packs.access IS
  'What the Access panel says about this pack: summary, ssh line, per-port label and kind, agent command.';
