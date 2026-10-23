# Local AI hardware

## Sizing table

| Model | Checkpoint | Weights (estimate) | Fits on |
|---|---|---|---|
| Nemotron Nano 12B v2 VL | BF16 / FP8 | ~26 GB / ~14 GB | RTX 4090 24 GB, RTX 5090 32 GB |
| Nemotron 3 Nano | FP8 / NVFP4 | ~32 GB / ~18 GB | RTX 5090 32 GB, L40S 48 GB |
| Nemotron 3 Super | NVFP4 / BF16 | ~65–70 GB / ~240 GB | A100 80 GB, H100 80 GB, RTX PRO 6000 96 GB |
| Nemotron 3 Ultra | NVFP4 / BF16 | ~280–300 GB / ~1.1 TB | 4× RTX PRO 6000, DGX B200, clustered DGX Spark |

**These numbers are our own arithmetic** — roughly `0.55 bytes × parameters` for NVFP4 weights plus
activations and KV cache. NVIDIA does not publish a VRAM table for these checkpoints. Measure
before you buy, and treat this page as a starting point rather than a guarantee.

## DGX Spark

A GB10 machine with 128 GB of unified memory and NVFP4 throughput in the petaflop class is the
natural single-box target for Nemotron 3 Nano and, with a capped context, Super. It is not a
single-node target for Ultra.

The appliance runs on it unchanged: same ISO, same UI, same policies. The only difference is that
the `local-nemotron` pack has a GPU to attach to, so the provider profile can point at localhost.

## No GPU at all

Nothing breaks. Use cloud models, keep the policies identical, and move the provider profile to a
local endpoint when the hardware arrives. That is the recommended way to evaluate the workflow
before spending money.
