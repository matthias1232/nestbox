-- 0028_node_token_locator — the local node names the token the proxy presents.
--
-- `nodes.node_token_ref` existed from the start and nothing set it, so
-- `authenticate_node` answered "not configured" for every request: the egress
-- proxy could never authenticate, and every proxied request was refused before
-- a rule was read.
--
-- The file is created by the appliance's init step (`secrets/node/token`) and
-- mounted read-only into the runner; the control API resolves it inside its own
-- secret directory. The locator, not the value, is stored (ADR 0005).

UPDATE nodes
   SET node_token_ref = 'file:///secrets/node/token',
       updated_at = now()
 WHERE is_local
   AND node_token_ref IS NULL;
