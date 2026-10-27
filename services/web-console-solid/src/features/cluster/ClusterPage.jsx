// Gateways, supervisors, and the cluster graph.

import { For, Show, createSignal, onMount } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import TopologyGraph from '../../components/TopologyGraph.jsx';
import Tabs from '../../components/Tabs.jsx';
import * as cluster from '../../api/cluster.js';

const TABS = [
  { id: 'graph', label: 'Topology' },
  { id: 'gateways', label: 'Gateways' },
  { id: 'supervisors', label: 'Supervisors' },
];

export default function ClusterPage() {
  const [tab, setTab] = createSignal('graph');
  const [gateways, setGateways] = createSignal([]);
  const [supervisors, setSupervisors] = createSignal([]);
  const [topology, setTopology] = createSignal({ nodes: [], edges: [] });
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);

  onMount(load);

  async function load() {
    setError(null);
    try {
      const [gatewayList, supervisorList, graph] = await Promise.all([
        cluster.gateways(),
        cluster.supervisors(),
        cluster.topology(),
      ]);
      setGateways(gatewayList.items || []);
      setSupervisors(supervisorList.items || []);
      setTopology({ nodes: graph.nodes || [], edges: graph.edges || [] });
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function sync(gateway) {
    setNotice(null);
    setError(null);
    try {
      const outcome = await cluster.sync(gateway.id);
      setNotice(`Sync: ${outcome.outcome.replaceAll('_', ' ')} (applied v${outcome.applied_version})`);
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Cluster</h1>
          <p class="muted">
            {gateways().length} gateways · {supervisors().length} supervisors
          </p>
        </div>
        <div class="controls">
          <button class="secondary" onClick={load}>Refresh</button>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <Tabs tabs={TABS} active={tab()} onChange={setTab} />

      <Show when={tab() === 'graph'}>
        <TopologyGraph nodes={topology().nodes} edges={topology().edges} />
        <p class="muted small">
          Gateways host sandboxes. Egress leaves through the gateway, where the
          policy decides. A sandbox never reaches the network directly.
        </p>
      </Show>

      <Show when={tab() === 'gateways'}>
        <Show
          when={gateways().length > 0}
          fallback={<p class="muted">No gateway registered yet.</p>}
        >
          <div class="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Status</th>
                  <th>Version</th>
                  <th class="narrow">Config</th>
                  <th class="narrow">Capacity</th>
                  <th>Last seen</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                <For each={gateways()}>
                  {(gateway) => {
                    const drifted = gateway.settings_applied_version < gateway.settings_version;
                    return (
                      <tr>
                        <td class="mono">{gateway.name}</td>
                        <td><StatusBadge status={gateway.status} /></td>
                        <td class="mono small">{gateway.version || '—'}</td>
                        <td class="narrow">
                          <Show when={drifted}
                                fallback={<span class="badge ok"><i />in sync</span>}>
                            <span class="badge warn" title={`desired v${gateway.settings_version}, applied v${gateway.settings_applied_version}`}>
                              <i />drift
                            </span>
                          </Show>
                        </td>
                        <td class="narrow mono small">
                          {gateway.used}/{gateway.capacity}
                        </td>
                        <td class="mono small">{gateway.last_seen || '—'}</td>
                        <td class="actions">
                          <button class="tiny" disabled={!drifted} onClick={() => sync(gateway)}>
                            Sync now
                          </button>
                        </td>
                      </tr>
                    );
                  }}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>

      <Show when={tab() === 'supervisors'}>
        <Show
          when={supervisors().length > 0}
          fallback={<p class="muted">No supervisor reported yet.</p>}
        >
          <div class="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Name</th><th>Status</th><th class="narrow">Config dirtiness</th>
                  <th>Last policy apply</th><th>Last seen</th>
                </tr>
              </thead>
              <tbody>
                <For each={supervisors()}>
                  {(supervisor) => (
                    <tr>
                      <td class="mono">{supervisor.name}</td>
                      <td><StatusBadge status={supervisor.status} /></td>
                      <td class="narrow mono">
                        {supervisor.config_dirtiness > 0
                          ? <span class="badge warn"><i />{supervisor.config_dirtiness}</span>
                          : <span class="badge ok"><i />0</span>}
                      </td>
                      <td class="mono small">{supervisor.last_policy_applied_at || '—'}</td>
                      <td class="mono small">{supervisor.last_seen || '—'}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>
    </section>
  );
}
