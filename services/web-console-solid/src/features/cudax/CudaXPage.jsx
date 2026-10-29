// CUDA-X is its own catalogue. Entries remain disabled until the node reports
// an NVIDIA container runtime and a compatible CUDA driver; this is intentional
// so the screen cannot claim an installation or VRAM reservation that the node
// cannot enforce.

import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import * as models from '../../api/models.js';
import { gib } from '../../components/ResourceBars.jsx';

const COMPONENTS = [
  { key: 'cuda-toolkit', name: 'CUDA Toolkit', group: 'Compute & base', description: 'Compiler, runtime and developer tools for CUDA applications.', ram: 2, disk: 6, gpu: false },
  { key: 'cudnn', name: 'cuDNN', group: 'Deep learning', description: 'Optimised neural-network primitives used by PyTorch and TensorFlow.', ram: 2, disk: 2, gpu: true },
  { key: 'tensorrt', name: 'TensorRT', group: 'Deep learning', description: 'Optimised inference runtime and engine builder.', ram: 4, disk: 5, gpu: true },
  { key: 'tensorrt-llm', name: 'TensorRT-LLM', group: 'Deep learning', description: 'LLM inference runtime; model VRAM is reserved separately in Models.', ram: 8, disk: 8, gpu: true },
  { key: 'nccl', name: 'NCCL', group: 'Deep learning', description: 'Collective communication for multi-GPU training.', ram: 1, disk: 1, gpu: true, multi: true },
  { key: 'rapids', name: 'RAPIDS', group: 'Data science', description: 'GPU dataframes, machine learning and graph analytics.', ram: 8, disk: 12, gpu: true },
  { key: 'dali', name: 'NVIDIA DALI', group: 'Data science', description: 'GPU-accelerated data loading and image/video pipelines.', ram: 4, disk: 3, gpu: true },
  { key: 'triton', name: 'Triton Inference Server', group: 'Serving', description: 'A managed local inference server for assigned models.', ram: 8, disk: 5, gpu: true },
  { key: 'nemo', name: 'NeMo Framework', group: 'Training', description: 'Training and customisation framework for generative AI.', ram: 16, disk: 15, gpu: true },
  { key: 'video-codec', name: 'Video Codec SDK', group: 'Graphics & video', description: 'NVENC/NVDEC access for supported NVIDIA GPUs.', ram: 2, disk: 1, gpu: true },
];

export default function CudaXPage() {
  const [hardware, setHardware] = createSignal(null);
  const [search, setSearch] = createSignal('');
  const [error, setError] = createSignal(null);
  const capable = () => Boolean(hardware()?.container_gpu && hardware()?.has_gpu);
  async function probe() { setError(null); try { setHardware(await models.hardware(true)); } catch (failure) { setError(failure.message); } }
  onMount(probe);
  const visible = createMemo(() => { const n = search().trim().toLowerCase(); return COMPONENTS.filter((c) => !n || `${c.name} ${c.group} ${c.description}`.toLowerCase().includes(n)); });
  return <section class="page"><header class="page-head"><div><h1>CUDA-X</h1><p class="muted">NVIDIA compute libraries are managed separately from models and skills. Requirements come before any installation action.</p></div><button class="secondary" onClick={probe}>Probe node again</button></header>
    <Show when={error()}><p class="error">{error()}</p></Show>
    <Show when={hardware()}><div class="oc-box"><b>{hardware().node}</b> · {hardware().cpu_cores || 'unknown'} CPU cores · {gib(hardware().system_ram_bytes || 0)} RAM · {hardware().gpu_count || 0} GPU(s) · {gib(hardware().total_vram_bytes || hardware().unified_memory_bytes || 0)} GPU/unified memory<div class="muted small">{capable() ? `Container GPU enabled · CUDA ${hardware().cuda_version || 'version not reported'} · driver ${hardware().driver_version || 'not reported'}` : `Unavailable for installation: ${hardware().note || 'this node does not offer a container-addressable NVIDIA GPU.'}`}</div></div></Show>
    <p class="notice">Download/installation is disabled until the image-builder has a verified, version-pinned CUDA-X artifact source. This avoids installing unverified binaries into a sandbox. Resource figures are planning minima; model VRAM reservations stay on the Models page.</p>
    <label class="field"><span>Find a CUDA-X component</span><input value={search()} placeholder="TensorRT, video, training …" onInput={(e) => setSearch(e.currentTarget.value)} /></label>
    <div class="table-scroll"><table><thead><tr><th>Component</th><th>What it is for</th><th>Minimum host need</th><th>Node status</th><th>Install / assign</th></tr></thead><tbody><For each={visible()}>{(item) => <tr><td><b>{item.name}</b><div class="muted small">{item.group}</div></td><td class="small">{item.description}</td><td class="small">{item.gpu ? 'NVIDIA GPU · ' : ''}{item.multi ? 'multi-GPU · ' : ''}{item.ram} GiB RAM · {item.disk} GiB disk</td><td>{item.gpu && !capable() ? <span class="badge warn"><i />GPU unavailable</span> : <span class="badge"><i />catalogued</span>}</td><td><button class="tiny" disabled title="A verified artifact source and reproducible image build are required before this is enabled.">Download & assign</button></td></tr>}</For></tbody></table></div>
  </section>;
}
