// Every request this sandbox made, live: what it asked for, what the proxy
// decided, and why. A blocked request can become a rule in two clicks.
import { For, Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';
import * as policies from '../../api/policies.js';
import Countdown from '../../components/Countdown.jsx';
import QuickAllow, { grantedText } from '../../components/QuickAllow.jsx';
import StatusBadge from '../../components/StatusBadge.jsx';
import { usePolling } from '../../components/Poll.js';

const WINDOWS = ['5m', '15m', '1h', '6h', '24h'];

export default function TrafficTab(props) {
  const [data, setData] = createSignal(null);
  const [windowSpan, setWindowSpan] = createSignal('15m');
  const [filter, setFilter] = createSignal('all');
  const [error, setError] = createSignal(null);
  const [message, setMessage] = createSignal(null);
  const [undo, setUndo] = createSignal(null);
  const [quick, setQuick] = createSignal([]);
  const [quickError, setQuickError] = createSignal(null);
  const poll = usePolling(
    () => deck.traffic(props.sandbox.id, { window: windowSpan(), filter: filter(), limit: 100 }),
    { everyMs: 4000, onData: (d) => { setData(d); setError(null); }, onError: (e) => setError(e.message) },
  );

  // The rules that still have time on them. Their list is polled slower than the
  // traffic: a countdown ticks itself, and a new rule arrives with a click.
  const quickPoll = usePolling(
    () => policies.quickAllowList(props.sandbox.id),
    {
      everyMs: 30000,
      onData: (d) => { setQuick(Array.isArray(d) ? d : d?.items || []); setQuickError(null); },
      onError: (e) => setQuickError(e.message),
    },
  );

  // One click: the rule is created enabled and the decider reads it on its next
  // question (the proxy caches an answer for a few seconds). Undo deletes it.
  async function allow(event, anyPort = false) {
    setMessage(null);
    setError(null);
    try {
      const rule = await policies.allowDestination(props.sandbox.id, {
        host: event.host,
        port: anyPort ? null : event.port,
        protocol: event.channel === 'tunnel' ? 'tcp' : event.protocol,
      });
      setUndo(rule);
      setMessage(`Allowed ${rule.host}${rule.port ? `:${rule.port}` : ''} for this sandbox. It is active within a few seconds and listed under Policies → Rules that apply.`);
      poll.refresh();
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function revert() {
    const rule = undo();
    if (!rule) return;
    try {
      await policies.removeRule(rule.id);
      setUndo(null);
      setMessage(`Rule for ${rule.host} removed again.`);
    } catch (failure) {
      setError(failure.message);
    }
  }

  // A quick allow is an ordinary rule with an end, so the list of windows and the
  // traffic both learn about it at once. The line at the top keeps the answer
  // visible after the row it came from was redrawn by the next poll.
  function quickAllowed(rule) {
    setUndo(null);
    setMessage(grantedText(rule));
    quickPoll.refresh();
    poll.refresh();
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
          <option value="all">all requests</option><option value="blocked">blocked only</option><option value="allowed">allowed only</option>
        </select>
        <Show when={s()}>
          <span class="muted small">{s().requests} requests · {s().allowed} allowed · <b>{s().denied} blocked</b> · {s().destinations} destinations</span>
        </Show>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={message()}><p class="notice">{message()} <Show when={undo()}><button class="tiny secondary" onClick={revert}>Undo</button></Show></p></Show>
      <Show when={quick().length}>
        <div class="controls" style="flex-wrap:wrap">
          <span class="muted small">Befristete Freigaben</span>
          <For each={quick()}>
            {(rule) => (
              <span class="badge info" title="Wird beim Ablauf automatisch entfernt">
                <span class="mono">{rule.host}{rule.port ? `:${rule.port}` : ''}</span>
                <Countdown until={rule.expires_at} onExpire={() => quickPoll.refresh()} />
              </span>
            )}
          </For>
        </div>
      </Show>
      <Show when={quickError()}><p class="error small">{quickError()}</p></Show>
      <Show when={(data()?.items || []).length} fallback={<p class="muted">No request in this window. Run something in the sandbox that reaches the network and it appears here within seconds.</p>}>
        <div class="table-scroll">
          <table class="dense">
            <thead><tr><th>Time</th><th>Decision</th><th>Destination</th><th>Request</th><th>Status</th><th>Content check</th><th>Why</th><th /></tr></thead>
            <tbody>
              <For each={data().items}>
                {(e) => (
                  <tr class={e.decision === 'deny' ? 'row-deny' : ''}>
                    <td class="mono small">{String(e.ts).slice(11, 19)}</td>
                    <td><StatusBadge status={e.decision === 'allow' ? 'running' : e.decision === 'audit' ? 'degraded' : 'denied'} label={e.decision} /></td>
                    <td class="mono small">{e.host}:{e.port}</td>
                    <td class="mono small">{e.method || (e.inspected ? '' : 'tunnel')} {e.path || ''}{e.mcp_tool ? ` · tool ${e.mcp_tool}` : ''}</td>
                    <td class="mono small">{e.status ?? '—'}</td>
                    <td>{dlpBadge(e.dlp)}</td>
                    <td class="small muted">{e.reason}</td>
                    <td>
                      <Show when={e.decision === 'deny'}>
                        <div class="controls" style="flex-wrap:wrap">
                          <button class="tiny primary" onClick={() => allow(e)}>Allow</button>
                          <button class="tiny secondary" title="Allow this host on every port" onClick={() => allow(e, true)}>any port</button>
                          <QuickAllow
                            sandboxId={props.sandbox.id}
                            host={e.host}
                            port={e.port}
                            onDone={quickAllowed}
                          />
                        </div>
                      </Show>
                    </td>
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

// What the content check did with this request, in the words a person uses.
// `blocked` means the answer never reached the model: that is a different fact
// from a network denial, and the one a user has to know about.
const DLP_LABELS = {
  blocked: { label: 'not delivered', tone: 'error' },
  replaced: { label: 'rewritten', tone: 'warn' },
  pseudonymized: { label: 'pseudonymised', tone: 'warn' },
  unavailable: { label: 'check failed', tone: 'error' },
  too_large: { label: 'too large', tone: 'error' },
};

function dlpBadge(value) {
  const entry = DLP_LABELS[value];
  if (!entry) return null;
  return <span class={`badge ${entry.tone}`}><i />{entry.label}</span>;
}
