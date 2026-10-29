// What is running inside the sandbox (`docker top`), refreshed live.
import { For, Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';
import { usePolling } from '../../components/Poll.js';

export default function ProcessesTab(props) {
  const [table, setTable] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [sort, setSort] = createSignal('cpu_pct');
  const [busy, setBusy] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const poll = usePolling(() => deck.processes(props.sandbox.id), { everyMs: 4000, onData: (d) => { setTable(d); setError(null); }, onError: (e) => setError(e.message) });
  const rows = () => [...(table()?.items || [])].sort((a, b) => Number(b[sort()]) - Number(a[sort()]));
  async function act(row, action) {
    if (['terminate', 'kill'].includes(action) && !window.confirm(`PID ${row.pid} wirklich ${action === 'kill' ? 'sofort beenden' : 'beenden'}?`)) return;
    setBusy(`${row.pid}:${action}`); setError(null); setNotice(null);
    try { const result = await deck.processAction(props.sandbox.id, row.pid, { action }); setNotice(result.message); await poll.refresh(); }
    catch (failure) { setError(failure.message); } finally { setBusy(null); }
  }

  return (
    <section class="tab-panel">
      <div class="controls">
        <button class={poll.paused() ? 'secondary tiny' : 'primary tiny'} onClick={poll.toggle}>{poll.paused() ? 'Paused' : 'Live'}</button>
        <button class="secondary tiny" onClick={poll.refresh}>Refresh</button>
        <span class="muted small">sort by</span>
        <select value={sort()} onChange={(e) => setSort(e.currentTarget.value)}>
          <option value="cpu_pct">CPU</option><option value="mem_pct">memory</option><option value="pid">PID</option>
        </select>
        <span class="muted small">{table() ? `${rows().length} processes · ${table().taken_at}` : ''}</span>
      </div>
       <Show when={error()}><p class="error">{error()} (a stopped sandbox has no processes)</p></Show>
       <Show when={notice()}><p class="notice">{notice()}</p></Show>
      <div class="table-scroll">
        <table class="dense">
           <thead><tr><th>PID</th><th>PPID</th><th>User</th><th>CPU %</th><th>Mem %</th><th>Time</th><th>Command</th><th>Control</th></tr></thead>
          <tbody>
            <For each={rows()}>
              {(r) => <tr><td class="mono">{r.pid}</td><td class="mono">{r.ppid}</td><td>{r.user}</td><td>{r.cpu_pct}</td><td>{r.mem_pct}</td><td class="mono small">{r.elapsed}</td><td class="mono small">{r.command}</td><td class="actions"><Show when={r.pid !== 1} fallback={<span class="muted small">entrypoint</span>}><button class="tiny" disabled={busy() === `${r.pid}:pause`} onClick={() => act(r, 'pause')}>Pause</button><button class="tiny" disabled={busy() === `${r.pid}:resume`} onClick={() => act(r, 'resume')}>Resume</button><button class="tiny danger" disabled={busy() === `${r.pid}:terminate`} onClick={() => act(r, 'terminate')}>End</button><button class="tiny danger" disabled={busy() === `${r.pid}:kill`} onClick={() => act(r, 'kill')}>Kill</button></Show></td></tr>}
            </For>
          </tbody>
        </table>
      </div>
    </section>
  );
}
