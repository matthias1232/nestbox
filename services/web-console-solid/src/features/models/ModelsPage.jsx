// The model catalog, classified against this machine.
//
// The screen answers three questions in order: what does this machine have, what
// does each model need, and where is the gap. Every verdict is the server's; the
// filters only switch verdicts on and off. Install reserves memory and starts a
// server; the install list shows which of the two the node is doing, because a
// reservation that never becomes an answer is the failure worth seeing.

import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import CapacityOverview from '../../components/CapacityOverview.jsx';
import * as nodeApi from '../../api/nodes.js';
import ModelFitCell from '../../components/ModelFitCell.jsx';
import StatusBadge from '../../components/StatusBadge.jsx';
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
  // The node the catalogue is classified against. `local` until the list of
  // nodes says otherwise: a page that opened on "no node" would show nothing
  // and explain nothing.
  const [nodes, setNodes] = createSignal([]);
  const [nodeName, setNodeName] = createSignal('local');
  const [budget, setBudget] = createSignal(null);
  const [installs, setInstalls] = createSignal([]);
  const [syncing, setSyncing] = createSignal(false);
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

  onMount(async () => {
    try {
      const answer = await nodeApi.list();
      const list = answer.items || [];
      setNodes(list);
      const local = list.find((node) => node.is_local) || list[0];
      if (local) setNodeName(local.name);
    } catch {
      // A console that cannot list nodes still shows the catalogue, which the
      // server classifies against the local node when no node is named.
    }
    await load(false);
  });

  async function load(refresh) {
    setBusy(true);
    setError(null);
    try {
      const catalog = await models.catalog(refresh, {}, nodeName());
      setItems(catalog.items || []);
      setHardware(catalog.hardware || null);
      setBudget(catalog.budget || null);
      try {
        const answer = await models.installs(nodeName());
        setInstalls(answer.items || []);
      } catch {
        setInstalls([]);
      }
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

  async function syncCatalog() {
    setSyncing(true);
    setError(null);
    setNotice(null);
    try {
      const result = await models.sync();
      const added = (result.sources || []).reduce((sum, source) => sum + source.added, 0);
      const updated = (result.sources || []).reduce((sum, source) => sum + source.updated, 0);
      setNotice(
        `Catalogue synced: ${added} added, ${updated} updated from ` +
        `${(result.sources || []).map((source) => source.source).join(', ') || 'no source'}. ` +
        `Job ${result.job_id}.`,
      );
      await load(false);
    } catch (failure) {
      setError(failure.message);
    } finally {
      setSyncing(false);
    }
  }

  // A model that the verdict says will not fit is still installable: the
  // server answers with the reason, and this screen offers the same request
  // with `force` instead of hiding the button. The reservation is real either
  // way, and the install row records the warning.
  const [forceCandidate, setForceCandidate] = createSignal(null);

  async function install(model, force = false) {
    setNotice(null);
    setError(null);
    setForceCandidate(null);
    try {
      const result = await models.install(model.key, nodeName(), force);
      setNotice(
        `${model.name}: ${gib(result.reserved_ram_bytes || 0)} reserved on ${result.node}` +
        (result.reserved_vram_bytes ? ` and ${gib(result.reserved_vram_bytes)} of VRAM` : '') +
        `. Job ${result.job_id}.` +
        (result.warnings?.length ? ` ${result.warnings.length} warning(s) recorded.` : ''),
      );
      await load(false);
    } catch (failure) {
      if (failure.code === 'node.insufficient_resources' || failure.code === 'model.not_installable') {
        setForceCandidate(model);
      }
      setError(failure.message);
    }
  }

  // The RAM figure being typed per install, in GiB. Kept as text so a half-typed
  // "4." is not rewritten under the cursor.
  const [ramDraft, setRamDraft] = createSignal({});

  async function change(row, patch, label) {
    setNotice(null);
    setError(null);
    try {
      const result = await models.updateInstall(row.model_key, { node: nodeName(), ...patch });
      setNotice(`${row.model_name || row.model_key}: ${label}. ${result.warning || ''}`.trim());
      setRamDraft({ ...ramDraft(), [row.model_key]: undefined });
      await load(false);
    } catch (failure) {
      setError(failure.message);
    }
  }

  function saveRam(row) {
    const text = ramDraft()[row.model_key];
    const value = Number.parseFloat(text);
    if (!Number.isFinite(value) || value <= 0) {
      setError('Reserved RAM is a number of GiB greater than zero.');
      return;
    }
    return change(row, { reserved_ram_bytes: Math.round(value * 1024 * 1024 * 1024) },
      `reserved RAM set to ${value} GiB`);
  }

  async function uninstall(model) {
    setNotice(null);
    setError(null);
    try {
      const result = await models.uninstall(model.key, nodeName());
      setNotice(
        `${model.name} removed from ${result.node}; ${gib(result.released_ram_bytes || 0)} of RAM released.`,
      );
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
    const matched = items().filter((m) => {
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
    // What this machine can run comes first: the point of the screen is to make
    // the installable rows stand out, not to bury them under 400B models.
    return matched.sort((a, b) => {
      const rank = (m) => (FITS.has(m.fit?.fit) ? 0 : 1);
      return rank(a) - rank(b) || a.name.localeCompare(b.name);
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
          <select
            aria-label="Node"
            value={nodeName()}
            onChange={(event) => {
              setNodeName(event.currentTarget.value);
              queueMicrotask(() => load(false));
            }}
          >
            <For each={nodes()}>
              {(node) => <option value={node.name}>{node.display_name || node.name}</option>}
            </For>
          </select>
          <button onClick={syncCatalog} disabled={syncing()}>
            {syncing() ? 'Syncing…' : 'Sync catalog'}
          </button>
          <button class="primary" onClick={() => load(true)} disabled={busy()}>
            {busy() ? 'Probing…' : 'Probe hardware again'}
          </button>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={forceCandidate()}>
        <p class="notice">
          {forceCandidate().name} does not fit this node as it is. Installing it anyway reserves
          the memory and records the warning; the server will start and may fail at run time.
          <button class="tiny" onClick={() => install(forceCandidate(), true)}>Install anyway</button>
        </p>
      </Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <Show when={hardware()}>
        <CapacityOverview
          hardware={hardware()}
          items={items()}
          ramReference={ramReference()}
          vramReference={vramReference()}
        />
      </Show>

      <Show when={budget()}>
        <p class="muted">
          On <strong>{nodeName()}</strong>: {gib(budget().ram_total_bytes)} of RAM, of which{' '}
          <strong>{gib(budget().ram_reserved_bytes)} reserved</strong> by{' '}
          {budget().installs} {budget().installs === 1 ? 'install' : 'installs'} and{' '}
          {budget().ram_free_bytes ? gib(budget().ram_free_bytes) : '0 GiB'} free.
          {budget().vram_total_bytes
            ? ` VRAM: ${gib(budget().vram_reserved_bytes)} of ${gib(budget().vram_total_bytes)} reserved.`
            : ''}
        </p>
      </Show>

      <Show when={installs().length}>
        <details class="installs" open>
          <summary>
            {installs().length} on {nodeName()}
            {' '}
            <button class="secondary" onClick={() => load(false)} disabled={busy()}>
              Refresh
            </button>
          </summary>
          <table class="model-table">
            <thead>
              <tr><th>Model</th><th>State</th><th>Reserved RAM</th><th>Endpoint</th><th>Active</th></tr>
            </thead>
            <tbody>
              <For each={installs()}>
                {(row) => (
                  <tr>
                    <td>{row.model_name || row.model_key}</td>
                    <td>
                      <StatusBadge status={row.state} />
                      <Show when={row.state === 'installing'}>
                        <span class="muted small"> — reserved, the job has not started yet</span>
                      </Show>
                      <Show when={row.state === 'downloading'}>
                        <span class="muted small"> — the node is fetching the artefact</span>
                      </Show>
                      <Show when={row.state === 'starting'}>
                        <span class="muted small"> — server up, loading the weights</span>
                      </Show>
                      <Show when={row.state === 'failed' && row.error}>
                        <div class="muted small">{row.error}</div>
                      </Show>
                    </td>
                    <td>
                      <Show when={row.state === 'installed'} fallback={<>{gib(row.reserved_ram_bytes || 0)}</>}>
                        {(() => {
                          const GIB = 1073741824;
                          const min = Math.max(0.1, row.min_ram_bytes / GIB);
                          const max = Math.max(min, row.max_ram_bytes / GIB);
                          const value = () => Number(
                            ramDraft()[row.model_key] ?? (row.reserved_ram_bytes / GIB));
                          return (
                            <div class="ram-slider">
                              <input
                                type="range" min={min} max={max} step="0.1"
                                value={Math.min(max, Math.max(min, value()))}
                                onInput={(event) =>
                                  setRamDraft({ ...ramDraft(), [row.model_key]: event.currentTarget.value })}
                              />
                              <div class="muted small">
                                <b>{value().toFixed(1)} GiB</b>
                                {' '}· min {min.toFixed(1)} (needed to load)
                                <Show when={row.recommended_ram_bytes}>
                                  {' '}· recommended {(row.recommended_ram_bytes / GIB).toFixed(1)}
                                </Show>
                                {' '}· max {max.toFixed(1)}
                              </div>
                              <button class="secondary"
                                      disabled={ramDraft()[row.model_key] === undefined}
                                      onClick={() => saveRam(row)}>
                                Apply
                              </button>
                            </div>
                          );
                        })()}
                        <Show when={!row.enabled}>
                          <div class="muted small">off: counts once switched on</div>
                        </Show>
                      </Show>
                    </td>
                    <td class="muted small">
                      {row.state === 'installed' && row.enabled
                        ? 'answering on the runtime network'
                        : row.state === 'installed' ? 'off, memory free' : '—'}
                    </td>
                    <td>
                      <Show when={row.state === 'installed'}>
                        <label class="switch">
                          <input type="checkbox" checked={row.enabled}
                                 onChange={(event) =>
                                   change(row, { enabled: event.currentTarget.checked },
                                     event.currentTarget.checked ? 'switching on' : 'switched off')} />
                          {row.enabled ? 'On' : 'Off'}
                        </label>
                      </Show>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </details>
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
                            disabled={model.fit?.fit === 'already_installed'}
                            title={model.fit?.fit?.match(/installable|cpu_installable|update_available/)
                              ? 'Install on this node'
                              : 'Install anyway; the verdict and its warnings are recorded'}
                            onClick={() => install(model, !model.fit?.fit?.match(/installable|cpu_installable|update_available/))}>
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
