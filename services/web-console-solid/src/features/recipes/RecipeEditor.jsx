// The recipe editor: a form over the whole recipe.
//
// The API takes the whole recipe on a PATCH, so this form starts from the recipe
// it was given and sends every field back. Two consequences are deliberate:
//
//   * a picker only ever offers what the catalog knows, and a value the catalog
//     does not know stays visible and selected instead of silently vanishing;
//   * the node is not a field. The server keeps the existing node when none is
//     sent, and a console cannot offer a move it has no reason to make.
//
// The agent and mount lists use <Index> rather than <For>: an edit produces a new
// object for the row, and a keyed list would recreate the input that is being
// typed into, losing the focus on every keystroke.

import { For, Index, Show, createMemo, createSignal } from 'solid-js';

const DEFAULT_RESOURCES = {
  memory_limit: '1g',
  cpu_limit: 1,
  gpu_count: 0,
  pids_limit: 256,
};

const HOME_MODES = [
  { id: 'shared', label: 'shared (/home/nestbox)' },
  { id: 'isolated', label: 'isolated (per agent)' },
];

const PERSISTENCE = [
  { id: 'persistent', label: 'persistent (root on the node)' },
  { id: 'ephemeral', label: 'ephemeral (dies with the container)' },
];

export default function RecipeEditor(props) {
  const [draft, setDraft] = createSignal(fromRecipe(props.recipe));
  const [problem, setProblem] = createSignal(null);

  const catalogs = () => props.catalogs || { baselines: [], packs: [], tools: [], roots: [] };

  const baselineKeys = createMemo(() => {
    const known = catalogs().baselines.map((baseline) => baseline.key);
    const current = draft().baseline_key;
    return known.includes(current) ? known : [current, ...known];
  });

  const toolKeys = createMemo(() => {
    const known = catalogs().tools.map((tool) => tool.key);
    return [...new Set([...known, ...draft().tools])].sort();
  });

  function update(patch) {
    setDraft({ ...draft(), ...patch });
  }

  function updateResources(patch) {
    update({ resources: { ...draft().resources, ...patch } });
  }

  function updateAgent(index, patch) {
    update({
      agents: draft().agents.map((agent, position) => (position === index ? { ...agent, ...patch } : agent)),
    });
  }

  function removeAgent(index) {
    update({ agents: draft().agents.filter((_, position) => position !== index) });
  }

  function addAgent() {
    update({
      agents: [
        ...draft().agents,
        {
          pack_key: catalogs().packs[0]?.key || '',
          instance: '',
          home_mode: 'shared',
          persistence: 'persistent',
          home_paths: '',
          port: '',
        },
      ],
    });
  }

  function updateMount(index, patch) {
    update({
      project_mounts: draft().project_mounts.map((mount, position) => (
        position === index ? { ...mount, ...patch } : mount
      )),
    });
  }

  function removeMount(index) {
    update({ project_mounts: draft().project_mounts.filter((_, position) => position !== index) });
  }

  function addMount() {
    update({
      project_mounts: [
        ...draft().project_mounts,
        { root_id: catalogs().roots[0]?.id || '', mount_path: '', read_only: false },
      ],
    });
  }

  function changeBaseline(baseline) {
    update({ baseline_key: baseline });
    props.onBaselineChange(baseline);
  }

  function toggleTool(key, selected) {
    const tools = selected
      ? [...draft().tools, key]
      : draft().tools.filter((tool) => tool !== key);
    update({ tools: tools.sort() });
  }

  function submit(event) {
    event.preventDefault();
    const body = toBody(draft());
    if (!body.name) {
      setProblem('A recipe needs a name: lowercase letters, digits, and hyphens.');
      return;
    }
    if (!body.baseline_key) {
      setProblem('A recipe needs a baseline.');
      return;
    }
    if (body.agents.some((agent) => !agent.pack_key || !agent.instance)) {
      setProblem('Every agent needs an agent pack and an instance name.');
      return;
    }
    if (body.project_mounts.some((mount) => !mount.root_id || !mount.mount_path)) {
      setProblem('Every mount needs a root and a path inside the sandbox.');
      return;
    }
    setProblem(null);
    props.onSave(body);
  }

  return (
    <form class="rp-editor" onSubmit={submit}>
      <p class="muted small">
        Saving sends the whole recipe. The revision increases, the state returns to draft,
        and the image reference keeps pointing at the last build until you build again.
        The node is not a field here: the recipe stays where it is.
      </p>

      <Show when={problem()}><p class="error">{problem()}</p></Show>
      <Show when={props.catalogError}>
        <p class="warn-note">
          Part of the catalog could not be read, so a picker may show only the values
          already in the recipe. {props.catalogError}
        </p>
      </Show>

      <div class="form-grid">
        <label class="field">
          <span>Name</span>
          <input value={draft().name} required
                 onInput={(event) => update({ name: event.currentTarget.value })} />
        </label>
        <label class="field">
          <span>Baseline</span>
          <select value={draft().baseline_key}
                  onChange={(event) => changeBaseline(event.currentTarget.value)}>
            <For each={baselineKeys()}>
              {(key) => <option value={key}>{baselineLabel(catalogs().baselines, key)}</option>}
            </For>
          </select>
        </label>
        <label class="field">
          <span>Memory limit</span>
          <input value={draft().resources.memory_limit} required placeholder="1g"
                 onInput={(event) => updateResources({ memory_limit: event.currentTarget.value })} />
        </label>
        <label class="field">
          <span>CPU limit</span>
          <input type="number" step="0.1" min="0" value={draft().resources.cpu_limit} required
                 onInput={(event) => updateResources({ cpu_limit: event.currentTarget.value })} />
        </label>
        <label class="field">
          <span>GPUs</span>
          <input type="number" step="1" min="0" value={draft().resources.gpu_count} required
                 onInput={(event) => updateResources({ gpu_count: event.currentTarget.value })} />
        </label>
        <label class="field">
          <span>Process limit</span>
          <input type="number" step="1" min="1" value={draft().resources.pids_limit} required
                 onInput={(event) => updateResources({ pids_limit: event.currentTarget.value })} />
        </label>
      </div>

      <h3>Tools</h3>
      <p class="muted small">
        What the image installs on top of the baseline. Only keys the catalog lists can be
        added; a key already in the recipe stays visible if the catalog no longer offers it.
      </p>
      <Show
        when={toolKeys().length > 0}
        fallback={<p class="muted small">This recipe has no tools and the catalog lists none.</p>}
      >
        <div class="rp-tools">
          <For each={toolKeys()}>
            {(key) => {
              const tool = () => catalogs().tools.find((entry) => entry.key === key);
              return (
                <label
                  class="rp-tool"
                  classList={{ unknown: !tool() }}
                  title={tool()?.description || 'Not in the catalog for this baseline'}
                >
                  <input
                    type="checkbox"
                    checked={draft().tools.includes(key)}
                    onChange={(event) => toggleTool(key, event.currentTarget.checked)}
                  />
                  <span>{tool()?.name || key}</span>
                </label>
              );
            }}
          </For>
        </div>
      </Show>

      <h3>Agents</h3>
      <Show when={draft().agents.length > 0} fallback={<p class="muted small">No agents in this recipe.</p>}>
        <div class="rp-rows">
          <Index each={draft().agents}>
            {(agent, index) => (
              <div class="rp-row agent">
                <label>
                  <span>Instance</span>
                  <input value={agent().instance} required placeholder="hermes"
                         onInput={(event) => updateAgent(index, { instance: event.currentTarget.value })} />
                </label>
                <label>
                  <span>Agent pack</span>
                  <select value={agent().pack_key}
                          onChange={(event) => updateAgent(index, { pack_key: event.currentTarget.value })}>
                    <For each={packKeys(catalogs().packs, agent().pack_key)}>
                      {(key) => <option value={key}>{packLabel(catalogs().packs, key)}</option>}
                    </For>
                  </select>
                </label>
                <label>
                  <span>Home</span>
                  <select value={agent().home_mode}
                          onChange={(event) => updateAgent(index, { home_mode: event.currentTarget.value })}>
                    <For each={HOME_MODES}>{(mode) => <option value={mode.id}>{mode.label}</option>}</For>
                  </select>
                </label>
                <label>
                  <span>Home storage</span>
                  <select value={agent().persistence}
                          onChange={(event) => updateAgent(index, { persistence: event.currentTarget.value })}>
                    <For each={PERSISTENCE}>
                      {(mode) => <option value={mode.id}>{mode.label}</option>}
                    </For>
                  </select>
                </label>
                <label>
                  <span>Port</span>
                  <input type="number" min="1" max="65535" value={agent().port} placeholder="—"
                         onInput={(event) => updateAgent(index, { port: event.currentTarget.value })} />
                </label>
                <button type="button" class="tiny danger" onClick={() => removeAgent(index)}>Remove</button>
                <div class="rp-row-extra">
                  <label>
                    <span>Home path overrides (comma separated; empty means the pack's own paths)</span>
                    <input value={agent().home_paths}
                           onInput={(event) => updateAgent(index, { home_paths: event.currentTarget.value })} />
                  </label>
                </div>
              </div>
            )}
          </Index>
        </div>
      </Show>
      <div class="actions rp-add">
        <button type="button" class="secondary tiny" onClick={addAgent}>Add agent</button>
      </div>

      <h3>Project mounts</h3>
      <p class="muted small">
        A mount binds a persistent root into the sandbox this recipe creates. Data lives
        in the root, never in the image.
      </p>
      <Show when={draft().project_mounts.length > 0} fallback={<p class="muted small">No mounts.</p>}>
        <div class="rp-rows">
          <Index each={draft().project_mounts}>
            {(mount, index) => (
              <div class="rp-row mount">
                <label>
                  <span>Root</span>
                  <select value={mount().root_id}
                          onChange={(event) => updateMount(index, { root_id: event.currentTarget.value })}>
                    <For each={rootOptions(catalogs().roots, mount().root_id)}>
                      {(root) => <option value={root.id}>{rootLabel(root)}</option>}
                    </For>
                  </select>
                </label>
                <label>
                  <span>Path in the sandbox</span>
                  <input value={mount().mount_path} required placeholder="/workspace/project"
                         onInput={(event) => updateMount(index, { mount_path: event.currentTarget.value })} />
                </label>
                <label class="rp-check">
                  <input type="checkbox" checked={!!mount().read_only}
                         onChange={(event) => updateMount(index, { read_only: event.currentTarget.checked })} />
                  <span>read-only</span>
                </label>
                <button type="button" class="tiny danger" onClick={() => removeMount(index)}>Remove</button>
              </div>
            )}
          </Index>
        </div>
      </Show>
      <div class="actions rp-add">
        <button type="button" class="secondary tiny" onClick={addMount}>Add mount</button>
      </div>

      <div class="actions rp-editor-actions">
        <button type="submit" disabled={props.busy}>{props.busy ? 'Saving…' : 'Save recipe'}</button>
        <button type="button" class="ghost" onClick={props.onCancel}>Cancel</button>
      </div>
    </form>
  );
}

function fromRecipe(recipe) {
  return {
    name: recipe.name || '',
    baseline_key: recipe.baseline_key || '',
    tools: [...(recipe.tools || [])],
    agents: (recipe.agents || []).map((agent) => ({
      ...agent,
      port: agent.port ?? '',
      home_paths: (agent.home_paths || []).join(', '),
    })),
    resources: { ...DEFAULT_RESOURCES, ...(recipe.resources || {}) },
    project_mounts: (recipe.project_mounts || []).map((mount) => ({ ...mount })),
  };
}

function toBody(draft) {
  return {
    name: draft.name.trim(),
    baseline_key: draft.baseline_key,
    tools: draft.tools,
    agents: draft.agents.map((agent) => ({
      pack_key: agent.pack_key,
      instance: agent.instance.trim(),
      home_mode: agent.home_mode,
      persistence: agent.persistence,
      home_paths: splitList(agent.home_paths),
      port: agent.port === '' || agent.port == null ? null : Number(agent.port),
    })),
    resources: {
      memory_limit: draft.resources.memory_limit,
      cpu_limit: Number(draft.resources.cpu_limit),
      gpu_count: Number(draft.resources.gpu_count),
      pids_limit: Number(draft.resources.pids_limit),
    },
    project_mounts: draft.project_mounts.map((mount) => ({
      root_id: mount.root_id,
      mount_path: mount.mount_path.trim(),
      read_only: !!mount.read_only,
    })),
  };
}

function splitList(value) {
  return String(value || '')
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function baselineLabel(baselines, key) {
  const baseline = baselines.find((entry) => entry.key === key);
  return baseline ? `${baseline.name} (${baseline.key})` : key;
}

function packKeys(packs, current) {
  const keys = packs.map((pack) => pack.key);
  return keys.includes(current) ? keys : [current, ...keys];
}

function packLabel(packs, key) {
  const pack = packs.find((entry) => entry.key === key);
  return pack ? `${pack.name} (${pack.key})` : key;
}

function rootOptions(roots, current) {
  if (current && roots.some((root) => root.id === current)) return roots;
  // The recipe names a root that is not in the list (archived, or unreadable
  // just now). Keep it selectable so saving does not silently drop the mount.
  return [{ id: current || '', kind: 'unknown', slug: String(current || '').slice(0, 8) }, ...roots];
}

function rootLabel(root) {
  if (!root || !root.id) return 'choose a root';
  return `${root.slug} (${root.kind})`;
}
