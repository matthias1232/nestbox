// The model conversations of this sandbox, live: what each agent asked, what the
// model answered, and what it cost in tokens.
//
// Read from the proxy's recorded bodies, so it needs an egress mode that
// inspects TLS (`transparent` or `hybrid`). In the other modes the proxy only
// sees a tunnel to the model host and there is nothing to read; the page says so
// instead of staying empty without a reason.
import { For, Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';
import * as policies from '../../api/policies.js';
import { usePolling } from '../../components/Poll.js';
import { isModelCall, readRequest, readResponse } from './chats.js';

const PER_POLL = 6;

export default function ChatsTab(props) {
  const [turns, setTurns] = createSignal([]);
  const [error, setError] = createSignal(null);
  const [seenTunnel, setSeenTunnel] = createSignal(false);
  const [open, setOpen] = createSignal({});
  const [mode, setMode] = createSignal(null);
  const [switching, setSwitching] = createSignal(false);
  const inspects = () => ['transparent', 'hybrid'].includes(mode());
  policies.egressMode(props.sandbox.id).then((answer) => setMode(answer.mode)).catch(() => setMode(null));

  // The mode is fixed when the container is created, so changing it recreates
  // the sandbox. Persistent roots (the agent's login and history) survive; a
  // running program does not.
  async function inspect() {
    if (!window.confirm('Switch this sandbox to "transparent"? It is recreated: running programs end, persistent folders (including the agent login) are kept. Afterwards the proxy can read model calls.')) return;
    setSwitching(true);
    try {
      await policies.setEgressMode(props.sandbox.id, 'transparent');
      setMode('transparent');
    } catch (failure) {
      setError(failure.message);
    } finally {
      setSwitching(false);
    }
  }
  const cache = new Map();

  async function refresh() {
    const list = await deck.traffic(props.sandbox.id, { window: '24h', filter: 'all', limit: 100 });
    const items = list.items || [];
    setSeenTunnel(items.some((item) => item.channel === 'tunnel' && /anthropic|openai|openrouter|googleapis/.test(item.host || '')));
    const calls = items.filter((item) => isModelCall(item) && item.has_body);
    let fetched = 0;
    for (const item of calls) {
      if (cache.has(item.id) || fetched >= PER_POLL) continue;
      fetched += 1;
      try {
        const body = await deck.trafficBody(props.sandbox.id, item.id);
        const ask = readRequest(body.request_body);
        const answer = readResponse(body.response_body);
        cache.set(item.id, { id: item.id, ts: item.ts, host: item.host, path: item.path, status: item.status, truncated: body.truncated, ask, answer });
      } catch {
        cache.set(item.id, { id: item.id, ts: item.ts, host: item.host, path: item.path, status: item.status, ask: { prompt: '' }, answer: { text: '' }, unreadable: true });
      }
    }
    // Calls with neither a question nor an answer (a model that only "thought",
    // a title request) are bookkeeping of the client, not a conversation.
    setTurns(
      [...cache.values()]
        .filter((turn) => turn.unreadable || turn.ask.prompt || turn.answer.text || turn.answer.error)
        .sort((a, b) => String(b.ts).localeCompare(String(a.ts))),
    );
  }

  const poll = usePolling(refresh, { everyMs: 3000, onData: () => setError(null), onError: (e) => setError(e.message) });
  const toggle = (id) => setOpen({ ...open(), [id]: !open()[id] });

  return (
    <section class="tab-panel">
      <div class="controls">
        <button class={poll.paused() ? 'secondary tiny' : 'primary tiny'} onClick={poll.toggle}>{poll.paused() ? 'Paused' : 'Live'}</button>
        <span class="muted small">{turns().length} model calls in the last 24 h, newest first</span>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={mode() && !inspects()}>
        <p class="notice">
          This sandbox runs in <b>{mode()}</b> mode: the proxy only sees an encrypted tunnel to the model host, so a
          conversation (also one from a signed-in account) cannot be read here. In <b>transparent</b> or
          <b> hybrid</b> mode the proxy opens the connection with its own certificate and records it.
          {' '}<button class="tiny primary" disabled={switching()} onClick={inspect}>
            {switching() ? 'Switching…' : 'Switch to transparent'}
          </button>
        </p>
      </Show>
      <Show when={!turns().length}>
        <p class="muted">
          No model call has been recorded for this sandbox.
          <Show when={seenTunnel()}> The sandbox does reach a model host, but only through a tunnel the proxy cannot read: switch it to the <b>transparent</b> or <b>hybrid</b> egress mode under Policies and recreate it.</Show>
        </p>
      </Show>
      <For each={turns()}>
        {(turn) => (
          <article class="card" style="margin-bottom:.75rem">
            <header class="controls">
              <b>{turn.ask.model || turn.host}</b>
              <span class="muted small">{turn.ts} · {turn.host}{turn.path}</span>
              <span class={`badge ${turn.status >= 400 ? 'warn' : 'ok'}`}><i />{turn.status}</span>
              <Show when={turn.answer.tokens}><span class="muted small">{turn.answer.tokens.input ?? '?'} in · {turn.answer.tokens.output ?? '?'} out</span></Show>
              <Show when={turn.ask.history}><span class="muted small">{turn.ask.history} messages of history</span></Show>
              <Show when={turn.ask.partial || turn.truncated}><span class="muted small" title="Bodies are kept up to 64 KiB">shortened</span></Show>
              <button class="tiny secondary" onClick={() => toggle(turn.id)}>{open()[turn.id] ? 'Less' : 'More'}</button>
            </header>
            <Show when={turn.unreadable}><p class="muted small">The recorded body could not be read.</p></Show>
            <Show when={turn.ask.prompt}>
              <p><span class="muted small">asked</span><br /><span style="white-space:pre-wrap">{open()[turn.id] ? turn.ask.prompt : turn.ask.prompt.slice(0, 400)}</span></p>
            </Show>
            <Show when={turn.answer.text || turn.answer.error}>
              <p><span class="muted small">{turn.answer.error ? 'error' : 'answered'}</span><br /><span style="white-space:pre-wrap">{turn.answer.error || (open()[turn.id] ? turn.answer.text : turn.answer.text.slice(0, 600))}</span></p>
            </Show>
            <Show when={open()[turn.id] && turn.ask.system}>
              <p><span class="muted small">system</span><br /><span class="small" style="white-space:pre-wrap">{turn.ask.system.slice(0, 2000)}</span></p>
            </Show>
          </article>
        )}
      </For>
    </section>
  );
}
