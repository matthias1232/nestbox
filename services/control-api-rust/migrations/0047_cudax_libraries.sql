-- 0047_cudax_libraries
--
-- CUDA-X components and their per-node installation.
--
-- The catalog is global, like `nodes`: a component is a fact about the
-- software, not about one tenant. An installation records what an operator
-- asked for on one node, the compatibility assessment that was shown at that
-- moment, and whether it actually installed. A missing GPU never prevents the
-- row: `installed_unsupported` is a real, visible state.

CREATE TABLE cudax_components (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    key               text NOT NULL,
    name              text NOT NULL,
    category          text NOT NULL DEFAULT 'runtime',
    version           text NOT NULL DEFAULT 'latest',
    description       text NOT NULL DEFAULT '',
    -- Declarative inputs for the node runner. Never a shell string.
    apt_packages      text[] NOT NULL DEFAULT '{}',
    container_images  text[] NOT NULL DEFAULT '{}',
    requires_gpu      boolean NOT NULL DEFAULT false,
    min_driver        text NOT NULL DEFAULT '',
    min_cuda          text NOT NULL DEFAULT '',
    docs_url          text NOT NULL DEFAULT '',
    is_active         boolean NOT NULL DEFAULT true,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT cudax_components_key_unique UNIQUE (key),
    CONSTRAINT cudax_components_category_check
        CHECK (category IN ('runtime', 'math', 'communication', 'inference', 'observability', 'framework'))
);

CREATE TABLE node_cudax_installs (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id       uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    component_id  uuid NOT NULL REFERENCES cudax_components(id) ON DELETE CASCADE,
    version       text NOT NULL DEFAULT '',
    state         text NOT NULL DEFAULT 'planned',
    force_install boolean NOT NULL DEFAULT false,
    -- The warnings the operator saw when assigning the component.
    assessment    jsonb NOT NULL DEFAULT '{}'::jsonb,
    last_error    text NOT NULL DEFAULT '',
    installed_at  timestamptz,
    created_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT node_cudax_installs_unique UNIQUE (node_id, component_id, version),
    CONSTRAINT node_cudax_installs_state_check
        CHECK (state IN ('planned', 'installing', 'installed', 'installed_unsupported', 'failed', 'removed'))
);

CREATE INDEX node_cudax_installs_node_idx ON node_cudax_installs (node_id, state);
CREATE INDEX node_cudax_installs_component_idx ON node_cudax_installs (component_id, state);

INSERT INTO cudax_components
    (key, name, category, version, description, apt_packages, container_images, requires_gpu, min_driver, min_cuda, docs_url)
VALUES
    ('cuda-toolkit', 'CUDA Toolkit', 'runtime', '12.9',
     'Compiler, runtime libraries and development headers for CUDA kernels.',
     ARRAY['cuda-toolkit-12-9'], ARRAY['nvidia/cuda:12.9.0-devel-ubuntu24.04'], false, '525.60', '12.0',
     'https://docs.nvidia.com/cuda/'),
    ('cudnn', 'cuDNN', 'math', '9',
     'Deep-learning primitives for convolutions, attention and normalisation.',
     ARRAY['libcudnn9-cuda-12'], ARRAY[]::text[], true, '525.60', '12.0',
     'https://docs.nvidia.com/deeplearning/cudnn/'),
    ('cublas', 'cuBLAS / cuBLASLt', 'math', '12',
     'GPU-accelerated dense matrix multiplication used by every inference runtime.',
     ARRAY['libcublas-12-9'], ARRAY[]::text[], true, '525.60', '12.0',
     'https://docs.nvidia.com/cuda/cublas/'),
    ('nccl', 'NCCL', 'communication', '2',
     'Collective communication for tensor, pipeline and expert parallelism.',
     ARRAY['libnccl2', 'libnccl-dev'], ARRAY[]::text[], true, '525.60', '12.0',
     'https://docs.nvidia.com/deeplearning/nccl/'),
    ('tensorrt', 'TensorRT', 'inference', '10',
     'Graph optimiser and runtime for low-latency NVIDIA inference.',
     ARRAY['tensorrt'], ARRAY['nvcr.io/nvidia/tensorrt:latest'], true, '535.54', '12.0',
     'https://docs.nvidia.com/tensorrt/'),
    ('tensorrt-llm', 'TensorRT-LLM', 'inference', 'latest',
     'Optimised LLM runtime with in-flight batching and quantisation.',
     ARRAY[]::text[], ARRAY['nvcr.io/nvidia/tensorrt-llm/release:latest'], true, '535.54', '12.4',
     'https://docs.nvidia.com/tensorrt-llm/'),
    ('nim', 'NVIDIA NIM', 'inference', 'latest',
     'Containerised, OpenAI-compatible NVIDIA inference microservices.',
     ARRAY[]::text[], ARRAY['nvcr.io/nim/meta/llama-3.1-8b-instruct:latest'], true, '535.54', '12.4',
     'https://docs.nvidia.com/nim/'),
    ('triton-inference-server', 'Triton Inference Server', 'inference', 'latest',
     'Multi-framework model server with Prometheus metrics.',
     ARRAY[]::text[], ARRAY['nvcr.io/nvidia/tritonserver:latest'], true, '535.54', '12.0',
     'https://docs.nvidia.com/deeplearning/triton-inference-server/'),
    ('vllm', 'vLLM', 'inference', 'latest',
     'Paged-attention OpenAI-compatible serving engine.',
     ARRAY[]::text[], ARRAY['vllm/vllm-openai:latest'], true, '525.60', '12.0',
     'https://docs.vllm.ai/'),
    ('sglang', 'SGLang', 'inference', 'latest',
     'High-throughput serving runtime with RadixAttention prefix caching.',
     ARRAY[]::text[], ARRAY['lmsysorg/sglang:latest'], true, '525.60', '12.0',
     'https://docs.sglang.ai/'),
    ('dcgm', 'DCGM / NVML', 'observability', '4',
     'GPU telemetry for utilisation, memory, power, temperature and NVLink.',
     ARRAY['datacenter-gpu-manager'], ARRAY['nvcr.io/nvidia/dcgm:latest'], true, '525.60', '12.0',
     'https://docs.nvidia.com/datacenter/dcgm/'),
    ('nemo', 'NVIDIA NeMo', 'framework', 'latest',
     'Training and customisation framework for Nemotron-family models.',
     ARRAY[]::text[], ARRAY['nvcr.io/nvidia/nemo:latest'], true, '535.54', '12.4',
     'https://docs.nvidia.com/nemo/'),
    ('rapids', 'RAPIDS', 'framework', 'latest',
     'GPU data-science stack (cuDF, cuML, cuGraph).',
     ARRAY[]::text[], ARRAY['nvcr.io/nvidia/rapidsai/base:latest'], true, '525.60', '12.0',
     'https://docs.rapids.ai/');
