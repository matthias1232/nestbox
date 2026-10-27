-- 0009_rls — row-level security and the least-privilege application role.
--
-- Two independent controls protect tenant data (ADR 0004):
--
--   1. every repository call takes an environment_id (application layer)
--   2. PostgreSQL refuses rows from another environment (database layer)
--
-- The application connects as `nestbox_app`, a non-owner role, so RLS applies to
-- it. Migrations and cross-tenant maintenance connect as the schema owner, which
-- is how the scheduler can iterate environments without disabling a policy.
--
-- The tenant scope is set per transaction:
--     SET LOCAL app.environment_id = '<uuid>'
-- A query without a scope returns no rows rather than all rows: the comparison
-- against a missing setting yields NULL, which is not true.

-- ---------------------------------------------------------------------------
-- Enforce RLS on every table that carries tenant data.
-- ---------------------------------------------------------------------------

DO $$
DECLARE
    t text;
    tenant_tables text[] := ARRAY[
        'clusters', 'gateways', 'workspaces', 'supervisors', 'sandboxes',
        'sandbox_templates', 'policies', 'policy_versions', 'policy_templates',
        'sandbox_policies', 'network_rules', 'dns_rules', 'egress_modes',
        'rule_drafts', 'traffic_windows', 'tool_profiles', 'provider_profiles',
        'secret_references', 'address_pools', 'address_assignments',
        'gateway_endpoints', 'certificates', 'ingress_rules', 'forwards',
        'service_exposures', 'jobs', 'job_logs', 'job_targets', 'schedules',
        'audit_events', 'telemetry_events', 'sandbox_migrations',
        'hardware_profiles', 'inference_requests'
    ];
BEGIN
    FOREACH t IN ARRAY tenant_tables LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    END LOOP;
END $$;

-- Policies are created from a helper so the predicate exists exactly once.
-- A table whose name is in this list but has no policy is a bug caught by the
-- RLS test suite.

DO $$
DECLARE
    r record;
BEGIN
    FOR r IN
        SELECT c.relname AS table_name
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          -- Ordinary tables and partitioned parents both need the policy. The
          -- first version of this only matched 'p', which silently skipped every
          -- regular table and left them without a policy.
          AND c.relkind IN ('r', 'p')
          AND c.relrowsecurity
    LOOP
        -- Drop first so the migration is re-runnable in a rebuilt dev database.
        EXECUTE format('DROP POLICY IF EXISTS tenant_isolation ON %I', r.table_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I
               USING (environment_id = current_setting(''app.environment_id'', true)::uuid)
               WITH CHECK (environment_id = current_setting(''app.environment_id'', true)::uuid)',
            r.table_name
        );
    END LOOP;
END $$;

-- Tables that reach the environment indirectly get a policy that walks the join.
-- `job_logs` hangs off `jobs`; `job_targets` off `jobs` as well.
DROP POLICY IF EXISTS tenant_isolation ON job_logs;
CREATE POLICY tenant_isolation ON job_logs
    USING (EXISTS (
        SELECT 1 FROM jobs j
        WHERE j.id = job_logs.job_id
          AND j.environment_id = current_setting('app.environment_id', true)::uuid
    ))
    WITH CHECK (EXISTS (
        SELECT 1 FROM jobs j
        WHERE j.id = job_logs.job_id
          AND j.environment_id = current_setting('app.environment_id', true)::uuid
    ));

DROP POLICY IF EXISTS tenant_isolation ON job_targets;
CREATE POLICY tenant_isolation ON job_targets
    USING (EXISTS (
        SELECT 1 FROM jobs j
        WHERE j.id = job_targets.job_id
          AND j.environment_id = current_setting('app.environment_id', true)::uuid
    ))
    WITH CHECK (EXISTS (
        SELECT 1 FROM jobs j
        WHERE j.id = job_targets.job_id
          AND j.environment_id = current_setting('app.environment_id', true)::uuid
    ));

-- The model catalogue is shared: a seeded entry has no environment and is
-- visible everywhere, while an environment-specific entry is scoped. A blanket
-- policy here would make the catalogue appear empty.
ALTER TABLE models ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON models;
CREATE POLICY tenant_isolation ON models
    USING (
        environment_id IS NULL
        OR environment_id = current_setting('app.environment_id', true)::uuid
    )
    WITH CHECK (
        environment_id IS NULL
        OR environment_id = current_setting('app.environment_id', true)::uuid
    );

-- `environments` deliberately has no row-level policy. The login path must list
-- every environment a user belongs to before an environment is chosen, and the
-- membership check in `user_scopes` is what authorises access. A policy here
-- would make the switcher empty.

COMMENT ON POLICY tenant_isolation ON sandboxes IS
  'Second line of defence. The repository filter is the first.';

-- ---------------------------------------------------------------------------
-- Least privilege for the application role.
--
-- The role is created by the Postgres init script, which has access to the
-- password secret. This migration only grants, so it works whether the role
-- exists (appliance) or not (a bare test database).
-- ---------------------------------------------------------------------------

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'nestbox_app') THEN
        GRANT USAGE ON SCHEMA public TO nestbox_app;

        GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO nestbox_app;
        GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO nestbox_app;

        -- Future tables created by later migrations inherit the same grants,
        -- so a new table is not silently inaccessible or over-exposed.
        ALTER DEFAULT PRIVILEGES IN SCHEMA public
            GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO nestbox_app;
        ALTER DEFAULT PRIVILEGES IN SCHEMA public
            GRANT USAGE, SELECT ON SEQUENCES TO nestbox_app;

        -- The audit trail is append-only for the application role as well as
        -- in the API. Revoking UPDATE and DELETE makes tampering impossible
        -- rather than merely unimplemented.
        REVOKE UPDATE, DELETE ON audit_events FROM nestbox_app;
    END IF;
END $$;
