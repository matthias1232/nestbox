// Model endpoints: the local router, a cloud API, a vLLM on the overlay.
//
// The screen never displays or accepts a credential value, only the locator that
// points at one. A probe is the only way to learn whether the credential works,
// and it reports the answer rather than a generic failure.

import { For, Show, createSignal, onMount } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import * as api from '../../api/providers.js';

const KINDS = [
  { id: 'omniroute', label: 'OmniRoute router' },
  { id: 'openai_compatible', label: 'OpenAI-compatible server' },
  { id: 'openai', label: 'OpenAI' },
  { id: 'anthropic', label: 'Anthropic' },
  { id: 'nvidia', label: 'NVIDIA' },
  { id: 'local_vllm', label: 'Local vLLM' },
];

const blank = {
  name: '',
  kind: 'openai_compatible',
  endpoint: '',
  model: '',
  secret_ref: 'file:///secrets/',
};

export default function ProvidersPage() {
  const [items, setItems] = createSignal([]);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [probes, setProbes] = createSignal({});
  const [form, setForm] = createSignal({ ...blank });
  const [editing, setEditing] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  onMount(load);

  async function load() {
    setError(null);
    try {
      const result = await api.list();
      setItems(result.items || []);
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function submit(event) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    setBusy(true);
    const value = form();
    try {
      const created = await api.create({
        name: value.name.trim(),
        kind: value.kind,
        endpoint: value.endpoint.trim(),
        model: value.model.trim() || null,
        secret_ref: value.secret_ref.trim() || null,
      });
      setNotice(`Provider “${created.name}” registered.`);
      setForm({ ...blank });
      await load();
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  async function saveEdit(event) {
    event.preventDefault();
    const current = editing();
    if (!current) return;
    setError(null);
    setNotice(null);
    try {
      await api.update(current.id, {
        endpoint: current.endpoint.trim(),
        model: current.model?.trim() || null,
        secret_ref: current.secret_ref?.trim() || null,
        labels: current.labels || {},
        version: current.version,
      });
      setNotice(`Provider “${current.name}” updated.`);
      setEditing(null);
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function probe(profile) {
    setError(null);
    setNotice(null);
    try {
      const result = await api.probe(profile.id);
      setProbes({ ...probes(), [profile.id]: result });
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function destroy(profile) {
    if (!window.confirm(`Remove provider “${profile.name}”? A sandbox that already uses its endpoint keeps working; a new one will not get it.`)) {
      return;
    }
    setError(null);
    try {
      await api.remove(profile.id);
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Providers</h1>
          <p class="muted">
            {items().length} endpoints · the first OpenAI-compatible profile is used
            for new agent sandboxes
          </p>
        </div>
        <div class="controls">
          <button class="secondary" onClick={load}>Refresh</button>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <Show
        when={items().length > 0}
        fallback={<p class="muted">No provider profile yet. Register one below.</p>}
      >
        <div class="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Kind</th>
                <th>Endpoint</th>
                <th>Model</th>
                <th>Credential</th>
                <th>Probe</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={items()}>
                {(profile) => (
                  <tr>
                    <td class="mono">{profile.name}</td>
                    <td class="small">
                      {KINDS.find((kind) => kind.id === profile.kind)?.label || profile.kind}
                    </td>
                    <td class="mono small">{profile.endpoint}</td>
                    <td class="mono small">{profile.model || '—'}</td>
                    <td class="narrow">
                      <Show
                        when={profile.credential_configured}
                        fallback={<span class="badge muted"><i />missing</span>}
                      >
                        <span class="badge ok" title={profile.secret_ref}><i />configured</span>
                      </Show>
                    </td>
                    <td class="small">
                      <Show when={probes()[profile.id]} fallback={<span class="muted">not probed</span>}>
                        <span class={probes()[profile.id].reachable ? 'badge ok' : 'badge bad'}>
                          <i />{probes()[profile.id].reachable ? 'reachable' : 'unreachable'}
                        </span>
                        <div class="muted small">{probes()[profile.id].detail}</div>
                        <Show when={probes()[profile.id].model_count !== null}>
                          <div class="muted small">{probes()[profile.id].model_count} models</div>
                        </Show>
                      </Show>
                    </td>
                    <td class="actions">
                      <button class="tiny" onClick={() => probe(profile)}>Probe</button>
                      <button class="tiny" onClick={() => setEditing({ ...profile })}>Edit</button>
                      <button class="tiny danger" onClick={() => destroy(profile)}>Remove</button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>

      <Show when={editing()}>
        <form class="editor" onSubmit={saveEdit}>
          <h3>Edit {editing().name}</h3>
          <p class="muted small">
            Name and kind are fixed: other rows and the audit trail refer to them.
            To change either, remove the profile and register a new one.
          </p>
          <label class="field">
            <span>Endpoint</span>
            <input value={editing().endpoint}
                   onInput={(e) => setEditing({ ...editing(), endpoint: e.currentTarget.value })} />
          </label>
          <label class="field">
            <span>Default model</span>
            <input value={editing().model || ''}
                   onInput={(e) => setEditing({ ...editing(), model: e.currentTarget.value })} />
          </label>
          <label class="field">
            <span>Secret locator (never the value)</span>
            <input value={editing().secret_ref || ''}
                   onInput={(e) => setEditing({ ...editing(), secret_ref: e.currentTarget.value })} />
          </label>
          <div class="actions">
            <button type="submit">Save</button>
            <button type="button" class="ghost" onClick={() => setEditing(null)}>Cancel</button>
          </div>
        </form>
      </Show>

      <form class="editor" onSubmit={submit}>
        <h3>Register a provider</h3>
        <p class="muted small">
          A credential is a file on the host, mode 0600. This form records where it
          is, never what it is.
        </p>
        <div class="form-grid">
          <label class="field">
            <span>Name</span>
            <input value={form().name} required
                   onInput={(e) => setForm({ ...form(), name: e.currentTarget.value })} />
          </label>
          <label class="field">
            <span>Kind</span>
            <select value={form().kind}
                    onChange={(e) => setForm({ ...form(), kind: e.currentTarget.value })}>
              <For each={KINDS}>{(kind) => <option value={kind.id}>{kind.label}</option>}</For>
            </select>
          </label>
          <label class="field">
            <span>Endpoint</span>
            <input value={form().endpoint} required placeholder="http://host:port/v1"
                   onInput={(e) => setForm({ ...form(), endpoint: e.currentTarget.value })} />
          </label>
          <label class="field">
            <span>Default model</span>
            <input value={form().model}
                   onInput={(e) => setForm({ ...form(), model: e.currentTarget.value })} />
          </label>
          <label class="field">
            <span>Secret locator</span>
            <input value={form().secret_ref}
                   onInput={(e) => setForm({ ...form(), secret_ref: e.currentTarget.value })} />
          </label>
        </div>
        <div class="actions">
          <button type="submit" disabled={busy()}>{busy() ? 'Saving…' : 'Register'}</button>
        </div>
      </form>
    </section>
  );
}
