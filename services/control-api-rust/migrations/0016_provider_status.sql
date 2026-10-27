-- 0016_provider_status — a provider profile knows whether it answers, and a
-- locally installed model becomes one.
--
-- Installing a model creates a profile with kind `local_openai_compatible`, an
-- endpoint on the node's overlay (`http://model-<key>:8000/v1`), and status
-- `pending`. Sandboxes may already select it; the inference environment only
-- hands out profiles that have probed `reachable`, so a pending model never
-- sends an agent to an endpoint that does not exist yet.

ALTER TABLE provider_profiles
    ADD COLUMN status           text NOT NULL DEFAULT 'reachable',
    ADD COLUMN node_id          uuid REFERENCES nodes(id) ON DELETE SET NULL,
    ADD COLUMN model_install_id uuid REFERENCES model_installs(id) ON DELETE SET NULL,
    ADD COLUMN last_probed_at   timestamptz,
    ADD CONSTRAINT provider_profiles_status_valid CHECK (status IN (
        'pending', 'reachable', 'unreachable'
    ));

-- Existing cloud profiles keep behaving as before: they are usable until a
-- probe says otherwise.
CREATE INDEX provider_profiles_status_idx ON provider_profiles (environment_id, status);
CREATE INDEX provider_profiles_install_idx ON provider_profiles (model_install_id)
    WHERE model_install_id IS NOT NULL;

ALTER TABLE model_installs
    ADD CONSTRAINT model_installs_provider_fk
        FOREIGN KEY (provider_profile_id) REFERENCES provider_profiles(id) ON DELETE SET NULL;

COMMENT ON COLUMN provider_profiles.status IS
  'pending: created for an install whose server is not up. reachable/unreachable: the last probe result.';
