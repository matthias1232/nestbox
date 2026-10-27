-- 0013_model_catalog — the broad catalog.
--
-- Requirements are data, not constants (ADR 0013). Everything an operator needs
-- in order to judge "can this machine run it" lives in the row: weights, the
-- VRAM or RAM gate, a recommended figure, CPU cores, runtime, publisher, and
-- licence.
--
-- How the numbers were derived, stated once so every row is auditable:
--
--   * weights_gib  = parameters x bytes-per-parameter at the stated format
--                    (FP16 2.0, FP8 1.0, Q8 1.05, Q4_K_M 0.60, NVFP4 0.55,
--                     AWQ 0.55). MoE figures use the *total* parameter count,
--                    because all experts must be resident even though only a
--                    few are active per token.
--   * min_vram_gib = weights + KV cache and runtime overhead. Roughly
--                    weights x 1.2 for models under 10 GiB and weights + 6..16
--                    GiB above that, rounded up.
--   * rec_vram_gib = min x 1.25..1.5, so the bar can show "starts" versus
--                    "comfortable".
--   * min_ram_gib  = for CPU/GGUF rows, the file plus roughly 2 GiB of
--                    working memory (llama.cpp mmaps the weights); for hybrid
--                    rows, the RAM needed for the CPU fallback.
--
-- These are planning figures, not measurements. A value that is too optimistic
-- produces a false "installable"; a value that is too pessimistic hides a model
-- that would have worked. Where a vendor publishes a hard minimum, that number
-- wins over the formula; the row comment says so.
--
-- The staging table keeps the numbers readable in GiB. Bytes are computed once
-- on insert, so a reader never has to check whether 25769803776 is 24 GiB.

CREATE TEMP TABLE model_seed (
    key               text PRIMARY KEY,
    name              text NOT NULL,
    family            text NOT NULL,
    parameters        text NOT NULL,
    context_length    bigint,
    quantization      text,
    weights_gib       numeric,
    min_vram_gib      numeric,
    rec_vram_gib      numeric,
    min_gpu_count     integer NOT NULL DEFAULT 1,
    min_ram_gib       numeric,
    rec_ram_gib       numeric,
    min_cores         integer,
    rec_cores         integer,
    runtime           text NOT NULL,
    arch              text,
    artifact_format   text,
    publisher         text,
    license           text,
    description       text,
    unified_only      boolean NOT NULL DEFAULT false,
    cpu_only          boolean NOT NULL DEFAULT false,
    source_uri        text,
    version           text NOT NULL
);

INSERT INTO model_seed (
    key, name, family, parameters, context_length, quantization,
    weights_gib, min_vram_gib, rec_vram_gib, min_gpu_count, min_ram_gib,
    rec_ram_gib, min_cores, rec_cores, runtime, arch, artifact_format,
    publisher, license, description, unified_only, cpu_only, source_uri, version
) VALUES

-- ---------------------------------------------------------------------------
-- NVIDIA / Nemotron. TensorRT-LLM, NVFP4. The 12B and 30B entries already
-- existed; they gain the RAM, core, and licence fields here.
-- ---------------------------------------------------------------------------
('nemotron-nano-12b-v2-vl', 'Nemotron Nano 12B v2 VL', 'nemotron',
 '12.6B dense', 131072, 'NVFP4', 14, 24, 32, 1, 32, 48, 4, 8,
 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Vision-language model for document and image reasoning.', false, false,
 'https://build.nvidia.com/nvidia/nemotron-nano-12b-v2-vl', '2.0.0'),

('nemotron-nano-9b-v2', 'Nemotron Nano 9B v2', 'nemotron',
 '9B dense', 131072, 'NVFP4', 6, 12, 16, 1, 24, 32, 4, 8,
 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Small dense model for high-throughput agent loops.', false, false,
 'https://build.nvidia.com/nvidia/nemotron-nano-9b-v2', '2.0.0'),

('nemotron-3-nano', 'Nemotron 3 Nano', 'nemotron',
 '30B-A3B (3.2B active)', 1048576, 'NVFP4', 18, 32, 40, 1, 32, 64, 4, 8,
 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Mixture-of-experts model with a very long context and low active cost.',
 false, false, 'https://research.nvidia.com/labs/nemotron/Nemotron-3/', '3.0.0'),

('nemotron-3-super', 'Nemotron 3 Super', 'nemotron',
 '120B-A12B', 1048576, 'NVFP4', 65, 80, 96, 1, 96, 128, 8, 16,
 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Large MoE for long-context reasoning on one 80 GB card.', false, false,
 'https://developer.nvidia.com/blog/introducing-nemotron-3-super/', '3.0.0'),

('nemotron-3-ultra', 'Nemotron 3 Ultra', 'nemotron',
 '550B-A55B', 1048576, 'NVFP4', 280, 384, 512, 4, 384, 512, 16, 32,
 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Frontier MoE. Needs four 96 GiB cards or a 384 GiB unified pool.',
 true, false, 'https://research.nvidia.com/labs/nemotron/Nemotron-3-Ultra/', '3.0.0'),

('nemotron-4-340b-instruct', 'Nemotron 4 340B Instruct', 'nemotron',
 '340B dense', 4096, 'FP8', 340, 400, 512, 8, 400, 512, 16, 32,
 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Synthetic-data and alignment model; an eight-GPU reference workload.',
 true, false, 'https://build.nvidia.com/nvidia/nemotron-4-340b-instruct', '4.0.0'),

('llama-3.1-nemotron-70b-instruct', 'Llama 3.1 Nemotron 70B Instruct',
 'nemotron', '70B dense', 131072, 'NVFP4', 39, 48, 64, 1, 64, 96, 8, 16,
 'cuda', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Instruction-tuned 70B with strong response quality per token.', false, false,
 'https://build.nvidia.com/nvidia/llama-3_1-nemotron-70b-instruct', '1.0.0'),

('llama-3.1-nemotron-nano-8b-v1', 'Llama 3.1 Nemotron Nano 8B v1',
 'nemotron', '8B dense', 131072, 'NVFP4', 5, 10, 16, 1, 16, 24, 4, 8,
 'hybrid', 'tensorrt', 'nvfp4', 'NVIDIA', 'NVIDIA Open Model License',
 'Compact instruct model for agents on a single mid-range card.', false, false,
 'https://build.nvidia.com/nvidia/llama-3_1-nemotron-nano-8b-v1', '1.0.0'),

-- ---------------------------------------------------------------------------
-- DeepSeek. The distills are small dense models and run well as GGUF on a CPU;
-- the V3/R1 base models are 671B MoE and need a pool, not a card.
-- ---------------------------------------------------------------------------
('deepseek-r1-distill-qwen-1.5b', 'DeepSeek R1 Distill Qwen 1.5B', 'deepseek',
 '1.5B dense', 131072, 'Q4_K_M', 1.1, NULL, NULL, 1, 3, 6, 2, 4,
 'cpu', 'gguf', 'gguf', 'DeepSeek', 'MIT',
 'Smallest R1 distill; a reasoning model that runs on any laptop.', false, true,
 'https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B', '1.0.0'),

('deepseek-r1-distill-llama-8b', 'DeepSeek R1 Distill Llama 8B', 'deepseek',
 '8B dense', 131072, 'Q4_K_M', 5.0, NULL, NULL, 1, 8, 16, 4, 8,
 'cpu', 'gguf', 'gguf', 'DeepSeek', 'MIT',
 'The most popular R1 distill; good reasoning at CPU speed.', false, true,
 'https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Llama-8B', '1.0.0'),

('deepseek-r1-distill-qwen-14b', 'DeepSeek R1 Distill Qwen 14B', 'deepseek',
 '14B dense', 131072, 'Q4_K_M', 9.0, NULL, NULL, 1, 12, 24, 4, 8,
 'cpu', 'gguf', 'gguf', 'DeepSeek', 'MIT',
 'Middle distill: noticeably better reasoning, still CPU-hostable.', false, true,
 'https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-14B', '1.0.0'),

('deepseek-r1-distill-qwen-32b', 'DeepSeek R1 Distill Qwen 32B', 'deepseek',
 '32B dense', 131072, 'Q4_K_M', 20, NULL, NULL, 1, 24, 48, 4, 8,
 'cpu', 'gguf', 'gguf', 'DeepSeek', 'MIT',
 'Largest distill that still fits a workstation without a GPU.', false, true,
 'https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B', '1.0.0'),

('deepseek-r1-distill-qwen-7b', 'DeepSeek R1 Distill Qwen 7B', 'deepseek',
 '7B dense', 131072, 'Q4_K_M', 4.4, 8, 12, 1, 12, 16, 4, 8,
 'hybrid', 'gguf', 'gguf', 'DeepSeek', 'MIT',
 'Small reasoner that runs on a 8 GB card or on the CPU.', false, false,
 'https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-7B', '1.0.0'),

('deepseek-r1', 'DeepSeek R1', 'deepseek',
 '671B-A37B', 163840, 'FP8', 700, 720, 800, 8, 768, 1024, 32, 64,
 'cuda', 'safetensors', 'fp8', 'DeepSeek', 'DeepSeek License',
 'Full R1 MoE. A datacentre workload, not a workstation one.', true, false,
 'https://huggingface.co/deepseek-ai/DeepSeek-R1', '1.0.0'),

('deepseek-v3.1', 'DeepSeek V3.1', 'deepseek',
 '671B-A37B', 163840, 'FP8', 700, 720, 800, 8, 768, 1024, 32, 64,
 'cuda', 'safetensors', 'fp8', 'DeepSeek', 'DeepSeek License',
 'Hybrid thinking and non-thinking 671B MoE.', true, false,
 'https://huggingface.co/deepseek-ai/DeepSeek-V3.1', '3.1.0'),

('deepseek-coder-v2-lite-instruct', 'DeepSeek Coder V2 Lite Instruct',
 'deepseek', '16B-A2.4B', 131072, 'Q4_K_M', 10, 16, 24, 1, 24, 32, 4, 8,
 'hybrid', 'safetensors', 'awq', 'DeepSeek', 'DeepSeek License',
 'MoE coding model that fits a 16 GB card and falls back to RAM.', false, false,
 'https://huggingface.co/deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct', '2.0.0'),

-- ---------------------------------------------------------------------------
-- Qwen. Apache-2.0 across the family, dense and MoE, all sizes.
-- ---------------------------------------------------------------------------
('qwen3-0.6b', 'Qwen3 0.6B', 'qwen',
 '0.6B dense', 32768, 'Q4_K_M', 0.4, NULL, NULL, 1, 2, 3, 2, 2,
 'cpu', 'gguf', 'gguf', 'Alibaba', 'Apache-2.0',
 'Tiny model for classification, routing, and smoke tests without a GPU.',
 false, true, 'https://huggingface.co/Qwen/Qwen3-0.6B', '3.0.0'),

('qwen3-1.7b', 'Qwen3 1.7B', 'qwen',
 '1.7B dense', 32768, 'Q4_K_M', 1.1, NULL, NULL, 1, 3, 5, 2, 4,
 'cpu', 'gguf', 'gguf', 'Alibaba', 'Apache-2.0',
 'Small instruct model that is usable on a laptop CPU.', false, true,
 'https://huggingface.co/Qwen/Qwen3-1.7B', '3.0.0'),

('qwen3-4b', 'Qwen3 4B', 'qwen',
 '4B dense', 32768, 'Q4_K_M', 2.5, 8, 12, 1, 8, 12, 2, 4,
 'hybrid', 'safetensors', 'awq', 'Alibaba', 'Apache-2.0',
 'Entry-level card model; also runs on the CPU as GGUF.', false, false,
 'https://huggingface.co/Qwen/Qwen3-4B', '3.0.0'),

('qwen3-8b', 'Qwen3 8B', 'qwen',
 '8B dense', 32768, 'Q4_K_M', 5.0, 12, 16, 1, 12, 20, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Alibaba', 'Apache-2.0',
 'The default general-purpose local model for a 12 GB card.', false, false,
 'https://huggingface.co/Qwen/Qwen3-8B', '3.0.0'),

('qwen3-14b', 'Qwen3 14B', 'qwen',
 '14B dense', 32768, 'Q4_K_M', 9.0, 20, 28, 1, 20, 32, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Alibaba', 'Apache-2.0',
 'Stronger reasoning at 14B; needs a 24 GB class card.', false, false,
 'https://huggingface.co/Qwen/Qwen3-14B', '3.0.0'),

('qwen3-32b', 'Qwen3 32B', 'qwen',
 '32B dense', 32768, 'Q4_K_M', 20, 40, 48, 1, 40, 64, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Alibaba', 'Apache-2.0',
 'Near-frontier dense quality on one 48 GB class card.', false, false,
 'https://huggingface.co/Qwen/Qwen3-32B', '3.0.0'),

('qwen3-235b-a22b', 'Qwen3 235B A22B', 'qwen',
 '235B-A22B', 131072, 'FP8', 235, 256, 320, 4, 256, 384, 16, 32,
 'cuda', 'safetensors', 'fp8', 'Alibaba', 'Apache-2.0',
 'Flagship MoE; four 80 GB cards or a unified pool.', true, false,
 'https://huggingface.co/Qwen/Qwen3-235B-A22B', '3.0.0'),

('qwen3-coder-30b-a3b', 'Qwen3 Coder 30B A3B', 'qwen',
 '30B-A3B', 262144, 'Q4_K_M', 18, 32, 40, 1, 32, 64, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Alibaba', 'Apache-2.0',
 'Coding MoE with a 256k context and low active cost.', false, false,
 'https://huggingface.co/Qwen/Qwen3-Coder-30B-A3B-Instruct', '3.0.0'),

('qwen2.5-coder-1.5b', 'Qwen2.5 Coder 1.5B', 'qwen',
 '1.5B dense', 32768, 'Q4_K_M', 1.0, NULL, NULL, 1, 3, 5, 2, 4,
 'cpu', 'gguf', 'gguf', 'Alibaba', 'Apache-2.0',
 'Completion and inline-edit model that costs almost nothing to run.',
 false, true, 'https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B', '2.5.0'),

('qwen2.5-coder-7b', 'Qwen2.5 Coder 7B', 'qwen',
 '7B dense', 32768, 'Q4_K_M', 4.4, NULL, NULL, 1, 8, 16, 4, 8,
 'cpu', 'gguf', 'gguf', 'Alibaba', 'Apache-2.0',
 'The workhorse local coding model; CPU-friendly as GGUF.', false, true,
 'https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct', '2.5.0'),

('qwen2.5-3b-instruct', 'Qwen2.5 3B Instruct', 'qwen',
 '3B dense', 32768, 'Q4_K_M', 1.9, NULL, NULL, 1, 6, 10, 2, 4,
 'cpu', 'gguf', 'gguf', 'Alibaba', 'Apache-2.0',
 'Compact general model for CPU-only appliances.', false, true,
 'https://huggingface.co/Qwen/Qwen2.5-3B-Instruct', '2.5.0'),

-- ---------------------------------------------------------------------------
-- Llama. Sizes from 1B to the 400B Maverick MoE.
-- ---------------------------------------------------------------------------
('llama-3.2-1b-instruct', 'Llama 3.2 1B Instruct', 'llama',
 '1.2B dense', 131072, 'Q4_K_M', 0.8, NULL, NULL, 1, 3, 5, 2, 2,
 'cpu', 'gguf', 'gguf', 'Meta', 'Llama 3.2 Community License',
 'The smallest Llama; useful for extraction and routing on a CPU.', false, true,
 'https://huggingface.co/meta-llama/Llama-3.2-1B-Instruct', '3.2.0'),

('llama-3.2-3b-instruct', 'Llama 3.2 3B Instruct', 'llama',
 '3.2B dense', 131072, 'Q4_K_M', 2.0, NULL, NULL, 1, 5, 8, 2, 4,
 'cpu', 'gguf', 'gguf', 'Meta', 'Llama 3.2 Community License',
 'Small instruct model with a long context; runs on a CPU.', false, true,
 'https://huggingface.co/meta-llama/Llama-3.2-3B-Instruct', '3.2.0'),

('llama-3.1-8b-instruct', 'Llama 3.1 8B Instruct', 'llama',
 '8B dense', 131072, 'Q4_K_M', 5.0, 10, 16, 1, 12, 16, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Meta', 'Llama 3.1 Community License',
 'Widely supported baseline; a 12 GB card is enough.', false, false,
 'https://huggingface.co/meta-llama/Llama-3.1-8B-Instruct', '3.1.0'),

('llama-3.1-70b-instruct', 'Llama 3.1 70B Instruct', 'llama',
 '70B dense', 131072, 'Q4_K_M', 40, 44, 56, 1, 64, 96, 8, 16,
 'cuda', 'safetensors', 'awq', 'Meta', 'Llama 3.1 Community License',
 '70B quality at 4-bit; fits one 48 GB card or two 24 GB cards.', false, false,
 'https://huggingface.co/meta-llama/Llama-3.1-70B-Instruct', '3.1.0'),

('llama-3.3-70b-instruct', 'Llama 3.3 70B Instruct', 'llama',
 '70B dense', 131072, 'Q4_K_M', 40, 44, 56, 1, 64, 96, 8, 16,
 'cuda', 'safetensors', 'awq', 'Meta', 'Llama 3.3 Community License',
 'Current 70B generation; same footprint as 3.1 70B.', false, false,
 'https://huggingface.co/meta-llama/Llama-3.3-70B-Instruct', '3.3.0'),

('llama-4-scout-17b-16e-instruct', 'Llama 4 Scout 17B 16E Instruct', 'llama',
 '109B-A17B', 10485760, 'Q4_K_M', 60, 64, 80, 1, 96, 128, 8, 16,
 'cuda', 'safetensors', 'awq', 'Meta', 'Llama 4 Community License',
 'Ten-million-token context MoE; needs a 64 GB class card.', false, false,
 'https://huggingface.co/meta-llama/Llama-4-Scout-17B-16E-Instruct', '4.0.0'),

('llama-4-maverick-17b-128e-instruct', 'Llama 4 Maverick 17B 128E Instruct',
 'llama', '400B-A17B', 1048576, 'FP8', 400, 420, 512, 8, 448, 512, 16, 32,
 'cuda', 'safetensors', 'fp8', 'Meta', 'Llama 4 Community License',
 'Meta''s open flagship MoE; a multi-GPU or unified-pool workload.',
 true, false,
 'https://huggingface.co/meta-llama/Llama-4-Maverick-17B-128E-Instruct', '4.0.0'),

-- ---------------------------------------------------------------------------
-- Gemma. Size classes 1B to 27B plus the 3n on-device line.
-- ---------------------------------------------------------------------------
('gemma-2-2b-it', 'Gemma 2 2B IT', 'gemma',
 '2.6B dense', 8192, 'Q4_K_M', 1.7, NULL, NULL, 1, 4, 8, 2, 4,
 'cpu', 'gguf', 'gguf', 'Google', 'Gemma Terms of Use',
 'Small chat model with a tight context; fine on a CPU.', false, true,
 'https://huggingface.co/google/gemma-2-2b-it', '2.0.0'),

('gemma-2-9b-it', 'Gemma 2 9B IT', 'gemma',
 '9B dense', 8192, 'Q4_K_M', 5.5, 12, 18, 1, 16, 24, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Google', 'Gemma Terms of Use',
 'Balanced 9B chat model for a 12 GB card.', false, false,
 'https://huggingface.co/google/gemma-2-9b-it', '2.0.0'),

('gemma-2-27b-it', 'Gemma 2 27B IT', 'gemma',
 '27B dense', 8192, 'Q4_K_M', 16, 24, 32, 1, 32, 48, 8, 16,
 'cuda', 'safetensors', 'awq', 'Google', 'Gemma Terms of Use',
 '27B chat model; a 24 GB card is the floor.', false, false,
 'https://huggingface.co/google/gemma-2-27b-it', '2.0.0'),

('gemma-3-1b-it', 'Gemma 3 1B IT', 'gemma',
 '1B dense', 32768, 'Q4_K_M', 0.8, NULL, NULL, 1, 3, 6, 2, 2,
 'cpu', 'gguf', 'gguf', 'Google', 'Gemma Terms of Use',
 'Tiny multimodal-lineage model; the cheapest useful chat on a CPU.',
 false, true, 'https://huggingface.co/google/gemma-3-1b-it', '3.0.0'),

('gemma-3-4b-it', 'Gemma 3 4B IT', 'gemma',
 '4B dense', 131072, 'Q4_K_M', 2.5, 8, 12, 1, 8, 12, 2, 4,
 'hybrid', 'safetensors', 'awq', 'Google', 'Gemma Terms of Use',
 'Compact model with a 128k context and image input.', false, false,
 'https://huggingface.co/google/gemma-3-4b-it', '3.0.0'),

('gemma-3-12b-it', 'Gemma 3 12B IT', 'gemma',
 '12B dense', 131072, 'Q4_K_M', 7.3, 16, 24, 1, 16, 32, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Google', 'Gemma Terms of Use',
 'Mid-size multimodal model for a 16 GB card.', false, false,
 'https://huggingface.co/google/gemma-3-12b-it', '3.0.0'),

('gemma-3-27b-it', 'Gemma 3 27B IT', 'gemma',
 '27B dense', 131072, 'Q4_K_M', 16.5, 24, 32, 1, 32, 48, 8, 16,
 'cuda', 'safetensors', 'awq', 'Google', 'Gemma Terms of Use',
 'Largest Gemma 3; needs a 24 GB card and a long-context budget.',
 false, false, 'https://huggingface.co/google/gemma-3-27b-it', '3.0.0'),

('gemma-3n-e4b-it', 'Gemma 3n E4B IT', 'gemma',
 '8B (E4B effective)', 32768, 'Q4_K_M', 4.0, 8, 12, 1, 8, 16, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Google', 'Gemma Terms of Use',
 'On-device model with audio and vision input; about a 4B footprint.',
 false, false, 'https://huggingface.co/google/gemma-3n-E4B-it', '3.0.0'),

-- ---------------------------------------------------------------------------
-- Phi. Microsoft's small-model line; the minis are CPU-friendly.
-- ---------------------------------------------------------------------------
('phi-3-mini-3.8b', 'Phi-3 Mini 3.8B', 'phi',
 '3.8B dense', 131072, 'Q4_K_M', 2.4, NULL, NULL, 1, 6, 10, 2, 4,
 'cpu', 'gguf', 'gguf', 'Microsoft', 'MIT',
 'Reasoning-focused small model; a common CPU baseline.', false, true,
 'https://huggingface.co/microsoft/Phi-3-mini-4k-instruct', '3.0.0'),

('phi-3.5-mini-instruct', 'Phi-3.5 Mini Instruct', 'phi',
 '3.8B dense', 131072, 'Q4_K_M', 2.4, NULL, NULL, 1, 6, 10, 2, 4,
 'cpu', 'gguf', 'gguf', 'Microsoft', 'MIT',
 'Successor to Phi-3 Mini with better multilingual behaviour.', false, true,
 'https://huggingface.co/microsoft/Phi-3.5-mini-instruct', '3.5.0'),

('phi-3-medium-14b', 'Phi-3 Medium 14B', 'phi',
 '14B dense', 131072, 'Q4_K_M', 9.0, 16, 24, 1, 16, 32, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Microsoft', 'MIT',
 '14B dense model that fits a 16 GB card.', false, false,
 'https://huggingface.co/microsoft/Phi-3-medium-4k-instruct', '3.0.0'),

('phi-4-14b', 'Phi-4 14B', 'phi',
 '14B dense', 16384, 'Q4_K_M', 9.0, 20, 28, 1, 20, 32, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Microsoft', 'MIT',
 'Reasoning-heavy 14B; expects a 24 GB card for its context.', false, false,
 'https://huggingface.co/microsoft/phi-4', '4.0.0'),

('phi-4-mini-3.8b', 'Phi-4 Mini 3.8B', 'phi',
 '3.8B dense', 131072, 'Q4_K_M', 2.4, NULL, NULL, 1, 6, 10, 2, 4,
 'cpu', 'gguf', 'gguf', 'Microsoft', 'MIT',
 'Current small Phi; strong instruction following at CPU size.', false, true,
 'https://huggingface.co/microsoft/Phi-4-mini-instruct', '4.0.0'),

-- ---------------------------------------------------------------------------
-- The rest: other publishers, and the smallest models that still answer.
-- ---------------------------------------------------------------------------
('mistral-7b-instruct-v0.3', 'Mistral 7B Instruct v0.3', 'mistral',
 '7B dense', 32768, 'Q4_K_M', 4.4, 10, 16, 1, 12, 16, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Mistral AI', 'Apache-2.0',
 'The classic 7B instruct baseline, with function calling.', false, false,
 'https://huggingface.co/mistralai/Mistral-7B-Instruct-v0.3', '0.3.0'),

('mistral-nemo-12b-instruct', 'Mistral NeMo 12B Instruct', 'mistral',
 '12B dense', 131072, 'Q4_K_M', 7.5, 16, 24, 1, 16, 32, 4, 8,
 'hybrid', 'safetensors', 'awq', 'Mistral AI', 'Apache-2.0',
 '12B model with a 128k context; fits a 16 GB card.', false, false,
 'https://huggingface.co/mistralai/Mistral-Nemo-Instruct-2407', '12.0.0'),

('mixtral-8x7b-instruct', 'Mixtral 8x7B Instruct', 'mistral',
 '47B-A13B', 32768, 'Q4_K_M', 28, 32, 40, 1, 48, 64, 8, 16,
 'cuda', 'safetensors', 'awq', 'Mistral AI', 'Apache-2.0',
 'Early MoE: 47B resident, 13B active; needs a 32 GB card.', false, false,
 'https://huggingface.co/mistralai/Mixtral-8x7B-Instruct-v0.1', '0.1.0'),

('tinyllama-1.1b-chat', 'TinyLlama 1.1B Chat', 'other',
 '1.1B dense', 2048, 'Q4_K_M', 0.7, NULL, NULL, 1, 2, 4, 2, 2,
 'cpu', 'gguf', 'gguf', 'TinyLlama', 'Apache-2.0',
 'The smallest model worth installing; proves the pipeline end to end.',
 false, true, 'https://huggingface.co/TinyLlama/TinyLlama-1.1B-Chat-v1.0', '1.0.0'),

('smollm2-360m-instruct', 'SmolLM2 360M Instruct', 'other',
 '0.36B dense', 8192, 'Q4_K_M', 0.24, NULL, NULL, 1, 1, 2, 1, 2,
 'cpu', 'gguf', 'gguf', 'Hugging Face', 'Apache-2.0',
 'Sub-gigabyte instruct model; runs anywhere, useful for tests.', false, true,
 'https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct', '2.0.0'),

('smollm2-1.7b-instruct', 'SmolLM2 1.7B Instruct', 'other',
 '1.7B dense', 8192, 'Q4_K_M', 1.1, NULL, NULL, 1, 3, 6, 2, 4,
 'cpu', 'gguf', 'gguf', 'Hugging Face', 'Apache-2.0',
 'Small instruct model with surprisingly usable output on a CPU.',
 false, true, 'https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct', '2.0.0');

-- ---------------------------------------------------------------------------
-- Upsert. Bytes are computed here so the seed stays in GiB. A re-run updates
-- the requirement data but never touches install state, which belongs to the
-- environment rather than to the catalog.
-- ---------------------------------------------------------------------------
INSERT INTO models (
    key, name, family, parameters, context_length, quantization,
    weights_bytes, min_vram_bytes, recommended_vram_bytes, min_gpu_count,
    min_ram_bytes, recommended_ram_bytes, min_cpu_cores, recommended_cpu_cores,
    runtime, arch, artifact_format, publisher, license, description,
    unified_memory_only, is_cpu_only, source_uri, version, is_remote
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
    s.unified_only, s.cpu_only, s.source_uri, s.version, false
FROM model_seed s
ON CONFLICT (key) DO UPDATE SET
    name                  = EXCLUDED.name,
    family                = EXCLUDED.family,
    parameters            = EXCLUDED.parameters,
    context_length        = EXCLUDED.context_length,
    quantization          = EXCLUDED.quantization,
    weights_bytes         = EXCLUDED.weights_bytes,
    min_vram_bytes        = EXCLUDED.min_vram_bytes,
    recommended_vram_bytes = EXCLUDED.recommended_vram_bytes,
    min_gpu_count         = EXCLUDED.min_gpu_count,
    min_ram_bytes         = EXCLUDED.min_ram_bytes,
    recommended_ram_bytes = EXCLUDED.recommended_ram_bytes,
    min_cpu_cores         = EXCLUDED.min_cpu_cores,
    recommended_cpu_cores = EXCLUDED.recommended_cpu_cores,
    runtime               = EXCLUDED.runtime,
    arch                  = EXCLUDED.arch,
    artifact_format       = EXCLUDED.artifact_format,
    publisher             = EXCLUDED.publisher,
    license               = EXCLUDED.license,
    description           = EXCLUDED.description,
    unified_memory_only   = EXCLUDED.unified_memory_only,
    is_cpu_only           = EXCLUDED.is_cpu_only,
    source_uri            = EXCLUDED.source_uri,
    version               = EXCLUDED.version,
    updated_at            = now();

DROP TABLE model_seed;

COMMENT ON TABLE models IS
  'Catalog of installable and remote models. min_vram_bytes is the GPU gate, '
  'min_ram_bytes the CPU gate; both are planning figures derived from weights '
  'times quantisation plus overhead, and are documented per row in migration 0013.';
