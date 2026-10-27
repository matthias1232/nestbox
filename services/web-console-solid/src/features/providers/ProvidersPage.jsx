// Model endpoints: the local router, a cloud API, a vLLM on the overlay, and the
// server an installed model will run as.
//
// Three things this screen refuses to fake:
//   1. a credential value — only the locator is ever shown (ADR 0005);
//   2. a status — it is the server's answer from the last probe, and a profile
//      an install created is `pending`, meaning its model server is not started
//      yet, which is not the same as an endpoint that failed (ADR 0018);
//   3. a removal — a profile a sandbox still selects is refused with
//      409 `provider.in_use`, and the screen names those sandboxes instead of
//      reporting a generic failure.

import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import { ApiError } from '../../api/client.js';
import * as api from '../../api/providers.js';
import * as models from '../../api/models.js';
import '../../styles/providers-status.css';

// What an operator can register by hand. `local_openai_compatible` is absent on
// purpose: that kind is created by an install, never typed into the form.
const KINDS = [
  { id: 'omniroute', label: 'OmniRoute router' },
  { id: 'openai_compatible', label: 'OpenAI-compatible server' },
  { id: 'openai', label: 'OpenAI' },
  { id: 'anthropic', label: 'Anthropic' },
  { id: 'nvidia', label: 'NVIDIA' },
  { id: 'local_vllm', label: 'Local vLLM' },
];

const KIND_LABELS = {
  ...Object.fromEntries(KINDS.map((kind) => [kind.id, kind.label])),
  local_openai_compatible: 'Model server of an install',
};

// The meaning of each status, in the badge's tooltip and again as a second line.
// `pending` never says "offline": nothing has failed, a server has not started.
const STATUS_MEANING = {
  pending:
    'Created by an install whose model server is not started yet, so a new sandbox ' +
    'is not given this profile until a probe has seen it answer.',
  reachable: 'The endpoint answered the last probe.',
  unreachable: 'The endpoint did not answer the last probe. The probe detail says why.',
};

const STATUS_LINE = {
  pending: 'Model server not started yet (install phase 2)',
  reachable: 'Answered the last probe',
  unreachable: 'Did not answer the last probe',
};

const FILTERS = [
  { id: 'all', label: 'All' },
  { id: 'pending', label: 'Pending' },
  { id: 'reachable', label: 'Reachable' },
  { id: 'unreachable', label: 'Unreachable' },
];

const blank = {
  name: '',
  kind: 'openai_compatible',
  endpoint: '',
  model: '',
  secret_ref: 'file:///secrets/',
};

// A probe is recent enough that "3 min ago" is more useful than a timestamp.
function relative(iso) {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return 'at an unknown time';
  const minutes = Math.round((Date.now() - then) / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(iso).toLocaleString();
}

function shortId(id) {
  return typeof id === 'string' ? id.slice(0, 8) : 'unknown';
}

// The refusal names what holds the profile; the sentence has to agree with the
// count, or the list below it reads like a contradiction.
function holders(blocked) {
  const count = blocked.sandboxes.length;
  if (count > 1) return `${count} sandboxes still select it`;
  return 'a sandbox still selects it';
}

export default function ProvidersPage() {
  const [items, setItems] = createSignal([]);
  const [installs, setInstalls] = createSignal([]);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  // The last probe of this session, per profile. The server also persists the
  // verdict; this keeps the sentence next to the row that produced it.
  const [probes, setProbes] = createSignal({});
  const [probing, setProbing] = createSignal({});
  // A 409 provider.in_use with the sandboxes that still select the profile.
  const [blocked, setBlocked] = createSignal(null);
  const [status, setStatus] = createSignal('all');
  const [form, setForm] = createSignal({ ...blank });
  const [editing, setEditing] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  onMount(load);

  async function load() {
    setError(null);
    const [profiles, installList] = await Promise.allSettled([
      api.list(),
      models.installs(),
    ]);

    if (profiles.status === 'fulfilled') {
      setItems(profiles.value.items || []);
    } else {
      setError(profiles.reason.message);
    }

    // Naming the model and the node is a convenience: a reader without
    // `model.read` still sees the profile, with the install id in the tooltip.
    setInstalls(installList.status === 'fulfilled' ? installList.value?.items || [] : []);
  }

  // The status the row shows: the answer of a probe in this session if there is
  // one, otherwise what the server stored.
  const statusOf = (profile) => {
    const probe = probes()[profile.id];
    if (probe) return probe.reachable ? 'reachable' : 'unreachable';
    return profile.status || 'unknown';
  };

  const installOf = (profile) => {
    if (!profile.model_install_id) return null;
    return installs().find((install) => install.id === profile.model_install_id) || null;
  };

  const counts = createMemo(() => {
    const all = items();
    return {
      all: all.length,
      pending: all.filter((profile) => statusOf(profile) === 'pending').length,
      reachable: all.filter((profile) => statusOf(profile) === 'reachable').length,
      unreachable: all.filter((profile) => statusOf(profile) === 'unreachable').length,
    };
  });

  const visible = createMemo(() => {
    if (status() === 'all') return items();
    return items().filter((profile) => statusOf(profile) === status());
  });

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
    setBlocked(null);
    setProbing({ ...probing(), [profile.id]: true });
    try {
      const result = await api.probe(profile.id);
      setProbes({ ...probes(), [profile.id]: result });
      // The server stores the verdict and the time of the probe, so the row
      // reads the same after a reload as it does right now.
      await load();
    } catch (failure) {
      setError(failure.message);
    } finally {
      const next = { ...probing() };
      delete next[profile.id];
      setProbing(next);
    }
  }

  async function destroy(profile) {
    const answer = window.confirm(
      `Remove provider “${profile.name}”? If a sandbox still selects it the server ` +
        'refuses, because that sandbox would keep an endpoint nobody maintains.',
    );
    if (!answer) return;
    setError(null);
    setNotice(null);
    setBlocked(null);
    try {
      await api.remove(profile.id);
      setNotice(`Provider “${profile.name}” removed.`);
      await load();
    } catch (failure) {
      // 409 provider.in_use is an answer, not a malfunction: it names what holds
      // the profile. Everything else is a failure and keeps the plain message.
      if (failure instanceof ApiError && failure.code === 'provider.in_use') {
        const names = Array.isArray(failure.details?.sandboxes)
          ? failure.details.sandboxes.filter((name) => typeof name === 'string')
          : [];
        setBlocked({ name: profile.name, message: failure.message, sandboxes: names });
      } else {
        setError(failure.message);
      }
    }
  }

  return (
    <section class="page prov-page">
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

      <Show when={blocked()}>
        <div class="error prov-blocked">
          <p>
            <strong>{blocked().name}</strong> was not removed: {holders(blocked())}, and that
            workload would keep an endpoint nobody maintains.
          </p>
          <Show
            when={blocked().sandboxes.length > 0}
            fallback={<p class="small">{blocked().message}</p>}
          >
            <p class="small">Sandboxes that select it:</p>
            <ul class="plain prov-blocked-names">
              <For each={blocked().sandboxes}>
                {(name) => <li class="mono small">{name}</li>}
              </For>
            </ul>
          </Show>
          <p class="small">
            Point those sandboxes at another provider, then remove this profile again.
          </p>
          <button class="tiny ghost" onClick={() => setBlocked(null)}>Dismiss</button>
        </div>
      </Show>

      <Show
        when={items().length > 0}
        fallback={<p class="muted">No provider profile yet. Register one below.</p>}
      >
        <div class="facets prov-filters">
          <For each={FILTERS}>
            {(filter) => (
              <button
                class={`chip ${status() === filter.id ? 'active' : ''}`}
                aria-pressed={status() === filter.id}
                onClick={() => setStatus(filter.id)}
              >
                {filter.label} <b>{counts()[filter.id]}</b>
              </button>
            )}
          </For>
        </div>

        <div class="table-scroll">
          <table class="prov-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Kind</th>
                <th>Endpoint</th>
                <th>Model</th>
                <th>Credential</th>
                <th>Status</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={visible()}>
                {(profile) => (
                  <tr>
                    <td>
                      <div class="mono">{profile.name}</div>
                      <Show when={profile.model_install_id}>
                        <div class="small prov-origin">
                          <Show
                            when={installOf(profile)}
                            fallback={
                              <span title={profile.model_install_id}>
                                from install <span class="mono">{shortId(profile.model_install_id)}</span>
                              </span>
                            }
                          >
                            {(install) => (
                              <>
                                from install of{' '}
                                <span
                                  class="prov-ref"
                                  title={`Model ${install().model_key} · install ${install().id}`}
                                >
                                  {install().model_name}
                                </span>{' '}
                                on{' '}
                                <span
                                  class="prov-ref"
                                  title={`Node ${install().node_id}`}
                                >
                                  {install().node_name}
                                </span>
                              </>
                            )}
                          </Show>
                        </div>
                      </Show>
                    </td>
                    <td class="small">{KIND_LABELS[profile.kind] || profile.kind}</td>
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
                    <td class="prov-status">
                      <StatusBadge
                        status={statusOf(profile)}
                        title={STATUS_MEANING[statusOf(profile)] || 'The server reported no status.'}
                      />
                      <div class="muted small prov-line">
                        {STATUS_LINE[statusOf(profile)] || 'No status reported'}
                      </div>
                      <Show when={probes()[profile.id]}>
                        {(result) => (
                          <div class="prov-detail">
                            <div class={`small ${result().reachable ? '' : 'bad-text'}`}>
                              {result().detail}
                            </div>
                            <div class="muted small">
                              {result().model_count === null || result().model_count === undefined
                                ? ''
                                : `${result().model_count} models · `}
                              {result().status ? `HTTP ${result().status}` : 'no HTTP answer'}
                            </div>
                          </div>
                        )}
                      </Show>
                      <Show when={!probes()[profile.id] && profile.last_probed_at}>
                        <div class="muted small prov-line">
                          Last probed {relative(profile.last_probed_at)}
                        </div>
                      </Show>
                    </td>
                    <td class="actions">
                      <button
                        class="tiny"
                        disabled={Boolean(probing()[profile.id])}
                        onClick={() => probe(profile)}
                      >
                        {probing()[profile.id] ? 'Probing…' : 'Probe'}
                      </button>
                      <button class="tiny" onClick={() => setEditing({ ...profile })}>Edit</button>
                      <button class="tiny danger" onClick={() => destroy(profile)}>Remove</button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>

        <Show when={visible().length === 0}>
          <p class="muted">
            No provider has the status “{status()}”. Pick another filter.
          </p>
        </Show>
      </Show>

      <Show when={editing()}>
        <form class="editor" onSubmit={saveEdit}>
          <h3>Edit {editing().name}</h3>
          <p class="muted small">
            Name and kind are fixed: other rows and the audit trail refer to them.
            To change either, remove the profile and register a new one.
          </p>
          <Show when={editing().model_install_id}>
            <p class="warn-note small">
              This profile belongs to a model install. Its endpoint is derived from the
              install and the node; changing it here separates the two.
            </p>
          </Show>
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
