// Folders × sandboxes: the whole map at a glance.
//
// The table above answers "what is this root mounted in". This one answers the
// other direction: for every folder, which sandboxes see it, at which path, and
// which sandbox is missing the data it probably needs. It is read-only on
// purpose — mounting is a decision with a conflict rule (one writer per root),
// and it stays in the row's own form rather than in a grid of clickable cells.
import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import * as sandboxApi from '../../api/sandboxes.js';

export default function MountsMatrix(props) {
  const [sandboxes, setSandboxes] = createSignal([]);
  const [error, setError] = createSignal(null);
  const [showAll, setShowAll] = createSignal(true);

  onMount(async () => {
    try {
      const page = await sandboxApi.list({ limit: 100 });
      setSandboxes((page.items || []).filter((item) => item.status !== 'deleted'));
    } catch (failure) {
      setError(failure.message);
    }
  });

  // A root carries `mounted_in` (sandbox id, name, path, read-only); the grid
  // needs the reverse lookup once per cell.
  const mountsOf = (root, sandbox) =>
    (root.mounted_in || []).find((mount) => mount.sandbox_id === sandbox.id);

  const visibleSandboxes = createMemo(() =>
    showAll() ? sandboxes() : sandboxes().filter((sandbox) => props.roots.some((root) => mountsOf(root, sandbox))),
  );

  const unmounted = createMemo(() => sandboxes().filter((sandbox) => !props.roots.some((root) => mountsOf(root, sandbox))));

  return (
    <section class="rp-matrix">
      <div class="controls">
        <h3>Folders × sandboxes</h3>
        <button class="secondary tiny" onClick={() => setShowAll(!showAll())}>
          {showAll() ? 'Only sandboxes with data' : 'All sandboxes'}
        </button>
        <span class="muted small">
          {props.roots.length} folders · {visibleSandboxes().length} sandboxes ·
          {' '}{unmounted().length} without any folder
        </span>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <p class="muted small">
        A cell names the path the sandbox sees and whether it may write. “rw” means one writer:
        a second read-write mount of the same folder is refused, read-only mounts are not.
      </p>
      <Show when={props.roots.length && visibleSandboxes().length} fallback={<p class="muted small">No folders or no sandboxes yet.</p>}>
        <div class="table-scroll">
          <table class="dense rp-matrix-table">
            <thead>
              <tr>
                <th class="rp-sticky">Folder</th>
                <For each={visibleSandboxes()}>{(sandbox) => <th>{sandbox.name}</th>}</For>
              </tr>
            </thead>
            <tbody>
              <For each={props.roots}>
                {(root) => (
                  <tr>
                    <td class="rp-sticky">
                      <b>{root.slug}</b>
                      <div class="muted small mono">{root.kind}</div>
                    </td>
                    <For each={visibleSandboxes()}>
                      {(sandbox) => {
                        const mount = mountsOf(root, sandbox);
                        return (
                          <td class={mount ? 'rp-cell rp-cell-on' : 'rp-cell'}>
                            <Show when={mount} fallback={<span class="muted small">—</span>}>
                              <span class="mono small">{mount.mount_path}</span>
                              <div class="muted small">{mount.read_only ? 'ro' : 'rw'}</div>
                            </Show>
                          </td>
                        );
                      }}
                    </For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>
      <Show when={unmounted().length}>
        <p class="muted small">
          Without a folder: {unmounted().map((sandbox) => sandbox.name).join(', ')}. A sandbox
          without a persistent root loses its files when it is recreated.
        </p>
      </Show>
    </section>
  );
}
