-- 0039_claude_and_openchamber_images — two packs that were a plain Ubuntu shell.
--
-- `claude-code` and `openchamber` started from `nestbox-ubuntu-dev:local`, where
-- neither `claude` nor `openchamber` exists, so a sandbox created from them had a
-- shell and a published port with nothing to run (ADR 0033). Each now has its own
-- image (services/tool-images/): Claude Code for SSH, and OpenChamber with
-- OpenCode, started in the foreground on port 3000 behind a UI password.
--
-- A sandbox created before this keeps the image it was created with; recreate it
-- to get the new one.

UPDATE agent_packs
   SET default_image = 'nestbox-claude-code:local', updated_at = now()
 WHERE key = 'claude-code';

UPDATE agent_packs
   SET default_image = 'nestbox-openchamber:local',
       default_command = ARRAY['/home/nestbox/.local/bin/nestbox-openchamber'],
       updated_at = now()
 WHERE key = 'openchamber';
