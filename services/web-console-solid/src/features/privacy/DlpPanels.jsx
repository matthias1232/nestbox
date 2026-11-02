// The DLP dashboard and log, shared by the admin page and the end user's view.
//
// What the numbers mean: an incident is one finding by one detector, and its
// action says what the layer did with it — refused the request (`blocked`), put
// stars over it (`redacted`), replaced the whole request (`replaced`), or only
// recorded it (`allowed`). `cloud` marks findings whose destination counted as a
// cloud delivery. Nothing here can show the value that matched: only the excerpt
// the incident row holds is ever returned.
import { For, Show, createSignal } from 'solid-js';
import * as privacy from '../../api/privacy.js';
import { usePolling } from '../../components/Poll.js';

const FRAMES = ['1h', '24h', '7d', '30d'];
const GROUPS = [
  { id: 'detector', label: 'Detector' },
  { id: 'action', label: 'What happened' },
  { id: 'severity', label: 'Severity' },
  { id: 'sandbox', label: 'Sandbox' },
  { id: 'day', label: 'Day' },
  { id: 'destination', label: 'Destination' },
];
const ACTIONS = ['all', 'blocked', 'redacted', 'replaced', 'allowed'];

const num = (value) => (typeof value === 'number' ? value.toLocaleString() : '—');
const word = (key) => {
  if (key === 'blocked') return 'refused';
  if (key === 'redacted') return 'masked';
  if (key === 'replaced') return 'replaced';
  if (key === 'allowed') return 'recorded only';
  return key.replaceAll('_', ' ');
};

export function DlpDashboard(props) {
  const [frame, setFrame] = createSignal('24h');
  const [group, setGroup] = createSignal('detector');
  const [report, setReport] = createSignal(null);
  const [error, setError] = createSignal(null);
  const poll = usePolling(
    () => privacy.overview({ timeframe: frame(), group_by: group(), sandbox_id: props.sandboxId }),
    { everyMs: 15000, onData: (data) => { setReport(data); setError(null); }, onError: (e) => setError(e.message) },
  );
  const totals = () => report()?.totals;
  const rows = () => report()?.rows || [];
  const peak = () => Math.max(1, ...rows().map((row) => row.incidents));

  return (
    <section class="tab-panel">
      <div class="controls">
        <button class={poll.paused() ? 'secondary tiny' : 'primary tiny'} onClick={poll.toggle}>{poll.paused() ? 'Paused' : 'Live'}</button>
        <span class="muted small">group by</span>
        <select value={group()} onChange={(e) => { setGroup(e.currentTarget.value); poll.refresh(); }}>
          <For each={GROUPS}>{(item) => <option value={item.id}>{item.label}</option>}</For>
        </select>
        <span class="muted small">last</span>
        <select value={frame()} onChange={(e) => { setFrame(e.currentTarget.value); poll.refresh(); }}>
          <For each={FRAMES}>{(item) => <option value={item}>{item}</option>}</For>
        </select>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={totals()}>
        <div class="controls" style="gap:1.5rem;flex-wrap:wrap">
          <span><b>{num(totals().incidents)}</b> <span class="muted small">findings</span></span>
          <span><b>{num(totals().blocked)}</b> <span class="muted small">requests refused</span></span>
          <span><b>{num(totals().redacted)}</b> <span class="muted small">masked</span></span>
          <span><b>{num(totals().replaced)}</b> <span class="muted small">replaced</span></span>
          <span><b>{num(totals().allowed)}</b> <span class="muted small">recorded only</span></span>
          <span><b>{num(totals().cloud)}</b> <span class="muted small">towards a cloud destination</span></span>
        </div>
      </Show>
      <div class="table-scroll">
        <table class="dense">
          <thead><tr><th>{GROUPS.find((g) => g.id === group())?.label}</th><th class="num">Findings</th><th class="num">Refused</th><th class="num">Masked</th><th class="num">Recorded</th><th class="num">Cloud</th><th /></tr></thead>
          <tbody>
            <For each={rows()} fallback={<tr><td colspan="7" class="muted small">No finding in this window. That is the good case; it is not proof that nothing would be found — the layer reports what its detectors match.</td></tr>}>
              {(row) => (
                <tr>
                  <td class="mono small">{group() === 'action' ? word(row.key) : row.key}</td>
                  <td class="num">{num(row.incidents)}</td>
                  <td class="num">{num(row.blocked)}</td>
                  <td class="num">{num(row.redacted)}</td>
                  <td class="num">{num(row.allowed)}</td>
                  <td class="num">{num(row.cloud)}</td>
                  <td style="width:90px"><div style={`height:6px;border-radius:3px;background:var(--chart-1,#60a5fa);width:${Math.max(2, Math.round((row.incidents / peak()) * 100))}%`} /></td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </section>
  );
}

export function DlpLog(props) {
  const [frame, setFrame] = createSignal('24h');
  const [action, setAction] = createSignal('all');
  const [detector, setDetector] = createSignal('all');
  const [items, setItems] = createSignal([]);
  const [error, setError] = createSignal(null);
  const poll = usePolling(
    () => privacy.log({
      timeframe: frame(),
      sandbox_id: props.sandboxId,
      action: action(),
      detector: detector(),
      limit: props.limit || 100,
    }),
    { everyMs: 10000, onData: (data) => { setItems(data.items || []); setError(null); }, onError: (e) => setError(e.message) },
  );
  const detectors = () => [...new Set(items().map((item) => item.incident.detector))].sort();

  return (
    <section class="tab-panel">
      <div class="controls">
        <button class={poll.paused() ? 'secondary tiny' : 'primary tiny'} onClick={poll.toggle}>{poll.paused() ? 'Paused' : 'Live'}</button>
        <span class="muted small">what happened</span>
        <select value={action()} onChange={(e) => { setAction(e.currentTarget.value); poll.refresh(); }}>
          <For each={ACTIONS}>{(item) => <option value={item}>{item === 'all' ? 'all' : word(item)}</option>}</For>
        </select>
        <span class="muted small">detector</span>
        <select value={detector()} onChange={(e) => { setDetector(e.currentTarget.value); poll.refresh(); }}>
          <option value="all">all</option>
          <For each={detectors()}>{(item) => <option value={item}>{word(item)}</option>}</For>
        </select>
        <span class="muted small">last</span>
        <select value={frame()} onChange={(e) => { setFrame(e.currentTarget.value); poll.refresh(); }}>
          <For each={FRAMES}>{(item) => <option value={item}>{item}</option>}</For>
        </select>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <div class="table-scroll">
        <table class="dense">
          <thead><tr><th>When</th><th>Detector</th><th>What happened</th><th>Severity</th><Show when={!props.sandboxId}><th>Sandbox</th></Show><th>Excerpt</th><th>Destination</th></tr></thead>
          <tbody>
            <For each={items()} fallback={<tr><td colspan={props.sandboxId ? 6 : 7} class="muted small">No finding in this window.</td></tr>}>
              {(item) => (
                <tr>
                  <td class="mono small">{String(item.incident.created_at).slice(11, 19)}</td>
                  <td>{word(item.incident.detector)}</td>
                  <td><span class={`badge ${item.incident.action === 'blocked' ? 'error' : item.incident.action === 'allowed' ? 'muted' : 'warn'}`}><i />{word(item.incident.action)}</span></td>
                  <td class="small">{item.incident.severity}</td>
                  <Show when={!props.sandboxId}><td class="mono small">{item.sandbox_name}</td></Show>
                  <td class="mono small">{item.incident.masked_value || '—'}</td>
                  <td class="small muted">{item.incident.destination || '—'}{item.incident.cloud_delivery ? ' · cloud' : ''}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
      <p class="muted small">Only the masked excerpt is stored. The value the detector matched is never kept, so it cannot be shown here.</p>
    </section>
  );
}
