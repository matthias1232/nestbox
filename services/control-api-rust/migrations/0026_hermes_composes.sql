-- 0026_hermes_composes — Hermes is installed into a recipe, not shipped as its
-- own image.
--
-- An `install_kind = image` pack cannot be merged into a recipe: two base
-- images have no honest union. That left the only agent anyone actually runs on
-- this appliance unable to appear in a recipe at all. Its installer is the same
-- one the prebuilt image ran, so the pack becomes a script install and the
-- recipe installs it like any other tool.
--
-- `default_image` stays: a plain sandbox created with `pack_key = hermes` still
-- starts from the prebuilt image, which is faster than installing at create
-- time. Only the recipe path changes.

UPDATE agent_packs
   SET install_kind = 'script',
       install_ref = 'https://hermes-agent.nousresearch.com/install.sh',
       updated_at = now()
 WHERE key = 'hermes'
   AND install_kind = 'image';

COMMENT ON COLUMN agent_packs.install_kind IS
  'How a recipe installs this pack into a baseline image. `image` packs cannot compose and run as their own sandbox.';
