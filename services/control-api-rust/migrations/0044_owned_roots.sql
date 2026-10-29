-- 0044_owned_roots — a user's persistent data is theirs, like their sandboxes.
--
-- Same rule as 0043: an admin (and the control plane itself) sees every persistent root;
-- anyone else sees the roots they own and the ones nobody owns. Backups follow their root:
-- one is visible when its root is, and a backup whose root is gone is visible to admins only,
-- because nobody can say whose it was.

DROP POLICY IF EXISTS tenant_isolation ON persistent_roots;
CREATE POLICY tenant_isolation ON persistent_roots
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

DROP POLICY IF EXISTS tenant_isolation ON root_backups;
CREATE POLICY tenant_isolation ON root_backups
    USING (
        environment_id = current_setting('app.environment_id', true)::uuid
        AND (
            coalesce(current_setting('app.actor_role', true), '') IN ('', 'admin')
            OR (root_id IS NOT NULL AND EXISTS (SELECT 1 FROM persistent_roots r WHERE r.id = root_backups.root_id))
        )
    )
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);
