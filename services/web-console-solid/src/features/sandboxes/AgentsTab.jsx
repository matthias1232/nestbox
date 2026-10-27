// The Agents tab: what runs inside the sandbox, and the command it starts from.
//
// Adding an agent keeps its home and its data; removing one is the only
// destructive action on this tab, so the checkbox says which of the two it will
// be before the confirm asks once. A taken instance name and an agent that is
// already gone are ordinary answers, shown as a sentence.

import { For, Show, createEffect } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import { useSandboxAgents } from './useSandboxAgents.js';
import '../../styles/sandbox-resources.css';

const HOME_MODES = [
  { id: 'shared', label: 'shared — agents see the same home' },
  { id: 'isolated', label: 'isolated — this agent only' },
];

const PERSISTENCE = [
  { id: 'ephemeral', label: 'ephemeral — gone with the sandbox' },
  { id: 'persistent', label: 'persistent — kept on the node' },
];

export default function AgentsTab(props) {
  const state = useSandboxAgents();

  createEffect(() => {
    const id = props.sandboxId;
    if (id) state.load(id);
  });

  function confirmRemove(agent) {
    const keepData = state.keeps(agent.instance);
    let sentence;
    if (agent.persistence !== 'persistent') {
      sentence = `Remove agent “${agent.instance}” from this sandbox? It has no persistent home; only the stored agent and its place in the entrypoint are removed.`;
    } else if (keepData) {
      sentence = `Remove agent “${agent.instance}” from this sandbox? Its persistent home and its data stay on the node.`;
    } else {
      sentence = `Remove agent “${agent.instance}” and stop keeping its persistent home? The data in that home is destroyed.`;
    }
    if (window.confirm(sentence)) state.remove(props.sandboxId, agent.instance);
  }

  async function submit(event) {
    event.preventDefault();
    await state.add(props.sandboxId);
  }

  const canAdd = () =>
    !state.busy()
    && state.form().pack_key !== ''
    && state.form().instance.trim() !== '';

  return (
    <div class="agents-tab">
      <Show when={state.error()}>
        <p class="error">{state.error()}</p>
      </Show>
      <Show when={state.notice()}>
        <p class="notice">{state.notice()}</p>
      </Show>

      <Show when={!state.loading()} fallback={<p class="muted">Reading the agents…</p>}>
        <Show
          when={state.agents().length > 0}
          fallback={
            <p class="muted">
              No agent is stored for this sandbox. It runs the command its pack gave
              it; an agent added below joins that command.
            </p>
          }
        >
          <div class="table-scroll">
            <table class="dense agents">
              <thead>
                <tr>
                  <th>Instance</th>
                  <th>Pack</th>
                  <th>Home</th>
                  <th>Persistence</th>
                  <th class="narrow">Port</th>
                  <th>Status</th>
                  <th>Remove</th>
                </tr>
              </thead>
              <tbody>
                <For each={state.agents()}>
                  {(agent) => (
                    <tr>
                      <td class="mono">{agent.instance}</td>
                      <td class="small">{agent.pack_name || agent.pack_key}</td>
                      <td class="small">
                        {agent.home_mode}
                        <div class="muted small mono">{agent.home_path}</div>
                      </td>
                      <td class="small">{agent.persistence}</td>
                      <td class="narrow mono">{agent.port ?? '—'}</td>
                      <td><StatusBadge status={agent.status} /></td>
                      <td class="narrow">
                        <div class="remove-cell">
                          <label class="keep-data">
                            <input
                              type="checkbox"
                              checked={state.keeps(agent.instance)}
                              onChange={(e) => state.setKeeps(agent.instance, e.currentTarget.checked)}
                            />
                            keep its data
                          </label>
                          <button
                            class="tiny danger"
                            disabled={state.busy()}
                            onClick={() => confirmRemove(agent)}
                          >
                            Remove
                          </button>
                        </div>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>

        <Show when={state.entrypoint().length > 0}>
          <details class="entrypoint">
            <summary>The command this sandbox is started from</summary>
            <pre class="logs mono small">{state.entrypoint().join(' ')}</pre>
          </details>
        </Show>

        <form class="agent-add" onSubmit={submit}>
          <h3>Add an agent</h3>
          <p class="muted small">
            The instance is a slug: lowercase letters, digits and hyphens. A
            persistent home is kept on the node and can be reused by a later add.
          </p>
          <div class="form-grid">
            <label class="field">
              <span>Agent pack</span>
              <select
                value={state.form().pack_key}
                onChange={(e) => state.choosePack(e.currentTarget.value)}
              >
                <For each={state.packs()}>
                  {(pack) => (
                    <option value={pack.key}>
                      {pack.name}{pack.requires_gpu ? ' · needs a GPU' : ''}
                    </option>
                  )}
                </For>
              </select>
            </label>

            <label class="field">
              <span>Instance</span>
              <input
                value={state.form().instance}
                placeholder="hermes"
                onInput={(e) => state.setField('instance', e.currentTarget.value)}
              />
            </label>

            <label class="field">
              <span>Home</span>
              <select
                value={state.form().home_mode}
                onChange={(e) => state.setField('home_mode', e.currentTarget.value)}
              >
                <For each={HOME_MODES}>
                  {(mode) => <option value={mode.id}>{mode.label}</option>}
                </For>
              </select>
            </label>

            <label class="field">
              <span>Persistence</span>
              <select
                value={state.form().persistence}
                onChange={(e) => state.setField('persistence', e.currentTarget.value)}
              >
                <For each={PERSISTENCE}>
                  {(mode) => <option value={mode.id}>{mode.label}</option>}
                </For>
              </select>
            </label>

            <label class="field">
              <span>Port (optional)</span>
              <input
                type="number"
                min="1"
                max="65535"
                step="1"
                value={state.form().port}
                placeholder="no port"
                onInput={(e) => state.setField('port', e.currentTarget.value)}
              />
            </label>
          </div>
          <div class="actions">
            <button type="submit" class="primary" disabled={!canAdd()}>
              {state.busy() ? 'Working…' : 'Add agent'}
            </button>
          </div>
        </form>
      </Show>
    </div>
  );
}
