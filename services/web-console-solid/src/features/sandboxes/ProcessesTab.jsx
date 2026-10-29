// What is running inside the sandbox (`docker top`), refreshed live.
import { For, Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';
import { usePolling } from '../../components/Poll.js';

export default function ProcessesTab(props) {
  const [table, setTable] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [sort, setSort] = createSignal('cpu_pct');
  const poll = usePolling(() => deck.processes(props.sandbox.id), { everyMs: 4000, onData: (d) => { setTable(d); setError(null); }, onError: (e) => setError(e.message) });
  const rows = () => [...(table()?.items || [])].sort((a, b) => Number(b[sort()]) - Number(a[sort()]));

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
      <div class="table-scroll">
        <table class="dense">
          <thead><tr><th>PID</th><th>PPID</th><th>User</th><th>CPU %</th><th>Mem %</th><th>Time</th><th>Command</th></tr></thead>
          <tbody>
            <For each={rows()}>
              {(r) => <tr><td class="mono">{r.pid}</td><td class="mono">{r.ppid}</td><td>{r.user}</td><td>{r.cpu_pct}</td><td>{r.mem_pct}</td><td class="mono small">{r.elapsed}</td><td class="mono small">{r.command}</td></tr>}
            </For>
          </tbody>
        </table>
      </div>
    </section>
  );
}
