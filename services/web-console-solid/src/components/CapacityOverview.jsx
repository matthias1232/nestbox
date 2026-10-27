// The machine's own capacity, as an overview.
//
// Every figure is measured (or explicitly unknown). The bar compares the
// machine with the largest requirement in the catalog that is currently loaded,
// so "16 GiB of RAM" gets a scale instead of being a bare number. A bar that
// fills completely means "this machine could host the biggest model listed",
// not "nothing is left over".

import { For, Show } from 'solid-js';
import { amount, gib } from './ResourceBars.jsx';

function tallest(items, pick) {
  return items.reduce((max, item) => {
    const value = pick(item);
    return value != null && value > max ? value : max;
  }, 0);
}

export default function CapacityOverview(props) {
  const hardware = () => props.hardware || {};
  const items = () => props.items || [];

  const maxRamNeed = () =>
    tallest(items(), (m) => Math.max(m.min_ram_bytes || 0, m.recommended_ram_bytes || 0));
  const maxVramNeed = () =>
    tallest(items(), (m) => Math.max(m.min_vram_bytes || 0, m.recommended_vram_bytes || 0));

  const ram = () => hardware().system_ram_bytes || hardware().unified_memory_bytes || 0;
  const vram = () =>
    hardware().is_unified_memory
      ? hardware().unified_memory_bytes || 0
      : hardware().addressable_memory_bytes || 0;

  const share = (have, need) => {
    if (!need) return 0;
    return Math.min(100, (have / need) * 100);
  };

  return (
    <section class="capacity">
      <div class="capacity-head">
        <div>
          <h2>This machine</h2>
          <p class="muted small">
            {hardware().hostname || 'host unknown'}
            {hardware().detected_at
              ? ` · probed ${new Date(hardware().detected_at).toLocaleString()}`
              : ''}
          </p>
        </div>
        <div class="capacity-tags">
          <span class={`badge ${hardware().has_gpu ? 'ok' : 'muted'}`}>
            <i />{hardware().has_gpu ? 'GPU available' : 'no GPU'}
          </span>
          <Show when={hardware().is_unified_memory}>
            <span class="badge info"><i />unified memory</span>
          </Show>
          <Show when={hardware().cuda_version}>
            <span class="badge muted"><i />CUDA {hardware().cuda_version}</span>
          </Show>
        </div>
      </div>

      <div class="capacity-grid">
        <article>
          <span>CPU</span>
          <b>{hardware().cpu_model || 'not detected'}</b>
          <small>
            {hardware().cpu_cores ? `${hardware().cpu_cores} cores` : 'cores unknown'}
            {hardware().cpu_threads ? ` · ${hardware().cpu_threads} threads` : ''}
          </small>
        </article>

        <article>
          <span>System RAM</span>
          <b>{gib(hardware().system_ram_bytes)}</b>
          <small>MemTotal as the kernel reports it</small>
          <div class="capacity-bar" title={
            `have ${gib(ram())}, largest catalog requirement ${gib(maxRamNeed())}`
          }>
            <div style={{ width: `${share(ram(), maxRamNeed())}%` }} />
          </div>
          <small class="muted">
            {maxRamNeed()
              ? `${Math.round(share(ram(), maxRamNeed()))}% of the largest requirement (${gib(maxRamNeed())})`
              : 'no requirement loaded'}
          </small>
        </article>

        <article>
          <span>{hardware().is_unified_memory ? 'Unified memory pool' : 'GPU memory'}</span>
          <b>{gib(vram())}</b>
          <small>
            {hardware().is_unified_memory
              ? 'shared by CPU and GPU'
              : `${hardware().gpu_count || 0} device(s), largest single card counts`}
          </small>
          <div class="capacity-bar" title={
            `have ${gib(vram())}, largest catalog requirement ${gib(maxVramNeed())}`
          }>
            <div style={{ width: `${share(vram(), maxVramNeed())}%` }} />
          </div>
          <small class="muted">
            {maxVramNeed()
              ? `${Math.round(share(vram(), maxVramNeed()))}% of the largest requirement (${gib(maxVramNeed())})`
              : 'no requirement loaded'}
          </small>
        </article>
      </div>

      <Show when={(hardware().gpus || []).length > 0}>
        <div class="capacity-gpus">
          <For each={hardware().gpus}>
            {(gpu) => (
              <span class="gpu-chip mono small">
                #{gpu.index} {gpu.name} · {amount('vram', gpu.memory_bytes)}
                {gpu.free_bytes != null ? ` · ${gib(gpu.free_bytes)} free` : ''}
              </span>
            )}
          </For>
        </div>
      </Show>

      <Show when={hardware().unified_memory_source}>
        <p class="muted small">{hardware().unified_memory_source}</p>
      </Show>
      <Show when={hardware().note}>
        <p class="notice small">{hardware().note}</p>
      </Show>
    </section>
  );
}
