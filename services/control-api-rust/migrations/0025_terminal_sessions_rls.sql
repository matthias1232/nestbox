-- 0025 — row-level security for the sandbox sessions.
--
-- `sandbox_sessions` arrived in 0024 carrying an `environment_id`, but 0009 runs
-- only once and its catalogue walk therefore never saw the table. The result was
-- the one table in the schema that records who had a shell in which sandbox and
-- had no tenant policy behind it — the second control of ADR 0004 was missing
-- exactly where an incident review looks.
--
-- The policy has the same shape as every other one in 0009: a row is visible
-- only inside the environment named by `app.environment_id`, which the
-- application sets per transaction. No grants here: 0009's
-- `ALTER DEFAULT PRIVILEGES` already gives the application role CRUD on tables
-- created after it.

ALTER TABLE sandbox_sessions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_isolation ON sandbox_sessions;

CREATE POLICY tenant_isolation ON sandbox_sessions
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

COMMENT ON POLICY tenant_isolation ON sandbox_sessions IS
    'A session is visible only in its own environment; the terminal and SSH '
    'audit must not be readable across tenants.';
