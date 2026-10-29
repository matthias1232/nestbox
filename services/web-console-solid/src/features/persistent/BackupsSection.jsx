// Backups of one persistent root: take one, see the list, restore, delete.
//
// A backup is a point-in-time copy of the directory, kept on the node as one archive and
// checked against its checksum before a restore touches anything. Restoring is the
// careful action: the form says what it will do, and "replace" keeps the current content
// aside instead of deleting it.

import { For, Show, createSignal, onMount } from 'solid-js';
import * as api from '../../api/persistent.js';

function size(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MiB`;
  return `${(bytes / 1073741824).toFixed(2)} GiB`;
}

export default function BackupsSection(props) {
  const [items, setItems] = createSignal([]);
  const [note, setNote] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);
  const [message, setMessage] = createSignal(null);
  const [restoring, setRestoring] = createSignal(null);
  const [replace, setReplace] = createSignal(true);
  const [force, setForce] = createSignal(false);

  async function load() {
    try {
      setItems((await api.backups(props.root.id)).items || []);
    } catch (failure) {
      setError(failure.message);
    }
  }
  onMount(load);

  async function take(event) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const made = await api.createBackup(props.root.id, note().trim());
      setNote('');
      setMessage(`Backup taken: ${size(made.bytes)}, checksum ${made.sha256.slice(0, 12)}…`);
      await load();
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  async function restore() {
    const backup = restoring();
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const answer = await api.restoreBackup(backup.id, { replace: replace(), force: force() });
      const kept = answer.previous_content_kept_at ? ` The previous content is kept at ${answer.previous_content_kept_at} on the node.` : '';
      setMessage(`Restored ${answer.restored} entries.${kept}`);
      setRestoring(null);
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  async function remove(backup) {
    if (!window.confirm('Delete this backup and its archive? This cannot be undone.')) return;
    setError(null);
    try {
      await api.deleteBackup(backup.id);
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  return (
    <>
      <h3>Backups</h3>
      <p class="muted small">
        A backup is a copy of everything in this root as it is now, kept on the node as one
        archive. Restoring it brings the root back to that state. Back up before an upgrade or
        before a risky change; the sandbox may keep running while you do.
      </p>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={message()}><p class="notice">{message()}</p></Show>

      <form class="rp-mount-form" onSubmit={take}>
        <label class="field">
          <span>Note (optional)</span>
          <input value={note()} maxlength="200" placeholder="before upgrading the agent"
                 onInput={(e) => setNote(e.currentTarget.value)} />
          <small class="muted">A reminder of why you took it, up to 200 characters.</small>
        </label>
        <button type="submit" disabled={busy()}>{busy() ? 'Working…' : 'Back up now'}</button>
      </form>

      <Show when={items().length > 0} fallback={<p class="muted small">No backup yet.</p>}>
        <div class="table-scroll">
          <table class="dense">
            <thead><tr><th>Taken</th><th>Size</th><th>Note</th><th>Checksum</th><th /></tr></thead>
            <tbody>
              <For each={items()}>
                {(item) => (
                  <tr>
                    <td class="mono small">{String(item.created_at).slice(0, 19).replace('T', ' ')}</td>
                    <td>{size(item.bytes)}</td>
                    <td class="small">{item.note || <span class="muted">—</span>}</td>
                    <td class="mono small">{item.sha256.slice(0, 12)}…</td>
                    <td class="actions">
                      <button class="tiny" onClick={() => setRestoring(item)}>Restore…</button>
                      <button class="tiny danger" onClick={() => remove(item)}>Delete</button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>

      <Show when={restoring()}>
        <div class="editor">
          <h4>Restore the backup from {String(restoring().created_at).slice(0, 19).replace('T', ' ')}</h4>
          <label class="rp-check">
            <input type="checkbox" checked={replace()} onChange={(e) => setReplace(e.currentTarget.checked)} />
            <span>Replace the current content (it is moved aside, not deleted)</span>
          </label>
          <small class="muted">
            On: the root ends up exactly as the backup. Off: the backup is unpacked over the current
            files, so newer files that are not in it stay.
          </small>
          <label class="rp-check">
            <input type="checkbox" checked={force()} onChange={(e) => setForce(e.currentTarget.checked)} />
            <span>Restore even if a running sandbox is writing to this root</span>
          </label>
          <small class="muted">
            Normally refused, because the files would change under the running agent. Stop the sandbox
            first, or tick this if you accept that.
          </small>
          <div class="actions">
            <button class="danger" disabled={busy()} onClick={restore}>Restore</button>
            <button class="secondary" onClick={() => setRestoring(null)}>Cancel</button>
          </div>
        </div>
      </Show>
    </>
  );
}
