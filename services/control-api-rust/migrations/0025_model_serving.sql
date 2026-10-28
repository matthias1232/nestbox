-- 0025_model_serving — an install can now become a running model server.
--
-- Phase 1 reserved memory and recorded intent. This migration gives the intent
-- somewhere to go: the artefact a model is served from, the image that serves
-- it, and the two states between "asked for" and "answering".
--
-- Nothing here downloads anything by itself. The control plane calls the node's
-- runner, which owns the disk and the Docker daemon; the row only says what to
-- fetch and what to start.

-- ---------------------------------------------------------------------------
-- 1. Two more states on the way to `installed`.
-- ---------------------------------------------------------------------------

ALTER TABLE model_installs DROP CONSTRAINT model_installs_state_valid;
ALTER TABLE model_installs ADD CONSTRAINT model_installs_state_valid CHECK (state IN (
    'installing', 'downloading', 'starting', 'installed', 'updating', 'uninstalling', 'failed'
));

COMMENT ON COLUMN model_installs.state IS
  'installing: reserved, nothing fetched. downloading: the runner is fetching the artefact. starting: the server container is up but has not answered a probe. installed: it answers.';

-- ---------------------------------------------------------------------------
-- 2. Where the weights come from, and which runtime serves them.
-- ---------------------------------------------------------------------------

-- `huggingface:<repo>/<file>` — one file, not a snapshot: a GGUF file is a
-- complete model, and a snapshot of a safetensors repository is several files
-- whose names the platform would have to know. A model without this cannot be
-- served, and the install says so instead of pretending.
ALTER TABLE models ADD COLUMN artifact_ref text;

-- The runtime name in `model_server_images`; null means "no server for this
-- model in this phase".
ALTER TABLE models ADD COLUMN serve_runtime text;

ALTER TABLE models
    ADD CONSTRAINT models_artifact_ref_shape
    CHECK (artifact_ref IS NULL OR artifact_ref ~ '^huggingface:[A-Za-z0-9._-]+/[A-Za-z0-9._-]+/.+$');

-- The first model that can actually run on the appliance: a 1.7B GGUF at Q8,
-- about 1.8 GiB of weights, served by llama.cpp on four CPU cores.
UPDATE models
   SET artifact_ref = 'huggingface:Qwen/Qwen3-1.7B-GGUF/Qwen3-1.7B-Q8_0.gguf',
       serve_runtime = 'llama_cpp'
 WHERE key = 'qwen3-1.7b';

-- ---------------------------------------------------------------------------
-- 3. The images allowed to serve a model, pinned by digest.
-- ---------------------------------------------------------------------------
--
-- An image is a dependency with a version, and `latest` is not a version. The
-- runner validates the image it is asked to start against its own allowlist as
-- well, so a compromised control plane cannot run an arbitrary image on a node.

CREATE TABLE model_server_images (
    runtime     text PRIMARY KEY,
    image       text NOT NULL,
    -- `repo@sha256:...`; the only form the runner is allowed to start.
    digest      text NOT NULL,
    port        integer NOT NULL,
    -- `{model}` is replaced by the path inside the container, `{threads}` by
    -- the node's CPU count. The rest is literal.
    command     text[] NOT NULL DEFAULT '{}',
    notes       text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_server_images_digest_shape
        CHECK (digest ~ '^[a-z0-9./-]+@sha256:[a-f0-9]{64}$')
);

INSERT INTO model_server_images (runtime, image, digest, port, command, notes) VALUES
    ('llama_cpp',
     'ghcr.io/ggml-org/llama.cpp',
     'ghcr.io/ggml-org/llama.cpp@sha256:8ee4ec7ce58a921313d07390c6699af13aa5ec083f3cdf67e5f4155767047373',
     8080,
     ARRAY['-m', '{model}', '--host', '0.0.0.0', '--port', '8080',
           '--threads', '{threads}', '--ctx-size', '4096', '--jinja'],
     'llama.cpp server, CPU. One GGUF file, one port, an OpenAI-compatible /v1.');
