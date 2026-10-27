// One node: what it is, what it has left, its sandboxes, and the two writes.
//
// The drawer exists because a node is the answer to "where does this run" and to
// "why was that model refused". Both questions need the budget and the sandbox
// list together, and neither is worth a page of its own.
//
// Nothing here is computed: the budget is the server's, the measurement follows
// the runner's answer, and the display name is the value the server stored.

import { For, Show, createEffect, createSignal } from 'solid-js';
import Drawer from '../../components/Drawer.jsx';
import StatusBadge from '../../components/StatusBadge.jsx';
import BudgetBars from './BudgetBars.jsx';
import { kindLabel, statusTone } from './nodeFormat.js';
import { useNodeDetail } from './useNodes.js';

export default function NodeDrawer(props) {
  const state = useNodeDetail(() => props.nodeId || null, {
    onChanged: () => props.onChanged?.(),
  });

  const [displayName, setDisplayName] = createSignal('');
  const [description, setDescription] = createSignal('');

  // The form follows the stored row: after a reload or a save it shows what the
  // server holds, not what was typed before.
  createEffect(() => {
    const node = state.node();
    if (!node) return;
    setDisplayName(node.display_name || '');
    setDescription(node.description || '');
  });

  const node = () => state.node();
  const unavailable = () => state.error()?.code === 'node.runtime_unavailable';

  async function save(event) {
    event.preventDefault();
    const name = displayName().trim();
    if (!name) return;
    await state.saveDisplay(name, description().trim());
  }

  return (
    <Drawer
      open={!!props.nodeId}
      title={node()?.display_name || 'Node'}
      subtitle={node() ? `${node().name} · ${kindLabel(node().kind)}` : 'loading…'}
      onClose={props.onClose}
    >
      <Show when={state.error()}>
        <Show
          when={unavailable()}
          fallback={<p class="error">{state.error()?.message}</p>}
        >
          <p class="warn-note">
            <b>The runtime on this node did not answer.</b> {state.error()?.message}
            {' '}The node is still registered; its stored state is shown below.
          </p>
        </Show>
      </Show>
      <Show when={state.notice()}><p class="notice">{state.notice()}</p></Show>

      <Show when={node()} fallback={<p class="muted">Loading…</p>}>
        <dl class="facts">
          <dt>Status</dt>
          <dd>
            <span class={`badge ${statusTone(node().status)}`}><i />{node().status}</span>
            <Show when={node().is_local}>
              <span class="badge muted tiny-badge"><i />this host</span>
            </Show>
          </dd>
          <dt>Key</dt><dd class="mono small">{node().name}</dd>
          <dt>Kind</dt>
          <dd>
            {kindLabel(node().kind)}
            <Show when={node().kind === 'kubernetes'}>
              <span class="badge warn tiny-badge" title="Declared in the data model; no adapter runs sandboxes on it yet.">
                <i />no adapter
              </span>
            </Show>
          </dd>
          <dt>Container GPU</dt>
          <dd>
            <Show
              when={node().container_gpu}
              fallback={<span class="muted">No — containers on this node cannot be given a GPU.</span>}
            >
              Yes — a container here can be given a GPU.
            </Show>
          </dd>
          <dt>Runner</dt>
          <dd class="mono small">
            {node().runner_url || 'the control plane itself'}
          </dd>
          <Show when={node().zone || node().region}>
            <dt>Zone</dt>
            <dd class="mono small">
              {[node().zone, node().region].filter(Boolean).join(' · ')}
            </dd>
          </Show>
          <dt>Sandboxes</dt>
          <dd class="mono">{node().budget?.sandboxes ?? state.total()}</dd>
          <dt>Last seen</dt>
          <dd class="mono small">{formatTime(node().last_seen)}</dd>
          <dt>Created</dt>
          <dd class="mono small">{formatTime(node().created_at)}</dd>
        </dl>

        <Show when={node().description}>
          <p class="muted">{node().description}</p>
        </Show>

        <h3>Budget</h3>
        <BudgetBars
          budget={node().budget}
          hardwareKnown={node().hardware_known}
          detectedAt={node().detected_at}
        />

        <div class="node-probe">
          <button class="secondary" onClick={() => state.probe()} disabled={state.probing()}>
            {state.probing() ? 'Probing…' : 'Probe now'}
          </button>
          <span class="muted small">
            Asks the runner on this node. A budget exists only after a measurement,
            so this is what turns "not measured yet" into numbers.
          </span>
        </div>

        <h3>Sandboxes on this node</h3>
        <Show
          when={state.sandboxes().length > 0}
          fallback={
            <p class="muted small">
              {state.loading()
                ? 'Loading…'
                : 'No sandbox sits on this node. A sandbox records its node when it is created.'}
            </p>
          }
        >
          <table class="dense">
            <thead>
              <tr>
                <th>Name</th><th>Status</th><th>Health</th><th>Pack</th><th>Address</th>
              </tr>
            </thead>
            <tbody>
              <For each={state.sandboxes()}>
                {(sandbox) => (
                  <tr>
                    <td>
                      <Show
                        when={props.onOpenSandbox}
                        fallback={<span class="mono small">{sandbox.name}</span>}
                      >
                        <button class="link" onClick={() => props.onOpenSandbox(sandbox.id)}>
                          {sandbox.name}
                        </button>
                      </Show>
                    </td>
                    <td><StatusBadge status={sandbox.status} /></td>
                    <td><StatusBadge status={sandbox.health} /></td>
                    <td class="mono small">{sandbox.pack_key}</td>
                    <td class="mono small">{sandbox.address || '—'}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>

        <h3>Name and description</h3>
        <p class="muted small">
          The key <code>{node().name}</code> is how the API addresses this node and
          cannot be changed here. Status, kind, and container GPU are measured, not
          typed.
        </p>
        <form class="node-form" onSubmit={save}>
          <label class="field">
            <span>Display name</span>
            <input
              value={displayName()}
              maxLength={120}
              onInput={(event) => setDisplayName(event.currentTarget.value)}
            />
          </label>
          <label class="field">
            <span>Description</span>
            <input
              value={description()}
              maxLength={500}
              placeholder="Where this machine stands, and what it is for"
              onInput={(event) => setDescription(event.currentTarget.value)}
            />
          </label>
          <div class="actions">
            <button type="submit" class="primary" disabled={state.saving() || !displayName().trim()}>
              {state.saving() ? 'Saving…' : 'Save'}
            </button>
            <button
              type="button"
              class="ghost"
              disabled={state.saving()}
              onClick={() => {
                setDisplayName(node().display_name || '');
                setDescription(node().description || '');
              }}
            >
              Reset
            </button>
          </div>
        </form>
      </Show>
    </Drawer>
  );
}

/** A timestamp in the reader's locale; an absent one says so instead of "1970". */
function formatTime(value) {
  if (!value) return 'never';
  return new Date(value).toLocaleString();
}
