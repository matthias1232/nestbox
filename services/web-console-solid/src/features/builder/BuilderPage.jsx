// The builder: one page, six sections, and a draft that is only a recipe.
//
// It exists because a recipe names things the API validates — a baseline, tool
// keys the catalog knows for that baseline, agent packs that baseline supports,
// limits, a node, and roots that exist. A free-text form would let an operator
// type a name nobody serves and learn about it from a 409; every field here is
// a choice from what the server returned, and the only text is the recipe's own
// name and the container paths.
//
// Nothing is stored until "Save recipe". After that the recipe has an id, a
// revision, and two actions of its own: build the image, create a sandbox.

import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import * as packs from '../../api/packs.js';
import * as recipes from '../../api/recipes.js';
import * as nodesApi from '../../api/nodes.js';
import * as persistent from '../../api/persistent.js';
import StatusBadge from '../../components/StatusBadge.jsx';
import '../../styles/builder.css';

const DEFAULT_RESOURCES = {
  memory_limit: '1g',
  cpu_limit: 1,
  gpu_count: 0,
  pids_limit: 256,
};

export default function BuilderPage() {
  const [baselines, setBaselines] = createSignal([]);
  const [packsForBaseline, setPacksForBaseline] = createSignal([]);
  const [toolsForBaseline, setToolsForBaseline] = createSignal([]);
  const [nodes, setNodes] = createSignal([]);
  const [roots, setRoots] = createSignal([]);
  const [catalogsLoaded, setCatalogsLoaded] = createSignal(false);

  const [name, setName] = createSignal('');
  const [baseline, setBaseline] = createSignal('');
  const [tools, setTools] = createSignal([]);
  const [agents, setAgents] = createSignal([]);
  const [resources, setResources] = createSignal({ ...DEFAULT_RESOURCES });
  const [node, setNode] = createSignal('local');
  const [mounts, setMounts] = createSignal([]);

  const [saved, setSaved] = createSignal(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);

  onMount(async () => {
    try {
      const [baselineAnswer, nodeAnswer, rootAnswer] = await Promise.all([
        packs.baselines(),
        nodesApi.list(),
        persistent.list(),
      ]);
      const baselineList = baselineAnswer.items || [];
      setBaselines(baselineList);
      const recommended = baselineList.find((entry) => entry.recommended) || baselineList[0];
      if (recommended) setBaseline(recommended.key);

      const nodeList = nodeAnswer.items || [];
      setNodes(nodeList);
      const local = nodeList.find((entry) => entry.is_local) || nodeList[0];
      if (local) setNode(local.name);

      setRoots(rootAnswer.items || []);
      await loadForBaseline(baseline());
    } catch (failure) {
      setError(failure.message);
    } finally {
      setCatalogsLoaded(true);
    }
  });

  // The catalog depends on the baseline: a tool is resolved to the package name
  // of the baseline's manager, and a pack declares which baselines it supports.
  async function loadForBaseline(key) {
    if (!key) return;
    try {
      const [packAnswer, toolAnswer] = await Promise.all([
        packs.agentPacks(key),
        packs.toolCatalog(key),
      ]);
      setPacksForBaseline(packAnswer.items || []);
      setToolsForBaseline(toolAnswer.items || []);
      // A selection the new baseline does not offer is dropped, with a sentence
      // rather than silently: the recipe must not contain a tool nobody serves.
      const offered = new Set((toolAnswer.items || []).map((tool) => tool.key));
      const dropped = tools().filter((tool) => !offered.has(tool));
      setTools(tools().filter((tool) => offered.has(tool)));
      if (dropped.length) {
        setNotice(`Removed ${dropped.join(', ')}: not available on ${key}.`);
      }
      const supported = new Set((packAnswer.items || []).map((pack) => pack.key));
      const lostAgents = agents().filter((agent) => !supported.has(agent.pack_key));
      setAgents(agents().filter((agent) => supported.has(agent.pack_key)));
      if (lostAgents.length) {
        setNotice(`Removed ${lostAgents.length} agent(s) this baseline does not support.`);
      }
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function changeBaseline(key) {
    setBaseline(key);
    setNotice(null);
    await loadForBaseline(key);
  }

  function toggleTool(key) {
    setTools(tools().includes(key) ? tools().filter((entry) => entry !== key) : [...tools(), key]);
  }

  function addAgent(pack) {
    if (!pack) return;
    const existing = agents().filter((agent) => agent.pack_key === pack.key).length;
    setAgents([
      ...agents(),
      {
        pack_key: pack.key,
        instance: existing ? `${pack.key}-${existing + 1}` : pack.key,
        home_mode: 'shared',
        persistence: pack.requires_gpu ? 'persistent' : 'ephemeral',
        port: pack.ports && pack.ports.length ? pack.ports[0] : null,
      },
    ]);
  }

  function patchAgent(index, patch) {
    setAgents(agents().map((agent, position) => (position === index ? { ...agent, ...patch } : agent)));
  }

  function removeAgent(index) {
    setAgents(agents().filter((_, position) => position !== index));
  }

  function addMount(root) {
    if (!root) return;
    if (mounts().some((mount) => mount.root_id === root.id)) return;
    setMounts([
      ...mounts(),
      { root_id: root.id, mount_path: `/workspace/${root.slug}`, read_only: false },
    ]);
  }

  const nodeBudget = createMemo(() => nodes().find((entry) => entry.name === node())?.budget || null);

  const problems = createMemo(() => {
    const list = [];
    if (!/^[a-z0-9][a-z0-9-]{0,62}$/.test(name().trim())) {
      list.push('A recipe name is a lowercase slug (letters, digits, hyphens).');
    }
    if (!baseline()) list.push('Choose a baseline.');
    if (!node()) list.push('Choose a node.');
    return list;
  });

  const canSave = createMemo(() => problems().length === 0 && !busy() && !saved());

  async function save() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const answer = await recipes.create({
        name: name().trim(),
        baseline_key: baseline(),
        tools: tools(),
        agents: agents().map((agent) => ({
          pack_key: agent.pack_key,
          instance: agent.instance,
          home_mode: agent.home_mode,
          persistence: agent.persistence,
          port: agent.port || null,
        })),
        resources: resources(),
        project_mounts: mounts().map((mount) => ({
          root_id: mount.root_id,
          mount_path: mount.mount_path,
          read_only: mount.read_only,
        })),
        node: node(),
      });
      setSaved(answer);
      setNotice(
        `Recipe "${answer.name}" saved as revision ${answer.revision}. Build it to get an image, ` +
        'then create a sandbox from that image.',
      );
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  async function build() {
    setBusy(true);
    setError(null);
    try {
      const answer = await recipes.build(saved().id);
      setSaved(answer.recipe || saved());
      setNotice(
        answer.reused
          ? 'This content already has an image; the tag is reused instead of rebuilt.'
          : `Build queued as job ${answer.job_id}. The state moves from building to built or failed.`,
      );
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  async function instantiate() {
    setBusy(true);
    setError(null);
    try {
      const sandbox = await recipes.instantiate(saved().id, `${saved().name}-${Date.now() % 10000}`);
      setNotice(`Sandbox "${sandbox.name}" created from the recipe image on ${node()}.`);
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section class="page builder-page">
      <header class="page-head">
        <div>
          <h1>Build a sandbox image</h1>
          <p class="muted">
            A recipe names a baseline, its tools, its agents, the limits, a node, and the roots
            to mount. The image is built from it; the sandbox is created from the image.
          </p>
        </div>
        <div class="controls">
          <Show when={saved()}>
            <span class="muted small">rev {saved().revision}</span>
            <StatusBadge status={saved().state} />
          </Show>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <Show when={catalogsLoaded()} fallback={<p class="muted">Loading the catalog…</p>}>
        <div class="builder-sections">
          <section class="builder-section">
            <h2>1 · Baseline</h2>
            <p class="muted small">The image everything else is added to.</p>
            <For each={baselines()}>
              {(entry) => (
                <label class="builder-choice">
                  <input
                    type="radio"
                    name="baseline"
                    checked={baseline() === entry.key}
                    onChange={() => changeBaseline(entry.key)}
                    disabled={!!saved()}
                  />
                  <span>
                    {entry.name}
                    <Show when={entry.recommended}><span class="muted small"> · recommended</span></Show>
                    <Show when={!entry.openshell_ready}>
                      <span class="muted small"> · not supported by NVIDIA OpenShell</span>
                    </Show>
                  </span>
                </label>
              )}
            </For>
          </section>

          <section class="builder-section">
            <h2>2 · Tools</h2>
            <p class="muted small">
              Names resolve per package manager. A tool this baseline does not package is not
              offered.
            </p>
            <div class="builder-tools">
              <For each={toolsForBaseline()}>
                {(tool) => (
                  <label class={`chip ${tools().includes(tool.key) ? 'active' : ''}`}>
                    <input
                      type="checkbox"
                      checked={tools().includes(tool.key)}
                      disabled={!!saved() || tool.available === false}
                      onChange={() => toggleTool(tool.key)}
                    />
                    {tool.name || tool.key}
                    <Show when={tool.package}>
                      <span class="muted small"> · {tool.package}</span>
                    </Show>
                  </label>
                )}
              </For>
            </div>
          </section>

          <section class="builder-section">
            <h2>3 · Agents</h2>
            <p class="muted small">
              One row per agent instance. A persistent home is a directory on the node and
              survives the container.
            </p>
            <For each={agents()}>
              {(agent, index) => (
                <div class="builder-agent">
                  <strong>{agent.pack_key}</strong>
                  <input
                    value={agent.instance}
                    disabled={!!saved()}
                    onInput={(event) => patchAgent(index(), { instance: event.currentTarget.value })}
                  />
                  <select
                    value={agent.home_mode}
                    disabled={!!saved()}
                    onChange={(event) => patchAgent(index(), { home_mode: event.currentTarget.value })}
                  >
                    <option value="shared">shared home</option>
                    <option value="isolated">isolated home</option>
                  </select>
                  <select
                    value={agent.persistence}
                    disabled={!!saved()}
                    onChange={(event) => patchAgent(index(), { persistence: event.currentTarget.value })}
                  >
                    <option value="ephemeral">ephemeral</option>
                    <option value="persistent">persistent</option>
                  </select>
                  <button class="secondary" disabled={!!saved()} onClick={() => removeAgent(index())}>
                    Remove
                  </button>
                </div>
              )}
            </For>
            <div class="builder-add">
              <For each={packsForBaseline()}>
                {(pack) => (
                  <button class="secondary" disabled={!!saved()} onClick={() => addAgent(pack)}>
                    + {pack.name}
                  </button>
                )}
              </For>
            </div>
          </section>

          <section class="builder-section">
            <h2>4 · Resources</h2>
            <div class="builder-resources">
              <label class="builder-field">
                Memory
                <input
                  class="builder-input"
                  value={resources().memory_limit}
                  disabled={!!saved()}
                  onInput={(event) => setResources({ ...resources(), memory_limit: event.currentTarget.value })}
                />
              </label>
              <label class="builder-field">
                CPU
                <input
                  class="builder-input"
                  type="number"
                  min="0.1"
                  step="0.1"
                  value={resources().cpu_limit}
                  disabled={!!saved()}
                  onInput={(event) => setResources({ ...resources(), cpu_limit: Number(event.currentTarget.value) })}
                />
              </label>
              <label class="builder-field">
                GPUs
                <input
                  class="builder-input"
                  type="number"
                  min="0"
                  value={resources().gpu_count}
                  disabled={!!saved()}
                  onInput={(event) => setResources({ ...resources(), gpu_count: Number(event.currentTarget.value) })}
                />
              </label>
              <label class="builder-field">
                PIDs
                <input
                  class="builder-input"
                  type="number"
                  min="32"
                  value={resources().pids_limit}
                  disabled={!!saved()}
                  onInput={(event) => setResources({ ...resources(), pids_limit: Number(event.currentTarget.value) })}
                />
              </label>
            </div>
          </section>

          <section class="builder-section">
            <h2>5 · Node</h2>
            <select value={node()} disabled={!!saved()} onChange={(event) => setNode(event.currentTarget.value)}>
              <For each={nodes()}>
                {(entry) => <option value={entry.name}>{entry.display_name || entry.name}</option>}
              </For>
            </select>
            <Show when={nodeBudget()}>
              <p class="muted small">
                {nodeBudget().ram_free_bytes
                  ? `${(nodeBudget().ram_free_bytes / 2 ** 30).toFixed(1)} GiB of RAM free`
                  : 'no free RAM measured'}
                {' · '}
                {nodeBudget().cpu_cores} cores · {nodeBudget().sandboxes} sandboxes
              </p>
            </Show>
          </section>

          <section class="builder-section">
            <h2>6 · Project mounts</h2>
            <p class="muted small">
              Optional. The host path is derived by the server; a mounted root is bound into the
              sandbox at the path you type.
            </p>
            <Show
              when={roots().length}
              fallback={<p class="muted small">No persistent root exists yet. Create one under Persistent data.</p>}
            >
              <select disabled={!!saved()} onChange={(event) => {
                const chosen = roots().find((root) => root.id === event.currentTarget.value);
                addMount(chosen);
                event.currentTarget.value = '';
              }}>
                <option value="">Add a root…</option>
                <For each={roots()}>
                  {(root) => <option value={root.id}>{root.slug} ({root.kind})</option>}
                </For>
              </select>
            </Show>
            <For each={mounts()}>
              {(mount, index) => (
                <div class="builder-agent">
                  <span class="mono small">
                    {roots().find((root) => root.id === mount.root_id)?.slug || mount.root_id}
                  </span>
                  <input
                    value={mount.mount_path}
                    disabled={!!saved()}
                    onInput={(event) => setMounts(mounts().map((entry, position) =>
                      position === index() ? { ...entry, mount_path: event.currentTarget.value } : entry))}
                  />
                  <label class="muted small">
                    <input
                      type="checkbox"
                      checked={mount.read_only}
                      disabled={!!saved()}
                      onChange={(event) => setMounts(mounts().map((entry, position) =>
                        position === index() ? { ...entry, read_only: event.currentTarget.checked } : entry))}
                    />
                    read-only
                  </label>
                  <button class="secondary" disabled={!!saved()}
                          onClick={() => setMounts(mounts().filter((_, position) => position !== index()))}>
                    Remove
                  </button>
                </div>
              )}
            </For>
          </section>

          <section class="builder-section">
            <h2>7 · Review and save</h2>
            <label class="builder-field">
              Recipe name
              <input
                class="builder-input"
                placeholder="dev-box"
                value={name()}
                disabled={!!saved()}
                onInput={(event) => setName(event.currentTarget.value)}
              />
            </label>
            <p class="muted small">
              {baseline() || 'no baseline'} · {tools().length} tools · {agents().length} agents ·{' '}
              {mounts().length} mounts · on {node()}
            </p>
            <For each={problems()}>{(problem) => <p class="muted small">· {problem}</p>}</For>
            <div class="controls">
              <button class="primary" disabled={!canSave()} onClick={save}>
                {busy() ? 'Saving…' : 'Save recipe'}
              </button>
              <Show when={saved()}>
                <button class="secondary" disabled={busy()} onClick={build}>Build image</button>
                <button class="secondary" disabled={busy() || saved().state !== 'built'} onClick={instantiate}>
                  Create sandbox
                </button>
              </Show>
            </div>
            <Show when={saved()}>
              <p class="muted small">
                Image: <span class="mono">{saved().image_ref || 'not built yet'}</span>
                <Show when={saved().build_error}> · {saved().build_error}</Show>
              </p>
            </Show>
          </section>
        </div>
      </Show>
    </section>
  );
}
