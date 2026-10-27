// The sandbox drawer.
//
// Shows the stored state and the observed state next to each other, because the
// interesting case is when they disagree.

import { Show, For, createSignal, createEffect } from 'solid-js';
import Drawer from '../../components/Drawer.jsx';
import Tabs from '../../components/Tabs.jsx';
import StatusBadge from '../../components/StatusBadge.jsx';
import * as api from '../../api/sandboxes.js';

const TABS = [
  { id: 'overview', label: 'Overview' },
  { id: 'logs', label: 'Logs' },
  { id: 'rules', label: 'Rules' },
  { id: 'inference', label: 'Model requests' },
];

export default function SandboxDetail(props) {
  const [tab, setTab] = createSignal('overview');
  const [detail, setDetail] = createSignal(null);
  const [logs, setLogs] = createSignal('');
  const [rules, setRules] = createSignal([]);
  const [inference, setInference] = createSignal([]);
  const [error, setError] = createSignal(null);
  const [loading, setLoading] = createSignal(false);

  createEffect(() => {
    const id = props.sandboxId;
    if (!id) {
      setDetail(null);
      setLogs('');
      setRules([]);
      setInference([]);
      setError(null);
      return;
    }
    setTab('overview');
    load(id);
  });

  async function load(id) {
    setLoading(true);
    setError(null);
    try {
      setDetail(await api.get(id));
    } catch (failure) {
      setError(failure.message);
    } finally {
      setLoading(false);
    }
  }

  async function loadTab(next) {
    setTab(next);
    const id = props.sandboxId;
    if (!id) return;
    setError(null);
    try {
      if (next === 'logs') setLogs((await api.logs(id, 400)).content || '');
      if (next === 'rules') setRules((await api.rules(id)).items || []);
      if (next === 'inference') setInference((await api.inference(id, 100)).items || []);
    } catch (failure) {
      setError(failure.message);
    }
  }

  const sandbox = () => detail()?.sandbox || null;
  const runtime = () => detail()?.runtime || null;

  return (
    <Drawer
      open={!!props.sandboxId}
      title={sandbox()?.name || 'Sandbox'}
      subtitle={sandbox() ? `${sandbox().pack_key} · ${sandbox().image}` : 'loading…'}
      onClose={props.onClose}
    >
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>

      <Tabs tabs={TABS} active={tab()} onChange={loadTab} />

      <Show when={tab() === 'overview'}>
        <Show when={sandbox()} fallback={<p class="muted">Loading…</p>}>
          <dl class="facts">
            <dt>Status</dt><dd><StatusBadge status={sandbox().status} /></dd>
            <dt>Health</dt><dd><StatusBadge status={sandbox().health} /></dd>
            <dt>Version</dt><dd class="mono">{sandbox().version}</dd>
            <dt>Image</dt><dd class="mono small">{sandbox().image}</dd>
            <dt>Digest</dt><dd class="mono small">{sandbox().image_digest || 'not pinned'}</dd>
            <dt>Created</dt><dd class="mono small">{sandbox().created_at}</dd>
            <dt>Updated</dt><dd class="mono small">{sandbox().updated_at}</dd>
          </dl>

          <h3>Runtime</h3>
          <Show
            when={runtime()}
            fallback={
              <p class="muted">
                The runtime did not answer. The stored state is shown above; live
                address, uptime, and resources are unavailable.
              </p>
            }
          >
            <dl class="facts">
              <dt>Container</dt><dd class="mono small">{runtime().container_name || '—'}</dd>
              <dt>Address</dt><dd class="mono">{runtime().ip || '—'}</dd>
              <dt>Gateway</dt><dd class="mono">{runtime().gateway || '—'}</dd>
              <dt>Uptime</dt><dd class="mono">{formatUptime(runtime().uptime_seconds)}</dd>
              <dt>Runtime state</dt><dd><StatusBadge status={runtime().status} /></dd>
              <dt>Exit code</dt><dd class="mono">{runtime().exit_code ?? '—'}</dd>
              <dt>Memory limit</dt><dd class="mono">{formatBytes(runtime().resources?.memory_limit)}</dd>
              <dt>CPU</dt><dd class="mono">{runtime().resources?.nano_cpus ? (runtime().resources.nano_cpus / 1e9).toFixed(2) : '—'}</dd>
              <dt>PIDs limit</dt><dd class="mono">{runtime().resources?.pids_limit ?? '—'}</dd>
            </dl>

            <h3>Interfaces</h3>
            <Show when={(runtime().network || []).length > 0} fallback={<p class="muted">No interface reported.</p>}>
              <ul class="plain">
                <For each={runtime().network}>
                  {(nic) => (
                    <li class="mono small">
                      {nic.name} · {nic.ip || 'no address'} · gw {nic.gateway || '—'}
                    </li>
                  )}
                </For>
              </ul>
            </Show>

            <Show when={runtime().ip}>
              <h3>Reachability</h3>
              <p class="muted small">
                Inside the runtime network the sandbox answers on {runtime().ip}.
                From outside, traffic goes through the gateway on the published
                port; no sandbox port is exposed directly.
              </p>
            </Show>
          </Show>
        </Show>
      </Show>

      <Show when={tab() === 'logs'}>
        <pre class="logs">{logs() || 'No log output.'}</pre>
      </Show>

      <Show when={tab() === 'rules'}>
        <Show when={rules().length > 0} fallback={<p class="muted">No rules attached to this sandbox.</p>}>
          <table class="dense">
            <thead><tr><th>Action</th><th>Host</th><th class="narrow">Port</th><th>Enforcement</th></tr></thead>
            <tbody>
              <For each={rules()}>
                {(rule) => (
                  <tr>
                    <td><StatusBadge status={rule.action === 'allow' ? 'running' : 'denied'}
                                     label={rule.action} /></td>
                    <td class="mono small">{rule.host}</td>
                    <td class="narrow mono">{rule.port ?? '—'}</td>
                    <td class="small">{rule.enforcement}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </Show>

      <Show when={tab() === 'inference'}>
        <Show when={inference().length > 0} fallback={<p class="muted">No model requests recorded yet.</p>}>
          <table class="dense">
            <thead><tr><th>Model</th><th>Endpoint</th><th class="narrow">Status</th><th class="narrow">ms</th><th class="narrow">tok in/out</th></tr></thead>
            <tbody>
              <For each={inference()}>
                {(row) => (
                  <tr>
                    <td class="mono small">{row.model_key || '—'}</td>
                    <td class="mono small">{row.endpoint || '—'}</td>
                    <td class="narrow mono">{row.status_code ?? '—'}</td>
                    <td class="narrow mono">{row.latency_ms ?? '—'}</td>
                    <td class="narrow mono small">{row.tokens_in ?? 0}/{row.tokens_out ?? 0}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </Show>

      <Show when={loading()}>
        <p class="muted small">Loading…</p>
      </Show>
    </Drawer>
  );
}

function formatUptime(seconds) {
  if (!seconds) return '—';
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days) return `${days}d ${hours}h`;
  if (hours) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

function formatBytes(bytes) {
  if (!bytes) return '—';
  const mib = bytes / (1024 * 1024);
  return mib >= 1024 ? `${(mib / 1024).toFixed(1)} GiB` : `${Math.round(mib)} MiB`;
}
