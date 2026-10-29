-- 0045_owned_telemetry — what a sandbox did is visible to whoever may see the sandbox.
--
-- Same rule as sandboxes and roots: an admin (and the control plane) sees everything; anyone
-- else sees telemetry rows, sandbox-specific network rules and supervisors of sandboxes they
-- may see. A row about no sandbox (a gateway, a node) is for admins; rules that belong to a
-- policy stay shared, because a policy is shared. The subqueries run with the caller's own
-- row-level security, so "may see the sandbox" is decided in one place.

DROP POLICY IF EXISTS tenant_isolation ON telemetry_events;
CREATE POLICY tenant_isolation ON telemetry_events
    USING (
        environment_id = current_setting('app.environment_id', true)::uuid
        AND (
            coalesce(current_setting('app.actor_role', true), '') IN ('', 'admin')
            OR (sandbox_id IS NOT NULL AND EXISTS (SELECT 1 FROM sandboxes s WHERE s.id = telemetry_events.sandbox_id))
        )
    )
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

DROP POLICY IF EXISTS tenant_isolation ON network_rules;
CREATE POLICY tenant_isolation ON network_rules
    USING (
        environment_id = current_setting('app.environment_id', true)::uuid
        AND (
            coalesce(current_setting('app.actor_role', true), '') IN ('', 'admin')
            OR sandbox_id IS NULL
            OR EXISTS (SELECT 1 FROM sandboxes s WHERE s.id = network_rules.sandbox_id)
        )
    )
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

DROP POLICY IF EXISTS tenant_isolation ON supervisors;
CREATE POLICY tenant_isolation ON supervisors
    USING (
        EXISTS (
            SELECT 1 FROM gateways g JOIN clusters c ON c.id = g.cluster_id
             WHERE g.id = supervisors.gateway_id
               AND c.environment_id = current_setting('app.environment_id', true)::uuid
        )
        AND (
            coalesce(current_setting('app.actor_role', true), '') IN ('', 'admin')
            OR (sandbox_id IS NOT NULL AND EXISTS (SELECT 1 FROM sandboxes s WHERE s.id = supervisors.sandbox_id))
        )
    )
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM gateways g JOIN clusters c ON c.id = g.cluster_id
             WHERE g.id = supervisors.gateway_id
               AND c.environment_id = current_setting('app.environment_id', true)::uuid
        )
    );
