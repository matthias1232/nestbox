-- 0010_default_gateway — register the gateway this appliance ships with.
--
-- The compose stack runs exactly one gateway. Leaving it unregistered would mean
-- every sandbox is unplaced, the cluster graph has no edges, and the console
-- cannot answer "which gateway carries this workload" — which is the whole point
-- of the view.
--
-- This is data, not a hardcoded assumption: the row is a normal gateway that can
-- be edited, synced, or replaced. A multi-gateway install simply adds more.

INSERT INTO gateways (
    id, cluster_id, node_id, name, endpoint, status, version,
    settings, settings_version, settings_applied_version, capacity, used
)
SELECT
    '00000000-0000-0000-0000-000000000010',
    c.id,
    n.id,
    'gateway-01',
    'http://gateway:8081',
    'healthy',
    '1.0.0',
    jsonb_build_object(
        'runtime', 'docker',
        'service_forwarding', true,
        'note', 'Single-gateway appliance. Additional gateways are registered rows, not code.'
    ),
    1,
    1,
    100,
    0
FROM clusters c
JOIN nodes n ON n.name = 'local'
WHERE c.slug = 'local'
ON CONFLICT (cluster_id, name) DO NOTHING;

-- Adopt sandboxes that existed before the gateway was registered.
--
-- Without this they would stay unplaced: running, visible in the list, and
-- disconnected in the graph. Adoption only applies when there is exactly one
-- gateway, because with several there is no defensible way to guess which one
-- should take an orphan.
UPDATE sandboxes s
   SET gateway_id = g.id,
       updated_at = now()
  FROM gateways g
 WHERE s.gateway_id IS NULL
   AND (SELECT count(*) FROM gateways) = 1
   AND g.name = 'gateway-01';

-- The public face of the gateway: one published port, hostname routing behind
-- it. Recorded now so the ingress work has a target to attach to.
INSERT INTO gateway_endpoints (gateway_id, public_address, overlay_address, status)
SELECT
    g.id,
    NULL,
    '10.244.0.1',
    'healthy'
FROM gateways g
WHERE g.name = 'gateway-01'
ON CONFLICT (gateway_id) DO NOTHING;
