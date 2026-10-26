-- 0003_policies — the four-section document, two scopes, and rule drafts.
--
-- Scope mirrors the runtime exactly (ADR 0007): a sandbox policy or the single
-- gateway-global override. There is no third scope, because OpenShell cannot
-- enforce one.
--
-- The document is stored whole in `spec` (that is what the gateway receives and
-- what diff/rollback shows) and also normalised into rule tables so that list,
-- filter, and bulk operations do not have to parse JSON on every query.

CREATE TABLE policies (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name           text NOT NULL,
    scope          text NOT NULL DEFAULT 'sandbox',
    -- The complete four-section document: filesystem_policy, landlock, process,
    -- network_policies.
    spec           jsonb NOT NULL,
    version        bigint NOT NULL DEFAULT 1,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT policies_name_unique UNIQUE (environment_id, name),
    CONSTRAINT policies_scope_valid CHECK (scope IN ('sandbox', 'global')),
    CONSTRAINT policies_spec_is_object CHECK (jsonb_typeof(spec) = 'object')
);

-- At most one global policy per environment. The runtime allows one; the
-- database refuses a second rather than leaving it to the application.
CREATE UNIQUE INDEX policies_single_global_idx
    ON policies (environment_id) WHERE scope = 'global';

CREATE TABLE policy_versions (
    policy_id  uuid NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
    version    bigint NOT NULL,
    spec       jsonb NOT NULL,
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (policy_id, version)
);

COMMENT ON TABLE policy_versions IS
  'Immutable revision history. Rollback reads from here rather than re-creating.';

-- A template is a composable fragment. It produces a policy; it is not enforced.
CREATE TABLE policy_templates (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name           text NOT NULL,
    description    text NOT NULL DEFAULT '',
    spec           jsonb NOT NULL,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT policy_templates_name_unique UNIQUE (environment_id, name)
);

-- Which policy applies where. `desired_version` is what the control plane wants;
-- the supervisor reports what it actually loaded.
CREATE TABLE sandbox_policies (
    sandbox_id       uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    policy_id        uuid NOT NULL REFERENCES policies(id) ON DELETE RESTRICT,
    desired_version  bigint NOT NULL,
    applied_version  bigint,
    applied_at       timestamptz,
    precedence       integer NOT NULL DEFAULT 100,
    created_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sandbox_id, policy_id)
);

CREATE INDEX sandbox_policies_sandbox_idx ON sandbox_policies (sandbox_id);

-- ---------------------------------------------------------------------------
-- Normalised network rules
--
-- `network_rules` is the queryable projection of the document's
-- network_policies section. It exists so the console can filter and bulk-edit
-- without loading every document. A service keeps the two in step, and a test
-- asserts the projection equals the document.
-- ---------------------------------------------------------------------------

CREATE TABLE network_rules (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    policy_id      uuid REFERENCES policies(id) ON DELETE CASCADE,
    -- A rule drafted from a single sandbox before it becomes part of a policy.
    sandbox_id     uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    name           text NOT NULL DEFAULT '',
    action         text NOT NULL,
    host           text NOT NULL,
    port           integer,
    protocol       text NOT NULL DEFAULT 'rest',
    methods        text[] NOT NULL DEFAULT '{}',
    binaries       text[] NOT NULL DEFAULT '{}',
    -- enforced blocks; audit records and allows. Audit is how a policy is
    -- developed without breaking the workload.
    enforcement    text NOT NULL DEFAULT 'enforced',
    priority       integer NOT NULL DEFAULT 100,
    enabled        boolean NOT NULL DEFAULT true,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT network_rules_action_valid CHECK (action IN ('allow', 'deny')),
    CONSTRAINT network_rules_protocol_valid
        CHECK (protocol IN ('rest', 'http', 'websocket', 'graphql', 'mcp', 'jsonrpc', 'tcp')),
    CONSTRAINT network_rules_enforcement_valid
        CHECK (enforcement IN ('enforced', 'audit')),
    CONSTRAINT network_rules_target_present
        CHECK (policy_id IS NOT NULL OR sandbox_id IS NOT NULL),
    CONSTRAINT network_rules_port_sane CHECK (port IS NULL OR port BETWEEN 1 AND 65535)
);

CREATE INDEX network_rules_env_idx ON network_rules (environment_id);
CREATE INDEX network_rules_policy_idx ON network_rules (policy_id);
CREATE INDEX network_rules_sandbox_idx ON network_rules (sandbox_id);

-- DNS is a separate concern with a separate rule shape. A name blocklist entry
-- is not a network allowlist entry, and conflating them would make both wrong.
CREATE TABLE dns_rules (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    policy_id      uuid REFERENCES policies(id) ON DELETE CASCADE,
    sandbox_id     uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    action         text NOT NULL,
    -- Exact name or a suffix pattern such as ".example.net".
    pattern        text NOT NULL,
    priority       integer NOT NULL DEFAULT 100,
    enabled        boolean NOT NULL DEFAULT true,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT dns_rules_action_valid CHECK (action IN ('allow', 'deny')),
    CONSTRAINT dns_rules_target_present
        CHECK (policy_id IS NOT NULL OR sandbox_id IS NOT NULL),
    CONSTRAINT dns_rules_pattern_not_empty CHECK (length(pattern) > 0)
);

CREATE INDEX dns_rules_env_idx ON dns_rules (environment_id);

-- How deeply a sandbox is observed. Forward proxy is the default; transparent
-- interception is an explicit, per-sandbox opt-in (the website promises this).
CREATE TABLE egress_modes (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    mode           text NOT NULL,
    -- Locator for the sandbox-scoped trust CA, never the key material.
    trust_ca_ref   text,
    enabled        boolean NOT NULL DEFAULT true,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT egress_modes_mode_valid
        CHECK (mode IN ('forward_proxy', 'transparent', 'hybrid')),
    CONSTRAINT egress_modes_one_per_sandbox UNIQUE (sandbox_id)
);

-- ---------------------------------------------------------------------------
-- Rule drafts — rule-from-event is a two-step workflow (ADR 0011)
-- ---------------------------------------------------------------------------

CREATE TABLE rule_drafts (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id   uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    -- Which telemetry event produced this draft, so the operator can see the
    -- evidence next to the proposal.
    source_event_id  uuid,
    source_kind      text NOT NULL DEFAULT 'network',
    target_type      text NOT NULL,
    target_id        uuid,
    -- The proposed rule, already prefilled and editable.
    draft            jsonb NOT NULL,
    -- The diff against the enforced policy at the time of creation, so an
    -- approver sees exactly what would change.
    diff             jsonb,
    status           text NOT NULL DEFAULT 'pending',
    reason           text,
    created_by       uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_by       uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    decided_at       timestamptz,
    CONSTRAINT rule_drafts_status_valid
        CHECK (status IN ('pending', 'approved', 'rejected', 'stale')),
    CONSTRAINT rule_drafts_source_valid
        CHECK (source_kind IN ('network', 'dns', 'inference')),
    CONSTRAINT rule_drafts_target_valid
        CHECK (target_type IN ('policy', 'sandbox'))
);

CREATE INDEX rule_drafts_env_status_idx ON rule_drafts (environment_id, status);

-- A temporary window that allows traffic for a bounded time, then reverts. This
-- is the safe version of "turn the firewall off for a minute": it expires on
-- its own and it is recorded.
CREATE TABLE traffic_windows (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    scope          text NOT NULL DEFAULT 'sandbox',
    reason         text NOT NULL,
    expires_at     timestamptz NOT NULL,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    revoked_at     timestamptz,
    CONSTRAINT traffic_windows_scope_valid CHECK (scope IN ('sandbox', 'environment')),
    CONSTRAINT traffic_windows_expiry_future CHECK (expires_at > created_at)
);

CREATE INDEX traffic_windows_active_idx ON traffic_windows (environment_id, expires_at)
    WHERE revoked_at IS NULL;
