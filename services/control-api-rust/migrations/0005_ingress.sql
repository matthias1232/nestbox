-- 0005_ingress — addressing, certificates, ingress rules, forwards.
--
-- The design that scales: one published port (443) with hostname/SNI routing,
-- so thousands of sandboxes do not need thousands of port mappings. Each
-- sandbox gets its own stable address on an internal pool; external access is a
-- rule, not a port.

CREATE TABLE address_pools (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    cidr           cidr NOT NULL,
    purpose        text NOT NULL DEFAULT 'sandbox',
    allocation     text NOT NULL DEFAULT 'dynamic',
    gateway_id     uuid REFERENCES gateways(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT address_pools_purpose_valid
        CHECK (purpose IN ('sandbox', 'service', 'public')),
    CONSTRAINT address_pools_allocation_valid
        CHECK (allocation IN ('dynamic', 'static'))
);

CREATE TABLE address_assignments (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    pool_id        uuid NOT NULL REFERENCES address_pools(id) ON DELETE RESTRICT,
    ip             inet NOT NULL,
    hostname       text,
    allocated_at   timestamptz NOT NULL DEFAULT now(),
    released_at    timestamptz,
    -- No two live assignments may share an address or a hostname.
    CONSTRAINT address_assignments_ip_unique UNIQUE (pool_id, ip),
    CONSTRAINT address_assignments_hostname_unique UNIQUE (environment_id, hostname)
);

CREATE INDEX address_assignments_active_idx
    ON address_assignments (environment_id) WHERE released_at IS NULL;

-- The public face of a gateway: address, the one published port, overlay address.
CREATE TABLE gateway_endpoints (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    gateway_id         uuid NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
    public_address     text,
    public_port_range  int4range,
    overlay_address    inet,
    status             text NOT NULL DEFAULT 'unknown',
    last_seen          timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT gateway_endpoints_gateway_unique UNIQUE (gateway_id),
    CONSTRAINT gateway_endpoints_status_valid
        CHECK (status IN ('unknown', 'healthy', 'degraded', 'unhealthy'))
);

CREATE TABLE certificates (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    name           text NOT NULL,
    -- acme, internal_ca, imported
    kind           text NOT NULL,
    subject        text NOT NULL,
    sans           text[] NOT NULL DEFAULT '{}',
    issuer         text,
    not_before     timestamptz,
    not_after      timestamptz,
    -- Locator for the private key and chain. Never the key material (ADR 0005).
    secret_ref     text,
    state          text NOT NULL DEFAULT 'pending',
    auto_renew     boolean NOT NULL DEFAULT true,
    version        bigint NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT certificates_name_unique UNIQUE (environment_id, name),
    CONSTRAINT certificates_kind_valid
        CHECK (kind IN ('acme', 'internal_ca', 'imported')),
    CONSTRAINT certificates_state_valid
        CHECK (state IN ('pending', 'valid', 'expiring', 'expired', 'renewing', 'failed'))
);

CREATE INDEX certificates_expiry_idx ON certificates (environment_id, not_after);

-- Hostname routing on the single published port. This is the scaling unit:
-- a new sandbox publishes a hostname, not a port.
CREATE TABLE ingress_rules (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id    uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    name              text NOT NULL,
    hostname          text NOT NULL,
    path_prefix       text NOT NULL DEFAULT '/',
    target_sandbox_id uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    target_port       integer,
    -- http, https, tcp, ssh
    upstream_protocol text NOT NULL DEFAULT 'http',
    certificate_id    uuid REFERENCES certificates(id) ON DELETE SET NULL,
    tls_mode          text NOT NULL DEFAULT 'strict',
    acl               jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- Only for the exception case: an externally reachable TCP service that
    -- cannot use hostname routing.
    external_port     integer,
    priority          integer NOT NULL DEFAULT 100,
    enabled           boolean NOT NULL DEFAULT true,
    version           bigint NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ingress_rules_name_unique UNIQUE (environment_id, name),
    -- One hostname route per environment. This is the constraint that prevents
    -- two sandboxes claiming the same public name.
    CONSTRAINT ingress_rules_hostname_unique UNIQUE (environment_id, hostname),
    CONSTRAINT ingress_rules_tls_mode_valid CHECK (tls_mode IN ('strict', 'off')),
    CONSTRAINT ingress_rules_protocol_valid
        CHECK (upstream_protocol IN ('http', 'https', 'tcp', 'ssh')),
    CONSTRAINT ingress_rules_port_sane
        CHECK (target_port IS NULL OR target_port BETWEEN 1 AND 65535)
);

CREATE INDEX ingress_rules_env_idx ON ingress_rules (environment_id);
CREATE INDEX ingress_rules_sandbox_idx ON ingress_rules (target_sandbox_id);

-- Reserved external port range as an exception. A partial unique index keeps
-- allocations collision-free without forbidding the NULL that most rows have.
CREATE UNIQUE INDEX ingress_rules_external_port_unique
    ON ingress_rules (environment_id, external_port)
    WHERE external_port IS NOT NULL;

-- Port forwarding mirrors the CLI's `forward` command.
CREATE TABLE forwards (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    local_port     integer NOT NULL,
    bind_address   text NOT NULL DEFAULT '127.0.0.1',
    target_port    integer NOT NULL,
    state          text NOT NULL DEFAULT 'pending',
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT forwards_local_port_sane CHECK (local_port BETWEEN 1 AND 65535),
    CONSTRAINT forwards_target_port_sane CHECK (target_port BETWEEN 1 AND 65535),
    CONSTRAINT forwards_state_valid
        CHECK (state IN ('pending', 'active', 'stopped', 'failed'))
);

-- Service exposure mirrors the CLI's `service expose` command.
CREATE TABLE service_exposures (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    name           text,
    port           integer NOT NULL,
    auth_mode      text NOT NULL DEFAULT 'strip',
    url            text,
    state          text NOT NULL DEFAULT 'pending',
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT service_exposures_port_sane CHECK (port BETWEEN 1 AND 65535),
    CONSTRAINT service_exposures_auth_valid
        CHECK (auth_mode IN ('strip', 'bearer_passthrough')),
    CONSTRAINT service_exposures_state_valid
        CHECK (state IN ('pending', 'active', 'stopped', 'failed'))
);
