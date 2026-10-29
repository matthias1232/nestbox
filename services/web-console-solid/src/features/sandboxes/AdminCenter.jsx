// The sandbox Admin Center: one sandbox, full screen, one tab per question.
//
// This file is the shell, not a feature. It owns the header (name, state, pack,
// image), the tab strip, and the permission-aware actions; each tab body lives
// in its own file and is filled by one package. That split is why seven console
// packages can work in parallel without editing the same file: a package
// replaces exactly one `*Tab.jsx` and the shell keeps working.
//
// What every tab receives:
//   props.sandbox  the stored sandbox row (name, pack, image, status, runtime)
//   props.reload   re-read the sandbox after an action
//
// The tab list order is the order an operator asks the questions:
// is it healthy → what is it doing → what tools does it have → let me in →
// how do I reach it → where does it talk to → what is allowed → what runs →
// what is installed → what did it say.

import { Show, For, createSignal, createEffect, onCleanup } from 'solid-js';
import Tabs from '../../components/Tabs.jsx';
import StatusBadge from '../../components/StatusBadge.jsx';
import * as api from '../../api/sandboxes.js';
import DashboardTab from './DashboardTab.jsx';
import TrafficTab from './TrafficTab.jsx';
import McpTab from './McpTab.jsx';
import TerminalTab from './TerminalTab.jsx';
import AccessTab from './AccessTab.jsx';
import MapTab from './MapTab.jsx';
import SupervisorTab from './SupervisorTab.jsx';
import '../../styles/sandbox-hierarchy.css';
import PoliciesTab from './PoliciesTab.jsx';
import ProcessesTab from './ProcessesTab.jsx';
import InventoryTab from './InventoryTab.jsx';
import LogsTab from './LogsTab.jsx';
import MountsTab from './MountsTab.jsx';

const TABS = [
  { id: 'dashboard', label: 'Dashboard', component: DashboardTab },
  { id: 'traffic', label: 'Traffic', component: TrafficTab },
  { id: 'mcp', label: 'MCP', component: McpTab },
  { id: 'terminal', label: 'Terminal', component: TerminalTab },
  { id: 'access', label: 'Access', component: AccessTab },
  { id: 'mounts', label: 'Mounts', component: MountsTab },
  { id: 'map', label: 'Topology', component: MapTab },
  { id: 'supervisor', label: 'Supervisor', component: SupervisorTab },
  { id: 'policies', label: 'Policies', component: PoliciesTab },
  { id: 'processes', label: 'Processes', component: ProcessesTab },
  { id: 'inventory', label: 'Inventory', component: InventoryTab },
  { id: 'logs', label: 'Logs', component: LogsTab },
];

const SHORT_DIGEST = 19;

export default function AdminCenter(props) {
  const [tab, setTab] = createSignal(props.initialTab || 'dashboard');
  const [sandbox, setSandbox] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [busy, setBusy] = createSignal(false);
  // Bumped by `reload`, so a tab can depend on it without owning the fetch.
  const [revision, setRevision] = createSignal(0);

  async function load() {
    const id = props.sandboxId;
    if (!id) {
      setSandbox(null);
      return;
    }
    try {
      setSandbox(await api.get(id));
      setError(null);
    } catch (failure) {
      setError(failure.message || 'The sandbox could not be read.');
    }
  }

  function reload() {
    setRevision(revision() + 1);
    load();
  }

  createEffect(() => {
    // Re-read whenever a different sandbox is opened.
    props.sandboxId;
    setTab(props.initialTab || 'dashboard');
    load();
  });

  // Close on Escape: a full-screen view with no browser chrome needs a way out
  // that does not require hunting for the button.
  function onKey(event) {
    if (event.key === 'Escape') props.onClose();
  }
  window.addEventListener('keydown', onKey);
  onCleanup(() => window.removeEventListener('keydown', onKey));

  async function act(action) {
    const id = props.sandboxId;
    if (!id || busy()) return;
    setBusy(true);
    try {
      await api.act(id, action);
      reload();
    } catch (failure) {
      setError(failure.message || `Could not ${action} the sandbox.`);
    } finally {
      setBusy(false);
    }
  }

  const running = () => (sandbox()?.status || '').includes('running');
  const address = () => {
    const networks = sandbox()?.runtime?.network || [];
    const first = networks.find((entry) => entry.ip) || networks[0];
    return first?.ip || '—';
  };
  const digest = () => {
    const value = sandbox()?.image_digest;
    if (!value) return 'not pinned';
    return value.length > SHORT_DIGEST ? `${value.slice(0, SHORT_DIGEST)}…` : value;
  };

  return (
    <div class="admin-center">
      <header class="ac-header">
        <div class="ac-title">
          <button class="ghost" onClick={props.onClose} aria-label="Back to sandboxes">←</button>
          <div>
            <h1>
              {sandbox()?.name || props.sandboxId}
              <Show when={sandbox()}>
                <StatusBadge status={sandbox().status} />
              </Show>
            </h1>
            <p class="muted small">
              {sandbox()?.pack_key || 'pack'} · {sandbox()?.image || 'image'} ·{' '}
              <span class="mono">{digest()}</span> · {sandbox()?.node_name || 'local'} ·{' '}
              <span class="mono">{address()}</span>
            </p>
          </div>
        </div>

        <div class="ac-actions">
          <button class="tiny" disabled={busy() || running()} onClick={() => act('start')}>Start</button>
          <button class="tiny" disabled={busy() || !running()} onClick={() => act('stop')}>Stop</button>
          <button class="tiny" disabled={busy() || !running()} onClick={() => act('restart')}>Restart</button>
          <button class="tiny" onClick={() => setTab('terminal')} disabled={!running()}>Terminal</button>
          <button class="tiny" onClick={() => setTab('access')}>Access</button>
        </div>
      </header>

      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>

      <Tabs tabs={TABS} active={tab()} onChange={setTab} />

      <section class="ac-body" role="tabpanel">
        <Show
          when={sandbox()}
          fallback={<p class="muted">Loading {props.sandboxId} …</p>}
        >
          <For each={TABS}>
            {(entry) => (
              <Show when={entry.id === tab()}>
                <entry.component sandbox={sandbox()} revision={revision()} reload={reload} />
              </Show>
            )}
          </For>
        </Show>
      </section>
    </div>
  );
}
