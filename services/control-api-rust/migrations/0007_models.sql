-- 0007_models — the catalog and hardware detection.
--
-- Requirements are data, not constants (ADR 0013). The install gate and the
-- console badge read the same row, so they cannot disagree.

CREATE TABLE models (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id   uuid REFERENCES environments(id) ON DELETE CASCADE,
    -- Stable identifier used in URLs and in the pack catalog.
    key              text NOT NULL,
    name             text NOT NULL,
    family           text NOT NULL DEFAULT 'nemotron',
    parameters       text NOT NULL,
    context_length   bigint,
    quantization     text,
    -- Estimated weight footprint in bytes at the given quantization.
    weights_bytes    bigint,
    -- The gate: what the machine must have.
    min_vram_bytes   bigint,
    min_gpu_count    integer NOT NULL DEFAULT 1,
    -- Where the artifacts come from, and the version this row describes.
    source_uri       text,
    version          text NOT NULL,
    installed_version text,
    state            text NOT NULL DEFAULT 'not_installed',
    installed_at     timestamptz,
    checksum         text,
    -- true for a catalog entry that is served by a cloud provider instead of
    -- installed on this machine.
    is_remote        boolean NOT NULL DEFAULT false,
    provider_key     text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT models_key_unique UNIQUE (key),
    CONSTRAINT models_state_valid CHECK (state IN (
        'not_installed', 'installing', 'installed', 'updating',
        'uninstalling', 'failed', 'remote'
    )),
    CONSTRAINT models_vram_sane CHECK (min_vram_bytes IS NULL OR min_vram_bytes > 0),
    CONSTRAINT models_gpu_count_sane CHECK (min_gpu_count > 0)
);

CREATE INDEX models_env_idx ON models (environment_id);

COMMENT ON COLUMN models.min_vram_bytes IS
  'Estimated requirement in bytes. The fit classifier compares this to detected VRAM.';

-- A snapshot of what the machine had, taken on demand and cached. VRAM changes
-- when another workload occupies the GPU, so it is refreshable rather than
-- computed once at boot.
CREATE TABLE hardware_profiles (
    id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id        uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    detected_at           timestamptz NOT NULL DEFAULT now(),
    hostname              text,
    gpus                  jsonb NOT NULL DEFAULT '[]'::jsonb,
    total_vram_bytes      bigint NOT NULL DEFAULT 0,
    -- GB10-class machines report unified memory instead of per-card VRAM.
    unified_memory_bytes  bigint,
    cuda_version          text,
    driver_version        text,
    -- Why detection produced what it did: no driver, no devices, permission.
    detection_note        text
);

CREATE INDEX hardware_profiles_env_idx
    ON hardware_profiles (environment_id, detected_at DESC);

-- A model request observed at the gateway. Stored in telemetry_events as a
-- `kind = inference` row; this view-shaped table exists only for the columns
-- the console sorts on, so the query does not parse JSON on a hot path.
CREATE TABLE inference_requests (
    id             uuid NOT NULL DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id     uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    provider_key   text,
    endpoint       text,
    model_key      text,
    method         text,
    path           text,
    status_code    integer,
    latency_ms     integer,
    tokens_in      integer,
    tokens_out     integer,
    bytes_in       bigint,
    bytes_out      bigint,
    decision       text NOT NULL DEFAULT 'allow',
    correlation_id text,
    ts             timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id, ts)
) PARTITION BY RANGE (ts);

CREATE INDEX inference_requests_env_ts_idx ON inference_requests (environment_id, ts DESC);
CREATE INDEX inference_requests_sandbox_ts_idx ON inference_requests (sandbox_id, ts DESC);

CREATE TABLE inference_requests_default PARTITION OF inference_requests DEFAULT;

DO $$
DECLARE
    start_month date := date_trunc('month', now())::date;
    i integer;
    part_start date;
    part_end date;
    part_name text;
BEGIN
    FOR i IN 0..2 LOOP
        part_start := (start_month + (i || ' month')::interval)::date;
        part_end := (start_month + ((i + 1) || ' month')::interval)::date;
        part_name := 'inference_requests_' || to_char(part_start, 'YYYY_MM');
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF inference_requests FOR VALUES FROM (%L) TO (%L)',
            part_name, part_start, part_end
        );
    END LOOP;
END $$;
