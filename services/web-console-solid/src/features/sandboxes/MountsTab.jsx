// Directories on the node that this sandbox sees, and moving them between sandboxes.
//
// A mount is a persistent root bound into a sandbox. Docker fixes the binds when a
// container is created, so changing them takes effect when the container is
// replaced: "Apply now" does that. What lives on a persistent root survives; what
// was written elsewhere inside the old container does not, and the button says so.
import { For, Show, createResource, createSignal } from 'solid-js';
import * as persistent from '../../api/persistent.js';
import * as deck from '../../api/deck.js';

export default function MountsTab(props) {
  const id = () => props.sandbox.id;
  const [roots, { refetch }] = createResource(() => persistent.list());
  const [rootId, setRootId] = createSignal('');
  const [path, setPath] = createSignal('/workspace');
  const [readOnly, setReadOnly] = createSignal(false);
  const [pending, setPending] = createSignal(false);
  const [message, setMessage] = createSignal(null);
  const [failure, setFailure] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  const items = () => roots()?.items || [];
  const mine = () => items().flatMap((root) => (root.mounted_in || []).filter((m) => m.sandbox_id === id()).map((m) => ({ root, mount: m })));
  const available = () => items().filter((root) => !(root.mounted_in || []).some((m) => m.sandbox_id === id()));

  async function run(action, done, changesMounts = true) {
    setFailure(null); setMessage(null);
    try {
      await action();
      if (done) setMessage(done);
      if (changesMounts) setPending(true);
      await refetch();
    } catch (error) {
      setFailure(error.message + (error.code ? ` (${error.code})` : ''));
    }
  }

  async function apply() {
    const ok = window.confirm(
      `Apply the mounts to ${props.sandbox.name}?\n\nThe container is replaced. Everything on a persistent root stays. Anything written elsewhere inside the container (installed packages, files outside a mounted folder) is lost. Open terminals end.`,
    );
    if (!ok) return;
    setBusy(true);
    await run(() => deck.recreate(id()), 'The sandbox was recreated with its current mounts.', false);
    setPending(false);
    setBusy(false);
    props.reload?.();
  }

  return (
    <section class="tab-panel">
      <p class="muted small">
        A mount shows a directory of the node inside this sandbox. A root can be mounted read-write in one sandbox
        and read-only in any number; to move one, remove it here and mount it in the other sandbox.
      </p>
      <Show when={failure()}><p class="error">{failure()}</p></Show>
      <Show when={message()}><p class="notice">{message()}</p></Show>

      <h3>Mounted in {props.sandbox.name}</h3>
      <Show when={mine().length} fallback={<p class="muted small">Nothing is mounted. The sandbox sees only its own container filesystem.</p>}>
        <table>
          <thead><tr><th>Root</th><th>Inside the sandbox</th><th>Mode</th><th>Host path</th><th /></tr></thead>
          <tbody>
            <For each={mine()}>
              {({ root, mount }) => (
                <tr>
                  <td>{root.kind} · <b>{root.slug}</b></td>
                  <td class="mono">{mount.mount_path}</td>
                  <td class="small">{mount.read_only ? 'read-only' : 'read-write'}</td>
                  <td class="mono small">{root.host_path}</td>
                  <td><button class="tiny" onClick={() => run(() => persistent.dismount(root.id, id()), 'Removed. Apply to take it out of the running sandbox.')}>Remove</button></td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>

      <h3>Mount another root</h3>
      <Show when={available().length} fallback={<p class="muted small">Every existing root is already mounted here. Create roots under Persistent storage.</p>}>
        <form class="controls" onSubmit={(e) => { e.preventDefault(); run(() => persistent.mount(rootId(), { sandbox_id: id(), mount_path: path(), read_only: readOnly() }), 'Saved. Apply to make it visible in the running sandbox.'); }}>
          <select value={rootId()} onChange={(e) => setRootId(e.currentTarget.value)}>
            <option value="">choose a root…</option>
            <For each={available()}>{(root) => (
              <option value={root.id}>{root.kind} · {root.slug}{(root.mounted_in || []).length ? ` (in ${(root.mounted_in).map((m) => m.sandbox_name).join(', ')})` : ''}</option>
            )}</For>
          </select>
          <input class="mono" value={path()} onInput={(e) => setPath(e.currentTarget.value)} placeholder="/workspace" />
          <label class="small"><input type="checkbox" checked={readOnly()} onChange={(e) => setReadOnly(e.currentTarget.checked)} /> read-only</label>
          <button class="primary tiny" type="submit" disabled={!rootId() || !path().trim()}>Mount</button>
        </form>
      </Show>

      <div class="notice warn">
        <p>
          <Show when={pending()} fallback={'Mounts are applied by replacing the container.'}>
            <b>The saved mounts are not in the running sandbox yet.</b>
          </Show>
          {' '}Files on a persistent root stay; files written elsewhere inside the container are lost.
        </p>
        <button class={pending() ? 'primary' : 'secondary'} disabled={busy()} onClick={apply}>{busy() ? 'Recreating…' : 'Apply now (recreate the container)'}</button>
      </div>
    </section>
  );
}
