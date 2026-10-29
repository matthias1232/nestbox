-- 0043_owned_sandboxes — a user manages their own sandboxes, an admin all of them.
--
-- A sandbox gets an owner: the user who created it. Row-level security on `sandboxes`
-- then decides who sees it, for every query that touches the table, so no endpoint can
-- forget the check:
--
--   * an admin sees every sandbox of the environment;
--   * any other role sees its own sandboxes and the ones nobody owns (shared: the
--     sandbox of a user who was removed from the environment);
--   * work done by the control plane itself, with no acting user, sees all.
--
-- The acting user and role are set per transaction by the service layer
-- (`app.actor_id`, `app.actor_role`), next to the environment.

ALTER TABLE sandboxes ADD COLUMN owner_user_id uuid REFERENCES users(id) ON DELETE SET NULL;
CREATE INDEX sandboxes_owner_idx ON sandboxes (owner_user_id);

-- Sandboxes from before ownership belong to the first admin of their organization, so
-- adding a user does not make every existing sandbox visible to them.
UPDATE sandboxes s
   SET owner_user_id = (
        SELECT u.id FROM users u
          JOIN environments e ON e.organization_id = u.organization_id
         WHERE e.id = s.environment_id AND u.role = 'admin' AND u.status = 'active'
         ORDER BY u.created_at LIMIT 1)
 WHERE owner_user_id IS NULL;

DROP POLICY IF EXISTS tenant_isolation ON sandboxes;
CREATE POLICY tenant_isolation ON sandboxes
    USING (
        environment_id = current_setting('app.environment_id', true)::uuid
        AND (
            coalesce(current_setting('app.actor_role', true), '') IN ('', 'admin')
            OR owner_user_id IS NULL
            OR owner_user_id::text = coalesce(current_setting('app.actor_id', true), '')
        )
    )
    WITH CHECK (
        environment_id = current_setting('app.environment_id', true)::uuid
        AND (
            coalesce(current_setting('app.actor_role', true), '') IN ('', 'admin')
            OR owner_user_id IS NULL
            OR owner_user_id::text = coalesce(current_setting('app.actor_id', true), '')
        )
    );
