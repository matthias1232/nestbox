// One persistent root: where it lives, what is inside it, where it is mounted,
// and the three actions on it.
//
// The mount path is typed. The control plane cannot list a node's filesystem, so
// a picker that promised to browse it would be a lie; the server validates the
// path, and its refusal is what the operator sees.

import { For, Show } from 'solid-js';
import Drawer from '../../components/Drawer.jsx';
import { usageLabel } from './usePersistent.js';

export default function PersistentDetail(props) {
  const root = () => props.root;
  const state = props.state;
  const usage = () => state.usage();

  return (
    <Drawer
      open={true}
      title={root().slug}
      subtitle={`${state.kindLabel(root().kind)} · ${state.nodeLabel(root().node_id)}`}
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

      <dl class="facts">
        <dt>Kind</dt><dd>{state.kindLabel(root().kind)}</dd>
        <dt>Node</dt><dd>{state.nodeLabel(root().node_id)}</dd>
        <dt>Slug</dt><dd class="mono">{root().slug}</dd>
        <dt>Host path</dt><dd class="mono small rp-wrap">{root().host_path}</dd>
        <dt>Used</dt>
        <dd>
          {usageLabel(root())}
          <Show when={root().measured_at}>
            <span class="muted small"> · measured {root().measured_at}</span>
          </Show>
        </dd>
        <dt>Created</dt><dd class="mono small">{root().created_at}</dd>
      </dl>

      <p class="muted small rp-note">
        {usage()?.note
          || 'The control plane cannot du a node\'s disk; it reports a measurement only when the runner supplies one.'}
      </p>

      <h3>Mounted in</h3>
      <Show
        when={root().mounted_in?.length > 0}
        fallback={<p class="muted small">Not mounted in any sandbox. The directory exists and holds its data.</p>}
      >
        <div class="table-scroll">
          <table class="dense">
            <thead>
              <tr>
                <th>Sandbox</th><th>Path in the sandbox</th><th class="narrow">Mode</th>
                <th class="narrow" />
              </tr>
            </thead>
            <tbody>
              <For each={root().mounted_in}>
                {(mounted) => (
                  <tr>
                    <td class="mono">{mounted.sandbox_name}</td>
                    <td class="mono small">{mounted.mount_path}</td>
                    <td class="narrow">
                      <span class={mounted.read_only ? 'badge muted' : 'badge warn'}>
                        <i />{mounted.read_only ? 'read-only' : 'read-write'}
                      </span>
                    </td>
                    <td class="narrow actions">
                      <button
                        class="tiny secondary"
                        disabled={state.busy()}
                        onClick={() => props.onDismount(root(), mounted)}
                      >
                        Dismount
                      </button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
        <p class="muted small rp-note">
          One read-write mount per root: two containers writing the same files is corruption the
          platform cannot resolve. Read-only mounts are unlimited.
        </p>
      </Show>

      <h3>Mount it</h3>
      <Show
        when={!state.sandboxesError() || state.sandboxes().length > 0}
        fallback={<p class="warn-note">The sandbox list could not be read. {state.sandboxesError()}</p>}
      >
        <Show
          when={state.sandboxes().length > 0}
          fallback={<p class="muted small">No sandbox on this node. A root mounts only into a sandbox of its own node.</p>}
        >
          <form class="rp-mount-form" onSubmit={(event) => { event.preventDefault(); state.mount(root()); }}>
            <label class="field">
              <span>Sandbox on this node</span>
              <select
                value={state.sandboxId()}
                onChange={(event) => state.setSandboxId(event.currentTarget.value)}
              >
                <option value="">choose a sandbox</option>
                <For each={state.sandboxes()}>
                  {(sandbox) => <option value={sandbox.id}>{sandbox.name} ({sandbox.status})</option>}
                </For>
              </select>
            </label>
            <label class="field">
              <span>Path inside the sandbox</span>
              <input
                value={state.mountPath()}
                placeholder="/workspace/project"
                onInput={(event) => state.setMountPath(event.currentTarget.value)}
              />
            </label>
            <label class="rp-check">
              <input
                type="checkbox"
                checked={state.readOnly()}
                onChange={(event) => state.setReadOnly(event.currentTarget.checked)}
              />
              <span>read-only</span>
            </label>
            <button type="submit" disabled={state.busy()}>
              {state.busy() ? 'Working…' : 'Mount'}
            </button>
          </form>
          <p class="muted small">
            The path is typed, not picked: the control plane cannot list the node's filesystem,
            so the server validates what you type. A second read-write mount answers
            <span class="mono"> 409 mount.conflict</span> and names the sandbox that holds it.
          </p>
        </Show>
      </Show>

      <h3>Archive or delete</h3>
      <p class="muted small">
        Archiving hides the root and keeps its directory and data. Deleting the record removes
        the row and leaves the directory on the node: the control plane has no access to that
        filesystem. A root that is still mounted is refused with
        <span class="mono"> 409 root.in_use</span>.
      </p>
      <div class="actions rp-editor-actions">
        <button class="secondary" disabled={state.busy()} onClick={() => props.onArchive(root())}>
          Archive root
        </button>
        <button class="danger" disabled={state.busy()} onClick={() => props.onPurge(root())}>
          Delete record
        </button>
      </div>
    </Drawer>
  );
}
