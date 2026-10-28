-- 0036_model_install_enabled — an installed model can be switched off.
--
-- Several models can stay installed on one node while only the ones the
-- operator needs are running. `enabled = false` means the server is stopped and
-- the reservation is released; the artefact stays on disk, so switching it back
-- on does not download it again. The reserved figures are kept as the operator's
-- setting and count against the node only while the install is enabled.

ALTER TABLE model_installs
    ADD COLUMN enabled boolean NOT NULL DEFAULT true;

COMMENT ON COLUMN model_installs.enabled IS
  'Whether the server runs and the reservation counts. Off keeps the artefact and the setting, and frees the memory.';
