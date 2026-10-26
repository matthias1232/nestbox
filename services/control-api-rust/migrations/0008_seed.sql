-- 0008_seed — the environment the appliance starts with.
--
-- Runs before row-level security is enabled, as the schema owner, so it can
-- insert without a tenant scope. After 0009 every tenant query must carry one.

-- The default organization and environment. The name `default` is fixed: the
-- first environment is always called default, and the console opens on it.
INSERT INTO organizations (id, name, slug)
VALUES ('00000000-0000-0000-0000-000000000001', 'Default Organization', 'default')
ON CONFLICT (slug) DO NOTHING;

INSERT INTO environments (id, organization_id, name, slug, timezone, retention_days)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'default', 'default', 'UTC', 30
)
ON CONFLICT (organization_id, slug) DO NOTHING;

-- A local cluster and node. On a single-machine appliance these are real rows,
-- not placeholders: the cluster is where gateways are grouped, and the node is
-- the machine the gateway runs on.
INSERT INTO nodes (id, name, zone, region, endpoint)
VALUES ('00000000-0000-0000-0000-000000000003', 'local', 'default', 'local', NULL)
ON CONFLICT (name) DO NOTHING;

INSERT INTO clusters (id, environment_id, name, slug, placement_policy)
VALUES (
    '00000000-0000-0000-0000-000000000004',
    '00000000-0000-0000-0000-000000000002',
    'local', 'local', 'least_loaded'
)
ON CONFLICT (environment_id, slug) DO NOTHING;

-- ---------------------------------------------------------------------------
-- The model catalog.
--
-- The numbers come from the project's own published sizing table and are
-- estimates, labelled as such. They are stored as data so the install gate and
-- the console badge read the same row (ADR 0013).
--
-- min_vram_bytes is in bytes: GiB * 1024^3.
-- ---------------------------------------------------------------------------

INSERT INTO models (
    key, name, family, parameters, context_length, quantization,
    weights_bytes, min_vram_bytes, min_gpu_count, source_uri, version, is_remote
) VALUES
    (
        'nemotron-nano-12b-v2-vl',
        'Nemotron Nano 12B v2 VL',
        'nemotron',
        '12.6B dense',
        131072,
        'NVFP4',
        15032385536,          -- ~14 GiB
        25769803776,          -- 24 GiB
        1,
        'https://build.nvidia.com/nvidia/nemotron-nano-12b-v2-vl',
        '2.0.0',
        false
    ),
    (
        'nemotron-3-nano',
        'Nemotron 3 Nano',
        'nemotron',
        '30B-A3B (3.2B active)',
        1048576,
        'NVFP4',
        19327352832,          -- ~18 GiB
        34359738368,          -- 32 GiB
        1,
        'https://research.nvidia.com/labs/nemotron/Nemotron-3/',
        '3.0.0',
        false
    ),
    (
        'nemotron-3-super',
        'Nemotron 3 Super',
        'nemotron',
        '120B-A12B',
        1048576,
        'NVFP4',
        69793218560,          -- ~65 GiB
        85899345920,          -- 80 GiB
        1,
        'https://developer.nvidia.com/blog/introducing-nemotron-3-super/',
        '3.0.0',
        false
    ),
    (
        'nemotron-3-ultra',
        'Nemotron 3 Ultra',
        'nemotron',
        '550B-A55B',
        1048576,
        'NVFP4',
        300647710720,         -- ~280 GiB
        412316860416,         -- 384 GiB total (4 x 96 GiB)
        4,
        'https://research.nvidia.com/labs/nemotron/Nemotron-3-Ultra/',
        '3.0.0',
        false
    )
ON CONFLICT (key) DO NOTHING;

-- ---------------------------------------------------------------------------
-- The operational schedules.
--
-- These are not optional extras. Without the partition maintenance job, inserts
-- fail when a month has no partition. Without config sync, drift never
-- converges. They are seeded so the system is complete on first boot.
-- ---------------------------------------------------------------------------

INSERT INTO schedules (environment_id, name, kind, cron, payload) VALUES
    ('00000000-0000-0000-0000-000000000002', 'gateway-config-sync',
     'gateway_config_sync', '*/20 * * * *', '{}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'policy-reconciliation',
     'policy_reconciliation', '*/5 * * * *', '{}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'certificate-expiry-check',
     'certificate_expiry_check', '0 6 * * *', '{"warn_days": 30}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'address-pool-reconcile',
     'address_pool_reconcile', '17 * * * *', '{}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'telemetry-partition-maintenance',
     'telemetry_partition_maintenance', '30 2 * * *', '{"months_ahead": 3}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'provider-refresh',
     'provider_refresh', '*/15 * * * *', '{}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'model-update-check',
     'model_update_check', '0 4 * * 1', '{}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'session-cleanup',
     'session_cleanup', '7 * * * *', '{}'::jsonb)
ON CONFLICT (environment_id, name) DO NOTHING;

-- The bootstrap administrator. The password hash is set by the binary on first
-- start, not here, so no credential is baked into a migration file.
INSERT INTO users (
    id, organization_id, email, display_name, password_hash, role, status
) VALUES (
    '00000000-0000-0000-0000-000000000005',
    '00000000-0000-0000-0000-000000000001',
    'admin@nestbox.local',
    'Administrator',
    -- Sentinel value. The API refuses to authenticate this hash and bootstraps
    -- a generated password on first start, printing it once.
    '!bootstrap',
    'admin',
    'active'
)
ON CONFLICT (organization_id, email) DO NOTHING;

INSERT INTO user_scopes (user_id, environment_id, role)
VALUES (
    '00000000-0000-0000-0000-000000000005',
    '00000000-0000-0000-0000-000000000002',
    'admin'
)
ON CONFLICT (user_id, environment_id) DO NOTHING;

-- A default address pool for sandboxes. The overlay range is private and is
-- what makes "every sandbox has its own address" possible without a port map.
INSERT INTO address_pools (environment_id, cidr, purpose, allocation)
VALUES ('00000000-0000-0000-0000-000000000002', '10.244.0.0/16', 'sandbox', 'dynamic')
ON CONFLICT DO NOTHING;
