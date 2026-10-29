-- 0042_root_backups — point-in-time copies of a persistent root.
--
-- The archive itself is a file on the node (the runner writes and reads it); this table
-- is the catalogue: what was backed up, when, by whom, how big, and its SHA-256, which a
-- restore checks before it touches anything. A backup outlives the record of its root:
-- an archived or purged root leaves its backups restorable into another root.

CREATE TABLE root_backups (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    node_id        uuid NOT NULL REFERENCES nodes(id) ON DELETE RESTRICT,
    root_id        uuid REFERENCES persistent_roots(id) ON DELETE SET NULL,
    -- What the root was called, kept for the list once the root is gone.
    root_slug      text NOT NULL,
    root_kind      text NOT NULL,
    note           text,
    bytes          bigint NOT NULL,
    sha256         text NOT NULL,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT root_backups_sha_shape CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT root_backups_note_sane CHECK (note IS NULL OR length(note) <= 200)
);

CREATE INDEX root_backups_root_idx ON root_backups (root_id, created_at DESC);

ALTER TABLE root_backups ENABLE ROW LEVEL SECURITY;
ALTER TABLE root_backups FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON root_backups
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);
