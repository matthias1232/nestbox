// The machine's own capacity, as an overview.
//
// The per-resource bars are scaled against the largest requirement among the
// models that fit this machine, and the page passes that reference in. Scaling
// against the biggest catalog entry instead would draw a 7 GiB appliance as a
// 1 % sliver and teach nobody anything; scaling against "the largest model you
// can actually run" shows the headroom that is left, which is the useful number.

import { For, Show } from 'solid-js';
import { amount, gib } from './ResourceBars.jsx';

function share(have, need) {
  if (!need) return 0;
  return Math.min(100, (have / need) * 100);
}

function CapacityBar(props) {
  const have = () => props.have || 0;
  const need = () => props.need || 0;
  return (
    <Show when={need() > 0} fallback={<small class="muted">nothing fits here yet</small>}>
      <div class="capacity-bar" title={`have ${gib(have())}, reference ${gib(need())}`}>
        <div style={{ width: `${share(have(), need())}%` }} />
      </div>
      <small class="muted">
        {Math.round(share(have(), need()))}% of the largest fitting requirement
        ({gib(need())})
      </small>
    </Show>
  );
}

export default function CapacityOverview(props) {
  const hardware = () => props.hardware || {};

  const ram = () => hardware().system_ram_bytes || hardware().unified_memory_bytes || 0;
  const vram = () =>
    hardware().is_unified_memory
      ? hardware().unified_memory_bytes || 0
      : hardware().addressable_memory_bytes || 0;

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
          <CapacityBar have={ram()} need={props.ramReference} />
        </article>

        <article>
          <span>{hardware().is_unified_memory ? 'Unified memory pool' : 'GPU memory'}</span>
          <b>{gib(vram())}</b>
          <small>
            {hardware().is_unified_memory
              ? 'shared by CPU and GPU'
              : `${hardware().gpu_count || 0} device(s), largest single card counts`}
          </small>
          <CapacityBar have={vram()} need={props.vramReference} />
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
