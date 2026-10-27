// The model catalog, classified against this machine.
//
// The screen answers three questions in order: what does this machine have, what
// does each model need, and where is the gap. Every verdict is the server's; the
// filters only switch verdicts on and off. Nothing is downloaded yet — the
// install button creates a job, and the UI says so rather than implying
// progress that does not exist.

import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import CapacityOverview from '../../components/CapacityOverview.jsx';
import ModelFitCell from '../../components/ModelFitCell.jsx';
import { gib } from '../../components/ResourceBars.jsx';
import * as models from '../../api/models.js';

// The verdicts that mean "this machine can run it". Same list the server's
// `Fit::fits` returns; kept here only to count and filter, never to decide.
const FITS = new Set([
  'installable',
  'cpu_installable',
  'already_installed',
  'update_available',
]);

export default function ModelsPage() {
  const [items, setItems] = createSignal([]);
  const [hardware, setHardware] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  const [search, setSearch] = createSignal('');
  const [scope, setScope] = createSignal('all');
  const [family, setFamily] = createSignal('all');
  const [publisher, setPublisher] = createSignal('all');
  const [runtime, setRuntime] = createSignal('all');
  const [cpuOnly, setCpuOnly] = createSignal(false);
  const [unifiedOnly, setUnifiedOnly] = createSignal(false);

  onMount(() => load(false));

  async function load(refresh) {
    setBusy(true);
    setError(null);
    try {
      const catalog = await models.catalog(refresh);
      setItems(catalog.items || []);
      setHardware(catalog.hardware || null);
      if (refresh) {
        const fitting = (catalog.items || []).filter((m) => FITS.has(m.fit?.fit)).length;
        setScope('fits');
        setNotice(
          `Hardware probed. ${fitting} of ${(catalog.items || []).length} models fit this ` +
          'machine; the filter now shows only those.',
        );
      }
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  async function install(model) {
    setNotice(null);
    setError(null);
    try {
      const result = await models.install(model.key);
      setNotice(`Install job ${result.job_id} created for ${model.name}.`);
      await load(false);
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function uninstall(model) {
    setNotice(null);
    setError(null);
    try {
      const result = await models.uninstall(model.key);
      setNotice(`Uninstall job ${result.job_id} created for ${model.name}.`);
      await load(false);
    } catch (failure) {
      setError(failure.message);
    }
  }

  const families = createMemo(() =>
    [...new Set(items().map((m) => m.family).filter(Boolean))].sort());
  const publishers = createMemo(() =>
    [...new Set(items().map((m) => m.publisher).filter(Boolean))].sort());

  const counts = createMemo(() => {
    const all = items();
    const local = all.filter((m) => !m.is_remote);
    const fitting = local.filter((m) => FITS.has(m.fit?.fit));
    return { total: all.length, fits: fitting.length, notFits: local.length - fitting.length };
  });

  // The capacity bars are scaled against the largest requirement among the
  // models that fit, taken from the server's own bars so the scale cannot
  // disagree with a verdict.
  const reference = (resources) => {
    let max = 0;
    for (const model of items()) {
      if (!FITS.has(model.fit?.fit)) continue;
      for (const bar of model.fit.bars || []) {
        if (!resources.includes(bar.resource)) continue;
        const need = bar.recommended || bar.minimum || 0;
        if (need > max) max = need;
      }
    }
    return max;
  };
  const ramReference = createMemo(() => reference(['ram']));
  const vramReference = createMemo(() => reference(['vram', 'unified_memory']));

  const visible = createMemo(() => {
    const needle = search().trim().toLowerCase();
    return items().filter((m) => {
      if (scope() === 'fits' && !FITS.has(m.fit?.fit)) return false;
      if (scope() === 'not' && (m.is_remote || FITS.has(m.fit?.fit))) return false;
      if (family() !== 'all' && m.family !== family()) return false;
      if (publisher() !== 'all' && m.publisher !== publisher()) return false;
      if (runtime() !== 'all' && m.runtime !== runtime()) return false;
      if (cpuOnly() && !m.is_cpu_only) return false;
      if (unifiedOnly() && !m.unified_memory_only) return false;
      if (!needle) return true;
      return `${m.name} ${m.key} ${m.publisher || ''} ${m.parameters || ''}`
        .toLowerCase()
        .includes(needle);
    });
  });

  const needs = (model) => {
    const parts = [];
    if (model.min_vram_bytes) parts.push(`${gib(model.min_vram_bytes)} GPU`);
    if (model.min_ram_bytes) parts.push(`${gib(model.min_ram_bytes)} RAM`);
    if (model.min_cpu_cores) parts.push(`${model.min_cpu_cores} cores`);
    if (model.min_gpu_count > 1) parts.push(`${model.min_gpu_count} GPUs`);
    return parts.length ? parts.join(' · ') : 'no hard gate';
  };

  const recommended = (model) => {
    const parts = [];
    if (model.recommended_vram_bytes) parts.push(`${gib(model.recommended_vram_bytes)} GPU`);
    if (model.recommended_ram_bytes) parts.push(`${gib(model.recommended_ram_bytes)} RAM`);
    if (model.recommended_cpu_cores) parts.push(`${model.recommended_cpu_cores} cores`);
    return parts.length ? parts.join(' · ') : null;
  };

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Models</h1>
          <p class="muted">
            What this machine can run, what each model needs, and how far apart the two are.
          </p>
        </div>
        <div class="controls">
          <button class="primary" onClick={() => load(true)} disabled={busy()}>
            {busy() ? 'Probing…' : 'Probe hardware again'}
          </button>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <Show when={hardware()}>
        <CapacityOverview
          hardware={hardware()}
          items={items()}
          ramReference={ramReference()}
          vramReference={vramReference()}
        />
      </Show>

      <div class="filters">
        <input
          placeholder="Search model, key, publisher…"
          value={search()}
          onInput={(event) => setSearch(event.currentTarget.value)}
        />
        <div class="facet-row">
          <button class={`chip ${scope() === 'all' ? 'active' : ''}`}
                  onClick={() => setScope('all')}>
            All <b>{counts().total}</b>
          </button>
          <button class={`chip ${scope() === 'fits' ? 'active' : ''}`}
                  onClick={() => setScope('fits')}>
            Fits this machine <b>{counts().fits}</b>
          </button>
          <button class={`chip ${scope() === 'not' ? 'active' : ''}`}
                  onClick={() => setScope('not')}>
            Does not fit <b>{counts().notFits}</b>
          </button>
          <button class={`chip ${cpuOnly() ? 'active' : ''}`}
                  onClick={() => setCpuOnly(!cpuOnly())}>
            Runs without a GPU
          </button>
          <button class={`chip ${unifiedOnly() ? 'active' : ''}`}
                  onClick={() => setUnifiedOnly(!unifiedOnly())}>
            Needs unified memory
          </button>
        </div>
        <div class="facet-row">
          <select value={family()} onChange={(event) => setFamily(event.currentTarget.value)}>
            <option value="all">All families</option>
            <For each={families()}>{(value) => <option value={value}>{value}</option>}</For>
          </select>
          <select value={publisher()} onChange={(event) => setPublisher(event.currentTarget.value)}>
            <option value="all">All publishers</option>
            <For each={publishers()}>{(value) => <option value={value}>{value}</option>}</For>
          </select>
          <select value={runtime()} onChange={(event) => setRuntime(event.currentTarget.value)}>
            <option value="all">Any runtime</option>
            <option value="cuda">GPU only</option>
            <option value="hybrid">GPU or CPU</option>
            <option value="cpu">CPU only</option>
          </select>
          <span class="muted small">{visible().length} shown</span>
        </div>
      </div>

      <div class="table-scroll">
        <table class="model-table">
          <thead>
            <tr>
              <th>Model</th>
              <th>Size and format</th>
              <th>Minimum / recommended</th>
              <th>Fit</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <For each={visible()}>
              {(model) => (
                <tr class={FITS.has(model.fit?.fit) ? 'fit-yes' : ''}>
                  <td>
                    <strong>{model.name}</strong>
                    <Show when={model.is_remote}>
                      <span class="badge info tiny-badge"><i />Cloud</span>
                    </Show>
                    <div class="muted small mono">{model.key}</div>
                    <div class="muted small">
                      {model.publisher || 'unknown publisher'}
                      {model.license ? ` · ${model.license}` : ''}
                    </div>
                  </td>
                  <td class="small">
                    <div>{model.parameters}</div>
                    <div class="muted">
                      {model.context_length
                        ? `${Math.round(model.context_length / 1024)}K context`
                        : 'context unknown'}
                      {model.quantization ? ` · ${model.quantization}` : ''}
                    </div>
                    <div class="muted">
                      <span class="tag">{model.runtime}</span>
                      {model.arch ? <span class="tag">{model.arch}</span> : null}
                    </div>
                  </td>
                  <td class="small">
                    <div class="mono">{needs(model)}</div>
                    <Show when={recommended(model)}>
                      <div class="muted mono">{recommended(model)}</div>
                    </Show>
                  </td>
                  <td class="fit-column">
                    <ModelFitCell fit={model.fit} />
                    <div class="muted small">{model.fit?.reason}</div>
                  </td>
                  <td class="actions">
                    <button class="tiny"
                            disabled={!model.fit?.fit?.match(/installable|update_available/)}
                            onClick={() => install(model)}>
                      {model.fit?.fit === 'update_available' ? 'Update' : 'Install'}
                    </button>
                    <button class="tiny danger"
                            disabled={!model.fit?.fit?.match(/already_installed|update_available/)}
                            onClick={() => uninstall(model)}>
                      Remove
                    </button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>

      <Show when={visible().length === 0}>
        <p class="muted">No model matches these filters. Clear one and try again.</p>
      </Show>

      <p class="muted small phase-note">
        Install creates a job and records the intent. The artefact download is Phase 2:
        no bytes are fetched yet, so a job does not make a model usable on its own.
      </p>
    </section>
  );
}
