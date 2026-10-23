# Nemotron models

Only Nemotron is supported as a first-party local model. That is a deliberate product decision: one
model family means one set of provider profiles, one set of policy profiles, and one thing to test.

| Model | Parameters | Context | Notes |
|---|---|---|---|
| Nemotron Nano 12B v2 VL | 12.6B dense | 128K | vision: up to 5 images, documents, video |
| Nemotron 3 Nano | 30B-A3B (3.2B active) | 1M | fastest in the family, hybrid Mamba-Transformer MoE |
| Nemotron 3 Super | 120B-A12B | 1M | multi-agent and coding; 85.6 % PinchBench (OpenClaw) |
| Nemotron 3 Ultra | 550B-A55B | 1M | strongest open model in the family, NVFP4 |

Sources and dates are in [../claims.md](../claims.md).

## How a nest reaches a model

Nestbox never injects a model endpoint into an agent's code. It registers a **provider profile**
and attaches it to the nest:

- **Cloud** — a NIM endpoint on `build.nvidia.com`, or any OpenAI-compatible provider.
- **Local** — a `local-nemotron` nest running vLLM, reachable over the overlay at
  `http://<overlay-ip>:8000/v1`.
- **Your laptop** — the same thing, on your laptop, over WireGuard.

The policy does not change between these. What changes is one profile.

## VRAM guidance

See [../hardware/](hardware/) for the sizing table and the caveats. The short version:

- 12B VL → 24–32 GB VRAM
- 3 Nano → 32–48 GB
- 3 Super → 80 GB
- 3 Ultra → 4× 96 GB or a DGX B200

Context length is the hidden cost: a 1M-token window needs hundreds of GB of KV cache, so cap the
context on single-GPU deployments (32K–128K is the practical default) unless the hardware is sized
for more.
