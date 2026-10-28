-- 0035_sandbox_stacks — which sandboxes belong together.
--
-- An agent such as Hermes needs helper sandboxes to run. Each is a sandbox in its
-- own right (own container, own policy, own traffic), so the relation is not
-- containment but membership: a member points at the sandbox that leads its
-- stack. One level only — a stack is a lead and its members — because a deeper
-- tree would need a reading order the list cannot show at a glance.
--
-- Deleting the lead does not delete the members: they are real sandboxes with
-- their own state, so they become standalone (`ON DELETE SET NULL`) and the
-- console says so before the delete.

ALTER TABLE sandboxes
    ADD COLUMN parent_id uuid REFERENCES sandboxes(id) ON DELETE SET NULL,
    ADD CONSTRAINT sandboxes_not_own_parent CHECK (parent_id IS NULL OR parent_id <> id);

-- Serves "the members of this lead" and the per-stack delete warning.
CREATE INDEX sandboxes_parent_idx ON sandboxes (parent_id) WHERE parent_id IS NOT NULL;

COMMENT ON COLUMN sandboxes.parent_id IS
  'The lead sandbox of the stack this sandbox belongs to. NULL for a lead or a standalone sandbox.';
