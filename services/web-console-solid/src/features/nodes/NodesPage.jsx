// The nodes: the machines sandboxes and models run on.
//
// The screen answers "where does this run" and "why was that model refused" for
// one machine at a time: the last measurement, what the model installs on it
// reserve, and what is left. The numbers are the server's; a node nobody has
// measured says so instead of drawing zeros as capacity.
//
// The page itself is markup. State and calls live in `useNodes.js`.

import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import Empty from '../../components/Empty.jsx';
import BudgetBars from './BudgetBars.jsx';
import NodeDrawer from './NodeDrawer.jsx';
import { kindLabel, statusTone } from './nodeFormat.js';
import { useNodes } from './useNodes.js';
import '../../styles/nodes.css';

export default function NodesPage(props) {
  const state = useNodes();
  const [openId, setOpenId] = createSignal(null);

  onMount(() => state.load());

  const measured = createMemo(() => state.items().filter((node) => node.hardware_known).length);
  const sandboxTotal = createMemo(() =>
    state.items().reduce((sum, node) => sum + (node.budget?.sandboxes || 0), 0));

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Nodes</h1>
          <p class="muted">
            {state.items().length} node{state.items().length === 1 ? '' : 's'}
            {' · '}{measured()} measured
            {' · '}{sandboxTotal()} sandbox{sandboxTotal() === 1 ? '' : 'es'} placed
          </p>
        </div>
        <div class="controls">
          <button class="secondary" onClick={() => state.load()} disabled={state.loading()}>
            {state.loading() ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </header>

      <Show when={state.error()}><p class="error">{state.error()}</p></Show>

      <Show when={state.items().length > 0}>
        <div class="table-scroll">
          <table class="node-table">
            <thead>
              <tr>
                <th>Node</th>
                <th class="narrow">Kind</th>
                <th class="narrow">Status</th>
                <th class="narrow">Container GPU</th>
                <th class="narrow">Sandboxes</th>
                <th>Budget</th>
              </tr>
            </thead>
            <tbody>
              <For each={state.items()}>
                {(node) => (
                  <tr class={openId() === node.id ? 'selected' : ''}>
                    <td>
                      <button class="link" onClick={() => setOpenId(node.id)}>
                        {node.display_name}
                      </button>
                      <div class="muted small mono">{node.name}</div>
                      <Show when={node.description}>
                        <div class="muted small node-desc">{node.description}</div>
                      </Show>
                    </td>
                    <td class="narrow small">
                      {kindLabel(node.kind)}
                      <Show when={node.kind === 'kubernetes'}>
                        <span
                          class="badge warn tiny-badge"
                          title="Declared in the data model; no adapter runs sandboxes on it yet."
                        >
                          <i />no adapter
                        </span>
                      </Show>
                      <Show when={node.is_local}>
                        <span class="badge muted tiny-badge" title="This is the host the control plane runs on.">
                          <i />this host
                        </span>
                      </Show>
                    </td>
                    <td class="narrow">
                      <span class={`badge ${statusTone(node.status)}`}><i />{node.status}</span>
                    </td>
                    <td class="narrow small">
                      <Show
                        when={node.container_gpu}
                        fallback={
                          <Show
                            when={node.hardware_known}
                            fallback={<span class="muted" title="Nobody has probed this node, so the runner has not answered this either.">not measured yet</span>}
                          >
                            <span class="muted" title="Measured by the runner: a container on this node cannot be given a GPU.">
                              no
                            </span>
                          </Show>
                        }
                      >
                        <span class="badge ok" title="Measured by the runner: a container here can be given a GPU.">
                          <i />yes
                        </span>
                      </Show>
                    </td>
                    <td class="narrow mono">
                      <button
                        class="link"
                        title="Show the sandboxes on this node"
                        onClick={() => setOpenId(node.id)}
                      >
                        {node.budget?.sandboxes ?? 0}
                      </button>
                    </td>
                    <td class="node-budget">
                      <BudgetBars
                        budget={node.budget}
                        hardwareKnown={node.hardware_known}
                        detectedAt={node.detected_at}
                        compact
                      />
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>

      <Show when={state.loaded() && state.items().length === 0}>
        <Empty
          title="No node registered"
          hint="Phase 1 registers the host the control plane runs on. A node appears here once it is registered, and its budget once it has been probed."
        />
      </Show>

      <p class="muted small phase-note">
        A budget is the node's last measurement minus what the models installed on it
        reserve; it is not a live reading of free memory. A node that has never been
        probed has no budget, and this page says so instead of showing zeros.
      </p>

      <NodeDrawer
        nodeId={openId()}
        onClose={() => setOpenId(null)}
        onChanged={() => state.load()}
        onOpenSandbox={props.onOpenSandbox}
      />
    </section>
  );
}
