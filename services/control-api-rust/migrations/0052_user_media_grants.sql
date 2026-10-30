-- 0052_user_media_grants — which media models a person may see and choose.
--
-- The environment allow-list answers "what may this appliance offer?". It cannot
-- answer "what may this person use?": two operators in one environment may need
-- different image budgets, and a viewer may need to see only one provider.
--
-- One row per person and environment:
--
--   * no row                → the person inherits the environment allow-list;
--   * `provider_ids = '{}'`  → the person may see every provider the environment
--                             allows (the same as inheriting, written explicitly);
--   * a non-empty list      → only those providers, intersected with the
--                             environment allow-list;
--   * `can_configure = false` → the person may see the models but not change a
--                             sandbox's choice, even with `sandbox.write`.
--
-- The table grants nothing by itself: every read still goes through the
-- environment allow-list, and an admin is never restricted by it.

CREATE TABLE user_media_grants (
    user_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    provider_ids   uuid[] NOT NULL DEFAULT '{}',
    can_configure  boolean NOT NULL DEFAULT true,
    updated_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, environment_id)
);

CREATE INDEX user_media_grants_env_idx ON user_media_grants (environment_id);

ALTER TABLE user_media_grants ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_media_grants FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON user_media_grants
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);
