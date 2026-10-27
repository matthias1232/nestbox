-- 0019_catalog_seed_highend — the frontier rows, with numbers taken from the
-- published artefacts rather than from memory.
--
-- Every figure below comes from the Hugging Face model API on 2026-10-27:
-- `safetensors.total` for the parameter count, the sum of the weight files for
-- `weights_gib`, `config.json` for context and expert counts. The derivation of
-- the gates is the rule stated in 0013: min_vram = weights + KV/runtime
-- overhead (about 10 % above 100 GiB, +6..16 GiB below), recommended = min x
-- 1.25, rounded up. A model above one card's memory is `unified_memory_only`
-- with `min_gpu_count = ceil(min_vram / 80)` as the discrete alternative.
--
-- Nobody runs Kimi K3 at home. It is in the list because an operator should see
-- the whole open-weight landscape and the honest distance to it, not a catalog
-- trimmed to what fits.

CREATE TEMP TABLE model_seed (
    key text PRIMARY KEY, name text NOT NULL, family text NOT NULL, parameters text NOT NULL,
    context_length bigint, quantization text, weights_gib numeric, min_vram_gib numeric,
    rec_vram_gib numeric, min_gpu_count integer NOT NULL DEFAULT 1, min_ram_gib numeric,
    rec_ram_gib numeric, min_cores integer, rec_cores integer, runtime text NOT NULL,
    arch text, artifact_format text, publisher text, license text, description text,
    unified_only boolean NOT NULL DEFAULT false, cpu_only boolean NOT NULL DEFAULT false,
    source_uri text, version text NOT NULL
);

INSERT INTO model_seed VALUES
-- DeepSeek V4 generation (MIT). All FP8/INT8 as published.
('deepseek-v4.1-flash', 'DeepSeek V4.1 Flash', 'deepseek', '763B MoE (6 active experts)', 1048576, 'FP8/INT8',
 475, 520, 640, 7, 560, 768, 32, 64, 'cuda', 'safetensors', 'fp8', 'DeepSeek', 'MIT',
 'Current DeepSeek flagship with vision input and a 1M context; seven 80 GB cards or a 640 GiB pool.',
 true, false, 'https://huggingface.co/deepseek-ai/DeepSeek-V4.1-Flash', '4.1.0'),
('deepseek-v4-pro', 'DeepSeek V4 Pro', 'deepseek', '1.6T MoE (384 experts, 6 active)', 1048576, 'FP8/INT8',
 805, 880, 1100, 11, 900, 1152, 32, 64, 'cuda', 'safetensors', 'fp8', 'DeepSeek', 'MIT',
 'The largest DeepSeek: 1.6 trillion parameters, 1M context.',
 true, false, 'https://huggingface.co/deepseek-ai/DeepSeek-V4-Pro', '4.0.0'),
('deepseek-v4-flash', 'DeepSeek V4 Flash', 'deepseek', '291B MoE (256 experts, 6 active)', 1048576, 'FP8/INT8',
 149, 170, 210, 3, 192, 256, 16, 32, 'cuda', 'safetensors', 'fp8', 'DeepSeek', 'MIT',
 'V4 Flash fits three 80 GB cards or a 256 GiB unified pool.',
 true, false, 'https://huggingface.co/deepseek-ai/DeepSeek-V4-Flash', '4.0.0'),
('deepseek-v3.2', 'DeepSeek V3.2', 'deepseek', '685B MoE (256 experts, 8 active)', 163840, 'FP8',
 642, 700, 880, 9, 704, 896, 32, 64, 'cuda', 'safetensors', 'fp8', 'DeepSeek', 'MIT',
 'V3.2 with sparse attention; the last V3-line release.',
 true, false, 'https://huggingface.co/deepseek-ai/DeepSeek-V3.2', '3.2.0'),

-- Moonshot Kimi (modified MIT).
('kimi-k3', 'Kimi K3', 'kimi', '2.8T MoE (896 experts)', 1048576, 'INT4/INT8',
 1454, 1600, 2000, 20, 1600, 2048, 64, 128, 'cuda', 'safetensors', 'int4', 'Moonshot AI', 'Modified MIT',
 'The largest open-weight model listed: 2.8 trillion parameters, 1M context.',
 true, false, 'https://huggingface.co/moonshotai/Kimi-K3', '3.0.0'),
('kimi-k2.6', 'Kimi K2.6', 'kimi', '1.0T MoE (8 active)', 262144, 'INT4',
 554, 610, 760, 8, 640, 768, 32, 64, 'cuda', 'safetensors', 'int4', 'Moonshot AI', 'Modified MIT',
 'K2.6 agentic model with vision; eight 80 GB cards.',
 true, false, 'https://huggingface.co/moonshotai/Kimi-K2.6', '2.6.0'),
('kimi-k2.7-code', 'Kimi K2.7 Code', 'kimi', '1.0T MoE (8 active)', 262144, 'INT4',
 554, 610, 760, 8, 640, 768, 32, 64, 'cuda', 'safetensors', 'int4', 'Moonshot AI', 'Modified MIT',
 'Coding-tuned K2.7 at the same footprint as K2.6.',
 true, false, 'https://huggingface.co/moonshotai/Kimi-K2.7-Code', '2.7.0'),
('kimi-k2-thinking', 'Kimi K2 Thinking', 'kimi', '1.0T MoE (384 experts, 8 active)', 262144, 'INT4',
 553, 610, 760, 8, 640, 768, 32, 64, 'cuda', 'safetensors', 'int4', 'Moonshot AI', 'Modified MIT',
 'The reasoning K2; INT4 as published.',
 true, false, 'https://huggingface.co/moonshotai/Kimi-K2-Thinking', '2.0.0'),

-- Z.ai GLM.
('glm-5.3', 'GLM-5.3', 'glm', '753B MoE (256 experts, 8 active)', 1048576, 'FP8',
 704, 770, 960, 10, 768, 1024, 32, 64, 'cuda', 'safetensors', 'fp8', 'Z.ai', 'Z.ai License',
 'Current GLM flagship, 1M context.',
 true, false, 'https://huggingface.co/zai-org/GLM-5.3', '5.3.0'),
('glm-5.3-flash', 'GLM-5.3 Flash', 'glm', '321B MoE (8 active)', 1048576, 'FP8',
 306, 340, 420, 5, 352, 448, 16, 32, 'cuda', 'safetensors', 'fp8', 'Z.ai', 'MIT',
 'Flash variant: five 80 GB cards or a 448 GiB pool.',
 true, false, 'https://huggingface.co/zai-org/GLM-5.3-Flash', '5.3.0'),
('glm-5', 'GLM-5', 'glm', '754B MoE (256 experts, 8 active)', 202752, 'BF16',
 1404, 1500, 1900, 19, 1536, 1920, 64, 128, 'cuda', 'safetensors', 'safetensors', 'Z.ai', 'MIT',
 'GLM-5 as published in BF16, which doubles the footprint of the FP8 successors.',
 true, false, 'https://huggingface.co/zai-org/GLM-5', '5.0.0'),
('glm-4.5-air', 'GLM-4.5 Air', 'glm', '110B MoE (128 experts, 8 active)', 131072, 'BF16',
 206, 230, 290, 3, 256, 320, 16, 32, 'cuda', 'safetensors', 'safetensors', 'Z.ai', 'MIT',
 'The small GLM MoE; three 80 GB cards in BF16, one at 4-bit.',
 true, false, 'https://huggingface.co/zai-org/GLM-4.5-Air', '4.5.0'),

-- MiniMax.
('minimax-m3', 'MiniMax M3', 'minimax', '427B sparse MoE (4 active)', 1048576, 'BF16',
 796, 870, 1090, 11, 896, 1152, 32, 64, 'cuda', 'safetensors', 'safetensors', 'MiniMax', 'MiniMax License',
 'M3 at BF16; a MXFP8 variant halves this.',
 true, false, 'https://huggingface.co/MiniMaxAI/MiniMax-M3', '3.0.0'),
('minimax-m3.1-flash', 'MiniMax M3.1 Flash', 'minimax', '221B sparse MoE (4 active)', 1048576, 'INT8',
 235, 260, 330, 4, 288, 384, 16, 32, 'cuda', 'safetensors', 'int8', 'MiniMax', 'MiniMax License',
 'Flash variant with a 1M context.',
 true, false, 'https://huggingface.co/MiniMaxAI/MiniMax-M3.1-Flash', '3.1.0'),
('minimax-m2.7', 'MiniMax M2.7', 'minimax', '229B MoE (256 experts, 8 active)', 204800, 'FP8',
 214, 240, 300, 3, 256, 320, 16, 32, 'cuda', 'safetensors', 'fp8', 'MiniMax', 'MiniMax License',
 'The last M2; FP8 as published.',
 true, false, 'https://huggingface.co/MiniMaxAI/MiniMax-M2.7', '2.7.0'),

-- OpenAI gpt-oss (Apache-2.0, MXFP4).
('gpt-oss-120b', 'gpt-oss 120B', 'gpt-oss', '117B MoE (128 experts, 4 active)', 131072, 'MXFP4',
 182, 80, 96, 1, 96, 128, 8, 16, 'cuda', 'safetensors', 'mxfp4', 'OpenAI', 'Apache-2.0',
 'Runs on a single 80 GB card at MXFP4, which is how OpenAI publishes it; the safetensors on disk are larger.',
 false, false, 'https://huggingface.co/openai/gpt-oss-120b', '1.0.0'),
('gpt-oss-20b', 'gpt-oss 20B', 'gpt-oss', '21B MoE (32 experts, 4 active)', 131072, 'MXFP4',
 38, 16, 24, 1, 24, 32, 4, 8, 'hybrid', 'safetensors', 'mxfp4', 'OpenAI', 'Apache-2.0',
 'The 16 GB-card gpt-oss; also runs on the CPU with 24 GiB of RAM.',
 false, false, 'https://huggingface.co/openai/gpt-oss-20b', '1.0.0'),

-- Qwen 3.5 / 3.8 (Apache-2.0, BF16 as published).
('qwen3.8-27b', 'Qwen3.8 27B', 'qwen', '27.8B dense', 262144, 'BF16',
 52, 60, 80, 1, 64, 96, 8, 16, 'cuda', 'safetensors', 'safetensors', 'Alibaba', 'Apache-2.0',
 'Current dense Qwen at BF16; a 4-bit build fits a 24 GB card.',
 false, false, 'https://huggingface.co/Qwen/Qwen3.8-27B', '3.8.0'),
('qwen3.5-9b', 'Qwen3.5 9B', 'qwen', '9.7B dense', 262144, 'BF16',
 18, 24, 32, 1, 24, 32, 4, 8, 'hybrid', 'safetensors', 'safetensors', 'Alibaba', 'Apache-2.0',
 'Qwen3.5 mid-size with a 256k context.',
 false, false, 'https://huggingface.co/Qwen/Qwen3.5-9B', '3.5.0'),
('qwen3.5-4b', 'Qwen3.5 4B', 'qwen', '4.7B dense', 262144, 'BF16',
 9, 12, 16, 1, 12, 16, 2, 4, 'hybrid', 'safetensors', 'safetensors', 'Alibaba', 'Apache-2.0',
 'Small Qwen3.5; CPU-capable at 4-bit.',
 false, false, 'https://huggingface.co/Qwen/Qwen3.5-4B', '3.5.0'),

-- Gemma 4 (Apache-2.0).
('gemma-4-31b-it', 'Gemma 4 31B IT', 'gemma', '31B dense', 262144, 'BF16',
 58, 64, 80, 1, 72, 96, 8, 16, 'cuda', 'safetensors', 'safetensors', 'Google', 'Apache-2.0',
 'Largest Gemma 4; needs an 80 GB card at BF16.',
 false, false, 'https://huggingface.co/google/gemma-4-31B-it', '4.0.0'),
('gemma-4-26b-a4b-it', 'Gemma 4 26B A4B IT', 'gemma', '26B MoE (128 experts, 4B active)', 262144, 'BF16',
 48, 56, 72, 1, 64, 96, 8, 16, 'cuda', 'safetensors', 'safetensors', 'Google', 'Apache-2.0',
 'Gemma 4 MoE; cheap per token, all experts resident.',
 false, false, 'https://huggingface.co/google/gemma-4-26B-A4B-it', '4.0.0'),
('gemma-4-12b-it', 'Gemma 4 12B IT', 'gemma', '12B dense', 262144, 'BF16',
 22, 28, 36, 1, 32, 48, 4, 8, 'hybrid', 'safetensors', 'safetensors', 'Google', 'Apache-2.0',
 'Mid-size Gemma 4 with unified text, image and audio input.',
 false, false, 'https://huggingface.co/google/gemma-4-12B-it', '4.0.0'),
('gemma-4-e4b-it', 'Gemma 4 E4B IT', 'gemma', '8B (E4B effective)', 131072, 'BF16',
 15, 18, 24, 1, 20, 32, 4, 8, 'hybrid', 'safetensors', 'safetensors', 'Google', 'Apache-2.0',
 'On-device Gemma 4; about a 4B footprint at runtime.',
 false, false, 'https://huggingface.co/google/gemma-4-E4B-it', '4.0.0'),
('gemma-4-e2b-it', 'Gemma 4 E2B IT', 'gemma', '5B (E2B effective)', 131072, 'BF16',
 10, 12, 16, 1, 12, 16, 2, 4, 'hybrid', 'safetensors', 'safetensors', 'Google', 'Apache-2.0',
 'Smallest Gemma 4; laptop-class.',
 false, false, 'https://huggingface.co/google/gemma-4-E2B-it', '4.0.0'),

-- NVIDIA additions.
('nemotron-3.5-lightning-30b-a3b', 'Nemotron 3.5 Lightning 30B A3B', 'nemotron', '30B-A3B (128 experts, 6 active)', 1048576, 'NVFP4',
 20, 32, 40, 1, 32, 64, 4, 8, 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Nemotron 3.5 Lightning at NVFP4 with a 1M context.',
 false, false, 'https://huggingface.co/nvidia/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-NVFP4', '3.5.0'),
('nemotron-3-nano-4b', 'Nemotron 3 Nano 4B', 'nemotron', '4B dense', 262144, 'BF16',
 7, 10, 16, 1, 10, 16, 2, 4, 'hybrid', 'safetensors', 'safetensors', 'NVIDIA', 'NVIDIA Open Model License',
 'The smallest Nemotron 3; fits an 8 GB card at 4-bit.',
 false, false, 'https://huggingface.co/nvidia/NVIDIA-Nemotron-3-Nano-4B-BF16', '3.0.0'),
('qwen3.6-35b-a3b-nvfp4', 'Qwen3.6 35B A3B (NVFP4)', 'qwen', '35B MoE (256 experts, 8 active)', 262144, 'NVFP4',
 22, 32, 40, 1, 32, 64, 4, 8, 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'Apache-2.0',
 'NVIDIA''s NVFP4 build of Qwen3.6 MoE for TensorRT-LLM.',
 false, false, 'https://huggingface.co/nvidia/Qwen3.6-35B-A3B-NVFP4', '3.6.0'),

-- Meta and Mistral large.
('llama-3.1-405b-fp8', 'Llama 3.1 405B (FP8)', 'llama', '406B dense', 131072, 'FP8',
 454, 500, 620, 7, 512, 640, 32, 64, 'cuda', 'safetensors', 'fp8', 'Meta', 'Llama 3.1 Community License',
 'The 405B dense Llama; gated on Hugging Face, seven 80 GB cards.',
 true, false, 'https://huggingface.co/meta-llama/Llama-3.1-405B-FP8', '3.1.0'),
('mistral-small-3.2-24b', 'Mistral Small 3.2 24B', 'mistral', '24B dense', 131072, 'BF16',
 89, 56, 72, 1, 64, 96, 8, 16, 'cuda', 'safetensors', 'safetensors', 'Mistral AI', 'Apache-2.0',
 'Mistral Small 3.2 with vision; the published repo carries both BF16 and consolidated weights.',
 false, false, 'https://huggingface.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506', '3.2.0'),
('devstral-small-2-24b', 'Devstral Small 2 24B', 'mistral', '24B dense', 393216, 'FP8',
 48, 56, 72, 1, 64, 96, 8, 16, 'cuda', 'safetensors', 'fp8', 'Mistral AI', 'Apache-2.0',
 'Agentic coding model with a 384k context.',
 false, false, 'https://huggingface.co/mistralai/Devstral-Small-2-24B-Instruct-2512', '2.0.0'),
('ministral-3-14b', 'Ministral 3 14B', 'mistral', '14B dense', 262144, 'FP8',
 29, 36, 48, 1, 40, 64, 4, 8, 'hybrid', 'safetensors', 'fp8', 'Mistral AI', 'Apache-2.0',
 'Ministral 3 at FP8; fits a 48 GB card.',
 false, false, 'https://huggingface.co/mistralai/Ministral-3-14B-Instruct-2512', '3.0.0');

INSERT INTO models (
    key, name, family, parameters, context_length, quantization,
    weights_bytes, min_vram_bytes, recommended_vram_bytes, min_gpu_count,
    min_ram_bytes, recommended_ram_bytes, min_cpu_cores, recommended_cpu_cores,
    runtime, arch, artifact_format, publisher, license, description,
    unified_memory_only, is_cpu_only, source_uri, version, is_remote, source, pinned
)
SELECT
    s.key, s.name, s.family, s.parameters, s.context_length, s.quantization,
    (s.weights_gib * 1073741824)::bigint,
    (s.min_vram_gib * 1073741824)::bigint,
    (s.rec_vram_gib * 1073741824)::bigint,
    s.min_gpu_count,
    (s.min_ram_gib * 1073741824)::bigint,
    (s.rec_ram_gib * 1073741824)::bigint,
    s.min_cores, s.rec_cores, s.runtime, s.arch, s.artifact_format,
    s.publisher, s.license, s.description,
    s.unified_only, s.cpu_only, s.source_uri, s.version, false, 'seed', true
FROM model_seed s
ON CONFLICT (key) DO UPDATE SET
    name = EXCLUDED.name, family = EXCLUDED.family, parameters = EXCLUDED.parameters,
    context_length = EXCLUDED.context_length, quantization = EXCLUDED.quantization,
    weights_bytes = EXCLUDED.weights_bytes, min_vram_bytes = EXCLUDED.min_vram_bytes,
    recommended_vram_bytes = EXCLUDED.recommended_vram_bytes, min_gpu_count = EXCLUDED.min_gpu_count,
    min_ram_bytes = EXCLUDED.min_ram_bytes, recommended_ram_bytes = EXCLUDED.recommended_ram_bytes,
    min_cpu_cores = EXCLUDED.min_cpu_cores, recommended_cpu_cores = EXCLUDED.recommended_cpu_cores,
    runtime = EXCLUDED.runtime, arch = EXCLUDED.arch, artifact_format = EXCLUDED.artifact_format,
    publisher = EXCLUDED.publisher, license = EXCLUDED.license, description = EXCLUDED.description,
    unified_memory_only = EXCLUDED.unified_memory_only, is_cpu_only = EXCLUDED.is_cpu_only,
    source_uri = EXCLUDED.source_uri, version = EXCLUDED.version, updated_at = now();

DROP TABLE model_seed;

-- The artifact formats the sync will also produce; widen the existing check.
ALTER TABLE models DROP CONSTRAINT IF EXISTS models_runtime_valid;
ALTER TABLE models ADD CONSTRAINT models_runtime_valid CHECK (runtime IN ('cuda', 'cpu', 'hybrid'));
