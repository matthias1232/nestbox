-- 0012_model_requirements — what a model needs beyond VRAM, and what a machine
-- has beyond GPUs.
--
-- The first catalog only knew VRAM and GPU count, which is the right gate for a
-- TensorRT model on a discrete card and the wrong one for everything else:
--
--   * a GGUF model on the CPU is gated by system RAM and cores, not VRAM;
--   * a model on a unified-memory machine (DGX Spark / GB10) draws from one
--     pool that is neither "VRAM" nor "RAM" in the discrete sense;
--   * "minimum" and "recommended" are different numbers, and an operator who
--     sees only one of them cannot tell "will start" from "will be usable".
--
-- Every new column is nullable so existing rows stay valid. The fit classifier
-- treats a NULL requirement as "no gate on this resource", never as zero, so a
-- row that omits a number cannot accidentally pass a check it should fail.

ALTER TABLE models
    ADD COLUMN IF NOT EXISTS min_ram_bytes          bigint,
    ADD COLUMN IF NOT EXISTS recommended_ram_bytes  bigint,
    ADD COLUMN IF NOT EXISTS recommended_vram_bytes bigint,
    ADD COLUMN IF NOT EXISTS min_cpu_cores          integer,
    ADD COLUMN IF NOT EXISTS recommended_cpu_cores  integer,
    -- cuda: needs a GPU. cpu: runs without one. hybrid: GPU preferred, CPU
    -- fallback with the RAM gate applied.
    ADD COLUMN IF NOT EXISTS runtime                text NOT NULL DEFAULT 'cuda',
    -- tensorrt, gguf, safetensors: the artefact family, which decides the
    -- server that will load it.
    ADD COLUMN IF NOT EXISTS arch                   text,
    ADD COLUMN IF NOT EXISTS artifact_format        text,
    ADD COLUMN IF NOT EXISTS publisher              text,
    ADD COLUMN IF NOT EXISTS license                text,
    -- True when the model only makes sense on a unified-memory machine: its
    -- weights exceed any single discrete card but fit the shared pool.
    ADD COLUMN IF NOT EXISTS unified_memory_only    boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS is_cpu_only            boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS description            text;

ALTER TABLE models
    ADD CONSTRAINT models_runtime_valid
        CHECK (runtime IN ('cuda', 'cpu', 'hybrid')),
    ADD CONSTRAINT models_ram_sane
        CHECK (min_ram_bytes IS NULL OR min_ram_bytes > 0),
    ADD CONSTRAINT models_recommended_ram_sane
        CHECK (recommended_ram_bytes IS NULL OR min_ram_bytes IS NULL
               OR recommended_ram_bytes >= min_ram_bytes),
    ADD CONSTRAINT models_recommended_vram_sane
        CHECK (recommended_vram_bytes IS NULL OR min_vram_bytes IS NULL
               OR recommended_vram_bytes >= min_vram_bytes),
    ADD CONSTRAINT models_cores_sane
        CHECK (min_cpu_cores IS NULL OR min_cpu_cores > 0),
    -- A CPU-only model must say how much RAM it needs; that is its whole gate.
    ADD CONSTRAINT models_cpu_only_has_ram
        CHECK (NOT is_cpu_only OR min_ram_bytes IS NOT NULL);

-- Existing Nemotron rows keep their VRAM gate and gain nothing else: they are
-- cuda/tensorrt by construction, which the defaults already say.
UPDATE models SET arch = 'tensorrt', artifact_format = 'nvfp4', publisher = 'NVIDIA'
 WHERE family = 'nemotron' AND arch IS NULL;

CREATE INDEX IF NOT EXISTS models_family_idx    ON models (family);
CREATE INDEX IF NOT EXISTS models_runtime_idx   ON models (runtime);
CREATE INDEX IF NOT EXISTS models_publisher_idx ON models (publisher);

COMMENT ON COLUMN models.runtime IS
  'cuda needs a GPU; cpu runs without one; hybrid prefers a GPU and falls back to RAM.';
COMMENT ON COLUMN models.unified_memory_only IS
  'True when the weights exceed any single discrete card but fit a unified pool (DGX Spark / GB10).';

-- ---------------------------------------------------------------------------
-- The machine side.
-- ---------------------------------------------------------------------------

ALTER TABLE hardware_profiles
    ADD COLUMN IF NOT EXISTS cpu_model         text,
    ADD COLUMN IF NOT EXISTS cpu_cores         integer,
    ADD COLUMN IF NOT EXISTS cpu_threads       integer,
    ADD COLUMN IF NOT EXISTS system_ram_bytes  bigint,
    -- How the unified pool was learned: a GB10 driver report, or an inference
    -- from "no discrete card but a large RAM pool". The console shows this so
    -- a number that was inferred is never mistaken for one that was measured.
    ADD COLUMN IF NOT EXISTS unified_memory_source text;

COMMENT ON COLUMN hardware_profiles.system_ram_bytes IS
  'MemTotal from /proc/meminfo. On a unified machine this is also the pool a model draws from.';
