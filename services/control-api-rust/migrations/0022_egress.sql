-- 0022_egress — the proxy that every sandbox connection goes through, and the
-- evidence it leaves behind.
--
-- Until now the sandbox network was internal and the gateway answered 403 to
-- everything: no egress, no enforcement, no real telemetry. From here on an
-- egress proxy on each node is the only way out. It asks the control API for a
-- decision per request (default deny), and every decision becomes a row.
--
-- Three tables, each with its own retention, so the small hot table
-- (telemetry_events) stays small:
--
--   * policy_checks   — every allow/deny/audit decision, 30 days;
--   * traffic_bodies  — request/response bodies when captured, 24 hours;
--   * mcp_tool_calls  — one row per MCP tools/call, with the tool name and
--                       (redacted) arguments, 7 days.

-- The node runs the proxy; the proxy reports which policy set it enforces.
ALTER TABLE nodes
    ADD COLUMN IF NOT EXISTS egress_proxy_url      text,
    ADD COLUMN IF NOT EXISTS policy_hash_reported  text,
    ADD COLUMN IF NOT EXISTS policy_hash_reported_at timestamptz,
    -- A node-scoped bearer the proxy and runner use for ingest and policy
    -- checks. Locator only; the value is a 0600 file on both sides.
    ADD COLUMN IF NOT EXISTS node_token_ref        text;

ALTER TABLE nodes ADD CONSTRAINT nodes_egress_url_scheme
    CHECK (egress_proxy_url IS NULL OR egress_proxy_url ~ '^https?://');
ALTER TABLE nodes ADD CONSTRAINT nodes_node_token_ref_is_locator
    CHECK (node_token_ref IS NULL OR node_token_ref ~ '^[a-z]+://');

-- The hash of the rule set the control API last compiled for an environment.
-- "Deployed" in the console means nodes.policy_hash_reported == this.
CREATE TABLE policy_compilations (
    environment_id uuid PRIMARY KEY REFERENCES environments(id) ON DELETE CASCADE,
    hash           text NOT NULL,
    rule_count     integer NOT NULL DEFAULT 0,
    window_count   integer NOT NULL DEFAULT 0,
    compiled_at    timestamptz NOT NULL DEFAULT now()
);

-- A traffic window carries a ceiling so "allow everything" cannot be left open
-- by accident.
ALTER TABLE traffic_windows
    ADD COLUMN IF NOT EXISTS max_duration_minutes integer NOT NULL DEFAULT 60;
ALTER TABLE traffic_windows ADD CONSTRAINT traffic_windows_duration_sane
    CHECK (max_duration_minutes BETWEEN 1 AND 1440);

-- Every decision the proxy asked for. Partitioned by day; 30 d retention.
CREATE TABLE policy_checks (
    id             uuid NOT NULL DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL,
    sandbox_id     uuid,
    node_id        uuid,
    ts             timestamptz NOT NULL DEFAULT now(),
    host           text NOT NULL,
    port           integer NOT NULL,
    protocol       text NOT NULL DEFAULT 'tcp',
    method         text,
    path           text,
    -- allow | deny | audit
    decision       text NOT NULL,
    -- rule | window | default_deny | backend_unavailable
    reason         text NOT NULL,
    rule_id        uuid,
    window_id      uuid,
    cache_hit      boolean NOT NULL DEFAULT false,
    PRIMARY KEY (id, ts),
    CONSTRAINT policy_checks_decision_valid CHECK (decision IN ('allow', 'deny', 'audit')),
    CONSTRAINT policy_checks_reason_valid
        CHECK (reason IN ('rule', 'window', 'default_deny', 'backend_unavailable'))
) PARTITION BY RANGE (ts);

CREATE INDEX policy_checks_sandbox_ts_idx ON policy_checks (sandbox_id, ts DESC);
CREATE INDEX policy_checks_env_decision_ts_idx ON policy_checks (environment_id, decision, ts DESC);
CREATE TABLE policy_checks_default PARTITION OF policy_checks DEFAULT;

-- Bodies are large and short-lived; a separate table keeps the hot event table
-- small and lets retention run independently. Everything here has passed the
-- redactor before insert (security::redaction), which is the only guarantee a
-- "log everything" setting can honestly make.
CREATE TABLE traffic_bodies (
    event_id       uuid NOT NULL,
    environment_id uuid NOT NULL,
    sandbox_id     uuid,
    ts             timestamptz NOT NULL DEFAULT now(),
    request_body   text,
    response_body  text,
    request_bytes  bigint NOT NULL DEFAULT 0,
    response_bytes bigint NOT NULL DEFAULT 0,
    -- true when the proxy hit its buffer limit and the body is a prefix.
    truncated      boolean NOT NULL DEFAULT false,
    PRIMARY KEY (event_id, ts)
) PARTITION BY RANGE (ts);

CREATE INDEX traffic_bodies_sandbox_ts_idx ON traffic_bodies (sandbox_id, ts DESC);
CREATE TABLE traffic_bodies_default PARTITION OF traffic_bodies DEFAULT;

-- One MCP tool call, as the proxy saw it on the wire. The arguments are the
-- redacted JSON the agent sent; the result is the redacted JSON the server
-- returned, cut at 16 KiB.
CREATE TABLE mcp_tool_calls (
    id             uuid NOT NULL DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL,
    sandbox_id     uuid,
    ts             timestamptz NOT NULL DEFAULT now(),
    server_host    text NOT NULL,
    server_key     text,
    session_id     text,
    -- initialize | tools/list | tools/call | resources/read | prompts/get | notification
    method         text NOT NULL,
    tool_name      text,
    arguments      jsonb,
    result         jsonb,
    is_error       boolean NOT NULL DEFAULT false,
    duration_ms    integer,
    request_id     text,
    event_id       uuid,
    PRIMARY KEY (id, ts)
) PARTITION BY RANGE (ts);

CREATE INDEX mcp_tool_calls_sandbox_ts_idx ON mcp_tool_calls (sandbox_id, ts DESC);
CREATE INDEX mcp_tool_calls_tool_ts_idx ON mcp_tool_calls (environment_id, tool_name, ts DESC);
CREATE TABLE mcp_tool_calls_default PARTITION OF mcp_tool_calls DEFAULT;

-- Pre-create today and the next two days for each of the three partitioned
-- tables, so a fresh install works before the maintenance job runs.
DO $$
DECLARE
    tables text[] := ARRAY['policy_checks', 'traffic_bodies', 'mcp_tool_calls'];
    t text;
    i integer;
    part_start date;
    part_end date;
BEGIN
    FOREACH t IN ARRAY tables LOOP
        FOR i IN 0..2 LOOP
            part_start := current_date + i;
            part_end := current_date + i + 1;
            EXECUTE format(
                'CREATE TABLE IF NOT EXISTS %I PARTITION OF %I FOR VALUES FROM (%L) TO (%L)',
                format('%s_%s', t, to_char(part_start, 'YYYYMMDD')), t, part_start, part_end
            );
        END LOOP;
    END LOOP;
END $$;

-- The hot event table is read by sandbox and time more than anything else now.
CREATE INDEX IF NOT EXISTS telemetry_events_sandbox_ts_idx
    ON telemetry_events (sandbox_id, ts DESC);

INSERT INTO schedules (environment_id, name, kind, cron, payload) VALUES
    ('00000000-0000-0000-0000-000000000002', 'egress-partition-maintenance',
     'egress_partition_maintenance', '20 0 * * *',
     '{"days_ahead": 2, "retain": {"policy_checks": 30, "traffic_bodies": 1, "mcp_tool_calls": 7}}'::jsonb)
ON CONFLICT (environment_id, name) DO NOTHING;

COMMENT ON TABLE policy_checks IS
  'Every egress decision the proxy asked for. Default deny; 30 d retention.';
COMMENT ON TABLE traffic_bodies IS
  'Request/response bodies after redaction, 64 KiB cap, 24 h retention.';
COMMENT ON TABLE mcp_tool_calls IS
  'MCP JSON-RPC calls as seen by the proxy: method, tool, redacted arguments. 7 d retention.';
