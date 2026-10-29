// The create form: a name, a pack, a node, and the three limits the runtime
// enforces.
//
// The node's free memory and cores are shown beside the fields they constrain,
// and a request that cannot fit is refused before it is sent. The refusal quotes
// the number the node reported, because "does not fit" alone leaves the operator
// guessing which field to change.
//
// The budget is the server's answer to `GET /nodes`; this form does not sum
// anything itself. The request still goes to the server, which may refuse it for
// a reason the console cannot see, and that refusal is shown as it arrived.

import { For, Show } from 'solid-js';
import { gib } from '../../components/ResourceBars.jsx';
import { useCreateSandbox, hasOwnLogin } from './useCreateSandbox.js';
import '../../styles/sandbox-resources.css';

export default function CreateSandboxForm(props) {
  const state = useCreateSandbox();

  async function submit(event) {
    event.preventDefault();
    const created = await state.submit();
    if (created && props.onCreated) props.onCreated(created);
  }

  return (
    <Show
      when={!state.loading()}
      fallback={<p class="muted">Reading the node list and the agent packs…</p>}
    >
      <Show when={state.error()}>
        <p class="error">{state.error()}</p>
      </Show>

      <form class="sbx-create" onSubmit={submit}>
        <div class="form-grid">
          <label class="field">
            <span>Name</span>
            <input
              value={state.form().name}
              placeholder="web-agent-01"
              onInput={(e) => state.setField('name', e.currentTarget.value)}
            />
            <small class="muted">Lowercase letters, digits and hyphens, at most 63 characters.</small>
          </label>

          <label class="field">
            <span>Agent pack</span>
            <select
              value={state.form().pack_key}
              onChange={(e) => state.setField('pack_key', e.currentTarget.value)}
            >
              <For each={state.packs()}>
                {(pack) => (
                  <option value={pack.key}>
                    {pack.name}{pack.requires_gpu ? ' · needs a GPU' : ''}
                  </option>
                )}
              </For>
            </select>
            <small class="muted">The pack decides the image and the command.</small>
          </label>

          <Show when={hasOwnLogin(state.form().pack_key)}>
            <label class="field">
              <span>Sign-in</span>
              <select
                value={state.form().login}
                onChange={(e) => state.setField('login', e.currentTarget.value)}
              >
                <option value="gateway">Gateway key (ready to work at once)</option>
                <option value="own_account">My own account (I sign in inside the sandbox)</option>
              </select>
              <small class="muted">
                Gateway key: the agent uses the appliance's model endpoint with its key, nothing to type.
                Own account: no key is placed in the sandbox; you log in once inside it (for example
                <code> claude</code> then <code>/login</code>), and that login is kept on the sandbox's
                persistent home, so it survives a restart or a recreate.
              </small>
            </label>
          </Show>

          <label class="field">
            <span>Node</span>
            <select
              value={state.form().node}
              onChange={(e) => state.setField('node', e.currentTarget.value)}
            >
              <For each={state.nodes()}>
                {(node) => (
                  <option value={node.name}>
                    {node.name} · {node.status}{node.is_local ? ' · local' : ''}
                  </option>
                )}
              </For>
            </select>
            <small class="muted">The sandbox runs on this machine.</small>
          </label>

          <label class="field">
            <span>Part of stack</span>
            <select
              value={state.form().parent_id}
              onChange={(e) => state.setField('parent_id', e.currentTarget.value)}
            >
              <option value="">None — a sandbox of its own</option>
              <For each={state.leads()}>
                {(lead) => <option value={lead.id}>{lead.name}</option>}
              </For>
            </select>
            <small class="muted">
              Choose the sandbox this one is needed by, such as the agent it supports. The list
              then shows them together.
            </small>
          </label>
        </div>

        <Show when={state.selectedNode()}>
          <div class="node-budget">
            <Show
              when={state.measured()}
              fallback={
                <p class="warn-note small">
                  {state.selectedNode().name} has not been measured, so its free memory
                  is unknown. The server decides whether the request fits.
                </p>
              }
            >
              <span>
                <b class="mono">{gib(state.budget().ram_free_bytes)}</b> free of{' '}
                <span class="mono">{gib(state.budget().ram_total_bytes)}</span>
              </span>
              <span>
                <b class="mono">{state.budget().cpu_cores}</b> cores
              </span>
              <Show when={state.budget().installs > 0}>
                <span class="muted">
                  <span class="mono">{gib(state.budget().ram_reserved_bytes)}</span> held by{' '}
                  {state.budget().installs} model {state.budget().installs === 1 ? 'install' : 'installs'}
                </span>
              </Show>
              <span class="muted">
                {state.budget().sandboxes} {state.budget().sandboxes === 1 ? 'sandbox' : 'sandboxes'} on it
              </span>
            </Show>
          </div>
        </Show>

        <div class="resource-row">
          <label class="field">
            <span>Memory (GiB)</span>
            <input
              type="number"
              min="0.25"
              step="0.25"
              value={state.form().memory_gib}
              onInput={(e) => state.setField('memory_gib', e.currentTarget.value)}
            />
            <small class="muted">{memoryHint(state)}</small>
          </label>

          <label class="field">
            <span>CPU cores</span>
            <input
              type="number"
              min="0.1"
              step="0.1"
              value={state.form().cpu}
              onInput={(e) => state.setField('cpu', e.currentTarget.value)}
            />
            <small class="muted">{coreHint(state)}</small>
          </label>

          <label class="field">
            <span>PIDs limit</span>
            <input
              type="number"
              min="1"
              step="1"
              value={state.form().pids}
              onInput={(e) => state.setField('pids', e.currentTarget.value)}
            />
            <small class="muted">The number of processes the sandbox may start.</small>
          </label>
        </div>

        <Show when={state.problems().length > 0}>
          <ul class="warn-note sbx-problems">
            <For each={state.problems()}>{(problem) => <li>{problem}</li>}</For>
          </ul>
        </Show>

        <div class="actions">
          <button type="submit" class="primary" disabled={state.blocked() || state.busy()}>
            {state.busy() ? 'Creating…' : 'Create sandbox'}
          </button>
          <button type="button" class="ghost" onClick={props.onCancel}>Cancel</button>
        </div>
      </form>
    </Show>
  );
}

// What the field asks for, against what the node reported. Both numbers come
// from the same budget line above, so the hint and the refusal cannot disagree.
function memoryHint(state) {
  if (state.memoryBytes() === null) return 'A positive number of GiB.';
  if (!state.selectedNode() || !state.measured()) return `${gib(state.memoryBytes())} requested.`;
  return `${gib(state.memoryBytes())} of ${gib(state.budget().ram_free_bytes)} free.`;
}

function coreHint(state) {
  const cores = state.cpuCores();
  if (cores === null) return 'A positive number of cores; a fraction is allowed.';
  if (!state.selectedNode() || !state.measured() || state.budget().cpu_cores <= 0) {
    return 'A fraction is allowed.';
  }
  return `${cores} of ${state.budget().cpu_cores} cores.`;
}
