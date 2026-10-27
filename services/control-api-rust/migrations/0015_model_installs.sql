-- 0015_model_installs — a model is installed on a node, not "in the system".
--
-- The catalog row (`models`) describes a model; an install row describes one
-- copy of it on one node. The same model can be installed on two nodes, and a
-- node's free memory is its measured capacity minus what its installs reserve.
--
-- Install state therefore leaves the catalog table. The columns stay for one
-- release behind a view so a reader of the old shape still gets an answer, but
-- the service reads and writes `model_installs` only.

CREATE TABLE model_installs (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id       uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    model_key            text NOT NULL REFERENCES models(key) ON DELETE CASCADE,
    node_id              uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    state                text NOT NULL DEFAULT 'installing',
    installed_version    text,
    -- What this install holds on the node while it exists. Recommended figure
    -- where the catalog has one, minimum otherwise, so a second model that no
    -- longer fits is reported as such instead of oversubscribing the node.
    reserved_ram_bytes   bigint NOT NULL DEFAULT 0,
    reserved_vram_bytes  bigint NOT NULL DEFAULT 0,
    -- The provider profile sandboxes use to reach it. Created pending at
    -- install time, flipped to reachable by the probe once the server answers.
    provider_profile_id  uuid,
    job_id               uuid,
    error                text,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_installs_unique UNIQUE (environment_id, model_key, node_id),
    CONSTRAINT model_installs_state_valid CHECK (state IN (
        'installing', 'installed', 'updating', 'uninstalling', 'failed'
    )),
    CONSTRAINT model_installs_reservation_sane
        CHECK (reserved_ram_bytes >= 0 AND reserved_vram_bytes >= 0)
);

CREATE INDEX model_installs_env_idx  ON model_installs (environment_id);
CREATE INDEX model_installs_node_idx ON model_installs (node_id);

ALTER TABLE model_installs ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON model_installs
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

-- Carry any existing install state over to the local node, then stop using the
-- catalog columns for state.
INSERT INTO model_installs (environment_id, model_key, node_id, state, installed_version)
SELECT e.id, m.key, n.id, m.state, m.installed_version
  FROM models m
  CROSS JOIN environments e
  JOIN nodes n ON n.is_local
 WHERE m.state IN ('installing', 'installed', 'updating', 'uninstalling', 'failed')
   AND (m.environment_id IS NULL OR m.environment_id = e.id)
ON CONFLICT (environment_id, model_key, node_id) DO NOTHING;

UPDATE models SET state = 'not_installed', installed_version = NULL, installed_at = NULL
 WHERE state <> 'remote';

-- ---------------------------------------------------------------------------
-- Where a catalog row came from, so a synced row and a hand-written one are
-- told apart and the sync never overwrites a curated number.
-- ---------------------------------------------------------------------------

ALTER TABLE models
    ADD COLUMN source        text NOT NULL DEFAULT 'seed',
    ADD COLUMN source_id     text,
    ADD COLUMN synced_at     timestamptz,
    -- A pinned row is curated by hand; the sync may add siblings but never
    -- changes it.
    ADD COLUMN pinned        boolean NOT NULL DEFAULT true,
    -- Whether build.nvidia.com lists it as a NIM, which is informational: the
    -- catalog does not fetch NIMs in this phase.
    ADD COLUMN nim_available boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT models_source_valid CHECK (source IN (
        'seed', 'huggingface', 'ollama', 'ngc'
    ));

CREATE INDEX models_source_idx ON models (source);
CREATE UNIQUE INDEX models_source_id_idx ON models (source, source_id) WHERE source_id IS NOT NULL;

COMMENT ON COLUMN models.pinned IS
  'Curated by hand. The catalog sync never updates a pinned row.';

-- The catalog sync runs daily; the job kind is handled by the service layer.
INSERT INTO schedules (environment_id, name, kind, cron, payload)
SELECT id, 'catalog-sync', 'catalog_sync', '15 3 * * *', '{}'::jsonb FROM environments
ON CONFLICT (environment_id, name) DO NOTHING;
