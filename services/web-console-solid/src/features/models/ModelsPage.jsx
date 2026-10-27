// The model catalog, classified against this machine.

import { For, Show, createSignal, onMount } from 'solid-js';
import FitBadge from '../../components/FitBadge.jsx';
import * as models from '../../api/models.js';

export default function ModelsPage() {
  const [items, setItems] = createSignal([]);
  const [hardware, setHardware] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  onMount(() => load(false));

  async function load(refresh) {
    setBusy(true);
    setError(null);
    try {
      const catalog = await models.catalog(refresh);
      setItems(catalog.items || []);
      setHardware(catalog.hardware || null);
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
      setNotice(`Install started as job ${result.job_id}.`);
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
      setNotice(`Uninstall started as job ${result.job_id}.`);
      await load(false);
    } catch (failure) {
      setError(failure.message);
    }
  }

  const gigabytes = (bytes) => (bytes ? `${Math.round(bytes / 1024 ** 3)} GiB` : '—');

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Models</h1>
          <p class="muted">What this machine can run, and what it cannot.</p>
        </div>
        <div class="controls">
          <button class="secondary" onClick={() => load(true)} disabled={busy()}>
            {busy() ? 'Probing…' : 'Probe hardware again'}
          </button>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <Show when={hardware()}>
        <div class="cards">
          <article>
            <span>GPUs</span>
            <b>{hardware().gpu_count || 0}</b>
            <small>{(hardware().gpus || []).map((gpu) => gpu.name).join(', ') || 'none detected'}</small>
          </article>
          <article>
            <span>Addressable memory</span>
            <b>{gigabytes(hardware().addressable_memory_bytes)}</b>
            <small>{hardware().unified_memory_bytes ? 'unified pool' : 'largest single card'}</small>
          </article>
          <article>
            <span>Detected</span>
            <b>{hardware().detected_at ? new Date(hardware().detected_at).toLocaleString() : '—'}</b>
            <small>{hardware().cuda_version ? `CUDA ${hardware().cuda_version}` : 'no driver reported'}</small>
          </article>
        </div>
        <Show when={hardware().note}>
          <p class="notice">{hardware().note}</p>
        </Show>
      </Show>

      <div class="table-scroll">
        <table>
          <thead>
            <tr>
              <th>Model</th><th>Parameters</th><th class="narrow">Context</th>
              <th class="narrow">Needs</th><th>Fit</th><th>Why</th><th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <For each={items()}>
              {(model) => (
                <tr>
                  <td>
                    <strong>{model.name}</strong>
                    <div class="muted small mono">{model.key}</div>
                  </td>
                  <td class="small">{model.parameters}</td>
                  <td class="narrow mono small">
                    {model.context_length ? `${Math.round(model.context_length / 1024)}K` : '—'}
                  </td>
                  <td class="narrow mono small">{gigabytes(model.min_vram_bytes)}</td>
                  <td><FitBadge fit={model.fit} /></td>
                  <td class="small muted">{model.fit?.reason}</td>
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
    </section>
  );
}
