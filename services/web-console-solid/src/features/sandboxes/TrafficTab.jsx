// Every request this sandbox made, live: what it asked for, what the proxy
// decided, and why. A blocked request can become a rule in two clicks.
import { For, Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';
import * as policies from '../../api/policies.js';
import StatusBadge from '../../components/StatusBadge.jsx';
import { usePolling } from '../../components/Poll.js';

const WINDOWS = ['5m', '15m', '1h', '6h', '24h'];

export default function TrafficTab(props) {
  const [data, setData] = createSignal(null);
  const [windowSpan, setWindowSpan] = createSignal('15m');
  const [filter, setFilter] = createSignal('all');
  const [error, setError] = createSignal(null);
  const [message, setMessage] = createSignal(null);
  const poll = usePolling(
    () => deck.traffic(props.sandbox.id, { window: windowSpan(), filter: filter(), limit: 100 }),
    { everyMs: 4000, onData: (d) => { setData(d); setError(null); }, onError: (e) => setError(e.message) },
  );

  async function draft(event) {
    setMessage(null);
    try {
      await policies.draftFromEvent({ host: event.host, port: event.port, protocol: event.protocol, method: event.method }, props.sandbox.id);
      setMessage(`A rule draft for ${event.host} is waiting under Policies → Pending drafts.`);
    } catch (failure) {
      setError(failure.message);
    }
  }

  const s = () => data()?.summary;
  return (
    <section class="tab-panel">
      <div class="controls">
        <button class={poll.paused() ? 'secondary tiny' : 'primary tiny'} onClick={poll.toggle}>{poll.paused() ? 'Paused' : 'Live'}</button>
        <select value={windowSpan()} onChange={(e) => { setWindowSpan(e.currentTarget.value); poll.refresh(); }}>
          <For each={WINDOWS}>{(w) => <option value={w}>last {w}</option>}</For>
        </select>
        <select value={filter()} onChange={(e) => { setFilter(e.currentTarget.value); poll.refresh(); }}>
          <option value="all">all requests</option><option value="denied">blocked only</option><option value="allowed">allowed only</option>
        </select>
        <Show when={s()}>
          <span class="muted small">{s().requests} requests · {s().allowed} allowed · <b>{s().denied} blocked</b> · {s().destinations} destinations</span>
        </Show>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={message()}><p class="notice">{message()}</p></Show>
      <Show when={(data()?.items || []).length} fallback={<p class="muted">No request in this window. Run something in the sandbox that reaches the network and it appears here within seconds.</p>}>
        <div class="table-scroll">
          <table class="dense">
            <thead><tr><th>Time</th><th>Decision</th><th>Destination</th><th>Request</th><th>Status</th><th>Why</th><th /></tr></thead>
            <tbody>
              <For each={data().items}>
                {(e) => (
                  <tr class={e.decision === 'deny' ? 'row-deny' : ''}>
                    <td class="mono small">{String(e.ts).slice(11, 19)}</td>
                    <td><StatusBadge status={e.decision === 'allow' ? 'running' : e.decision === 'audit' ? 'degraded' : 'denied'} label={e.decision} /></td>
                    <td class="mono small">{e.host}:{e.port}</td>
                    <td class="mono small">{e.method || (e.inspected ? '' : 'tunnel')} {e.path || ''}{e.mcp_tool ? ` · tool ${e.mcp_tool}` : ''}</td>
                    <td class="mono small">{e.status ?? '—'}</td>
                    <td class="small muted">{e.reason}</td>
                    <td><Show when={e.decision === 'deny'}><button class="tiny" onClick={() => draft(e)}>Allow this…</button></Show></td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>
    </section>
  );
}
