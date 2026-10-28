-- 0030_agent_context_window — the window Hermes accepts, within this node's RAM.
--
-- 0029 set 65,536, but Qwen3-1.7B was trained on 40,960 tokens: llama.cpp capped
-- the slot back to 40,960 and Hermes still refused it. RoPE scaling (YaRN, ×2)
-- makes the server honour the configured window; the KV cache is quantised to
-- q8_0 because 65,536 tokens at fp16 would be about 7 GiB, more than this node
-- reserves for the whole model.
--
-- Measured on the appliance: 3.79 GiB resident, ready in ~6 s on four cores.
-- The quality cost of 2× context extrapolation is real and is why this is
-- recorded as a decision rather than a default.

UPDATE model_server_images
   SET command = ARRAY['-m', '{model}', '--host', '0.0.0.0', '--port', '8080',
                       '--threads', '{threads}', '--ctx-size', '65536',
                       '--cache-type-k', 'q8_0', '--cache-type-v', 'q8_0',
                       '--rope-scaling', 'yarn', '--rope-scale', '2',
                       '--yarn-orig-ctx', '40960', '--jinja'],
       notes = 'llama.cpp server, CPU. One GGUF file, port 8080, an OpenAI-compatible /v1. 65k window via YaRN and q8 KV so an agent framework accepts it within a 5 GiB reservation.'
 WHERE runtime = 'llama_cpp';
