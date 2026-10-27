// One recipe: what it asks for, the image it produced, and the actions that
// change it.
//
// The immutability of the image reference is stated here and not in a tooltip,
// because this is the moment an operator might expect an edit to change the tag
// in place. It does not: the tag is the content hash.

import { For, Show } from 'solid-js';
import Drawer from '../../components/Drawer.jsx';
import StatusBadge from '../../components/StatusBadge.jsx';
import RecipeEditor from './RecipeEditor.jsx';

export default function RecipeDetail(props) {
  const recipe = () => props.recipe;
  const state = props.state;

  function buildLabel() {
    if (recipe().state === 'building') return 'Building…';
    if (recipe().state === 'built') return 'Build again';
    if (recipe().state === 'failed') return 'Build again';
    return 'Build image';
  }

  return (
    <Drawer
      open={true}
      title={recipe().name}
      subtitle={`revision ${recipe().revision} · ${recipe().baseline_key}`}
      onClose={state.close}
    >
      <Show when={state.error()}>
        <p class="error">
          {state.error()}
          <Show when={state.errorCode()}>
            <span class="rp-code">{state.errorCode()}</span>
          </Show>
        </p>
      </Show>
      <Show when={state.notice()}><p class="notice">{state.notice()}</p></Show>

      <Show
        when={!state.editing()}
        fallback={
          <RecipeEditor
            recipe={recipe()}
            catalogs={state.catalogs()}
            catalogError={state.catalogError()}
            busy={state.busy()}
            onBaselineChange={state.reloadCatalog}
            onSave={(body) => state.save(recipe().id, body)}
            onCancel={state.cancelEdit}
          />
        }
      >
        <dl class="facts">
          <dt>Baseline</dt><dd class="mono">{recipe().baseline_key}</dd>
          <dt>Node</dt><dd>{state.nodeLabel(recipe().node_id)}</dd>
          <dt>Created</dt><dd class="mono small">{recipe().created_at}</dd>
          <dt>Updated</dt><dd class="mono small">{recipe().updated_at}</dd>
        </dl>

        <h3>Image</h3>
        <div class="rp-section">
          <div class="rp-subhead">
            <StatusBadge status={recipe().state} />
            <Show when={recipe().build_job_id}>
              <span class="muted small">
                job <span class="mono">{recipe().build_job_id}</span>
              </span>
            </Show>
          </div>

          <Show
            when={recipe().image_ref}
            fallback={
              <p class="muted small rp-note">
                No image yet. Building writes an image tagged by this recipe's content hash.
              </p>
            }
          >
            <p class="rp-ref">{recipe().image_ref}</p>
            <p class="muted small rp-note">
              Immutable: the tag is the content hash, so this reference always names the same
              image. An edit produces a new reference instead of rebuilding this one.
            </p>
            <Show when={recipe().state !== 'built'}>
              <p class="warn-note small">
                This reference is the last image that was built, not one for revision
                {' '}{recipe().revision}. A sandbox created now would run it, because the
                API refuses only a recipe that has no image at all.
              </p>
            </Show>
          </Show>

          <Show when={recipe().state === 'failed' && recipe().build_error}>
            <div class="error rp-build-error">
              <strong>Build failed</strong>
              <pre>{recipe().build_error}</pre>
            </div>
          </Show>

          <div class="actions rp-actions">
            <button
              onClick={() => state.build(recipe().id)}
              disabled={state.busy() || recipe().state === 'building'}
            >
              {buildLabel()}
            </button>
            <Show when={recipe().state === 'built'}>
              <span class="muted small rp-hint">
                An unchanged recipe answers with the existing image instead of building again.
              </span>
            </Show>
          </div>
        </div>

        <h3>Tools</h3>
        <Show
          when={recipe().tools?.length > 0}
          fallback={<p class="muted small">No extra tools. Only the baseline's own packages are in the image.</p>}
        >
          <div class="rp-tags">
            <For each={recipe().tools}>{(tool) => <span class="rp-chip">{tool}</span>}</For>
          </div>
        </Show>

        <h3>Agents</h3>
        <Show
          when={recipe().agents?.length > 0}
          fallback={<p class="muted small">No agents. This is a base image with tools.</p>}
        >
          <div class="table-scroll">
            <table class="dense">
              <thead>
                <tr>
                  <th>Instance</th><th>Pack</th><th>Home</th><th>Storage</th>
                  <th class="narrow">Port</th>
                </tr>
              </thead>
              <tbody>
                <For each={recipe().agents}>
                  {(agent) => (
                    <tr>
                      <td class="mono">{agent.instance}</td>
                      <td class="mono small">{agent.pack_key}</td>
                      <td class="small" title={(agent.home_paths || []).join(', ')}>
                        {agent.home_mode}
                        <Show when={agent.home_paths?.length}>
                          <div class="muted small mono">{agent.home_paths.join(', ')}</div>
                        </Show>
                      </td>
                      <td class="small">{agent.persistence}</td>
                      <td class="narrow mono">{agent.port ?? '—'}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
          <p class="muted small rp-note">
            Persistent means the agent's home is a root on the node and survives the
            container; ephemeral means it dies with it.
          </p>
        </Show>

        <h3>Resources</h3>
        <dl class="facts">
          <dt>Memory</dt><dd class="mono">{recipe().resources?.memory_limit}</dd>
          <dt>CPU</dt><dd class="mono">{recipe().resources?.cpu_limit}</dd>
          <dt>GPUs</dt><dd class="mono">{recipe().resources?.gpu_count}</dd>
          <dt>Processes</dt><dd class="mono">{recipe().resources?.pids_limit}</dd>
        </dl>

        <h3>Project mounts</h3>
        <Show
          when={recipe().project_mounts?.length > 0}
          fallback={
            <p class="muted small">
              No mounts. A sandbox created from this recipe starts with an empty filesystem
              outside the image.
            </p>
          }
        >
          <ul class="rp-mount-list">
            <For each={recipe().project_mounts}>
              {(mount) => (
                <li>
                  <span class="mono small">{shortId(mount.root_id)}</span>
                  <span class="mono small"> → {mount.mount_path}</span>
                  <span class={mount.read_only ? 'ro' : 'rw'}>
                    {mount.read_only ? ' · read-only' : ' · read-write'}
                  </span>
                </li>
              )}
            </For>
          </ul>
          <p class="muted small rp-note">
            Roots are shown by id here; the Persistent data page names the root, its node,
            and where it lives on disk.
          </p>
        </Show>

        <h3>Run it</h3>
        <form class="rp-run" onSubmit={(event) => { event.preventDefault(); state.instantiate(recipe().id); }}>
          <label class="field">
            <span>Sandbox name</span>
            <input
              value={state.sandboxName()}
              required
              placeholder="lowercase-name"
              onInput={(event) => state.setSandboxName(event.currentTarget.value)}
            />
          </label>
          <button type="submit" disabled={state.busy() || !recipe().image_ref}>
            {state.busy() ? 'Working…' : 'Create sandbox'}
          </button>
        </form>
        <Show when={!recipe().image_ref}>
          <p class="muted small">
            Disabled because this recipe has no image. The API refuses an instantiate without
            one (409 recipe.not_built), so the button does not promise a sandbox it cannot make.
          </p>
        </Show>
        <p class="muted small">
          A recipe is a template, so the name is per sandbox. The sandbox gets this image, the
          resources, and the mounts above; its data lives in the mounts, so deleting it leaves
          the roots in place.
        </p>

        <div class="actions rp-editor-actions">
          <button class="secondary" onClick={() => state.startEdit(recipe())} disabled={state.busy()}>
            Edit recipe
          </button>
          <button class="danger" onClick={() => props.onDelete(recipe())} disabled={state.busy()}>
            Delete recipe
          </button>
        </div>
      </Show>
    </Drawer>
  );
}

function shortId(id) {
  return String(id || '').slice(0, 8);
}
