-- 0037_node_public_hostname — the name a client dials, next to the address.
--
-- `nodes.public_address` (0021) is where published ports are bound. A client
-- usually wants a name for it (`gpu-box.example.net`, a DNS entry or an
-- /etc/hosts line), and SSH config files read better with one. The hostname is a
-- label for humans and clients; it is never bound to or resolved by the control
-- plane, so a wrong one misleads but cannot misroute.

ALTER TABLE nodes ADD COLUMN IF NOT EXISTS public_hostname text;

COMMENT ON COLUMN nodes.public_hostname IS
  'DNS name clients use to reach the published ports of this node. Display and copy-paste only.';
