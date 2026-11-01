// The agents' conversations in this sandbox, live: what each session asked and
// what the model answered, the newest session first.
//
// Read from the proxy's recorded bodies, so it needs an egress mode that opens
// agent TLS (`transparent` or `hybrid`, or model traffic through the accounting
// proxy). In the tunnel-only modes the page says why there is nothing to read
// instead of staying empty without a reason.
//
// The egress mode comes from the `control` prop when the page passes the
// control summary; otherwise the panel reads it itself.
import { For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import * as deck from '../../api/deck.js';
import * as policies from '../../api/policies.js';
import { stream } from '../../api/telemetry.js';
import { usePolling } from '../../components/Poll.js';
import { isModelCall } from '../sandboxes/chats.js';
import { PER_REFRESH, clip, flatten, groupTurns, hasMore, turnOf } from './livechats.js';

const PROMPT_CHARS = 600;
const ANSWER_CHARS = 1200;
const TURNS_PER_GROUP = 3;
const DEBOUNCE_MS = 1000;
const POLL_MS = 20000;
// Modes in which the proxy only sees an encrypted tunnel to the model host.
const TUNNEL_MODES = ['forward_proxy', 'off_allow_all'];

// The panel: a Live/Paused toggle, the reason when the mode hides chats, and one
// card per conversation with its last three turns.
export default function LiveChatsPanel(props) {
  const [turns, setTurns] = createSignal([]);
  const [error, setError] = createSignal(null);
  const [mode, setMode] = createSignal(null);
  const [open, setOpen] = createSignal({});
  const cache = new Map();
  let timer = null;

  const sandboxId = () => props.sandbox?.id;
  const egressMode = () => props.control?.egress?.mode ?? mode();
  const readable = () => !TUNNEL_MODES.includes(egressMode());

  async function refresh() {
    const id = sandboxId();
    if (!id) return;
    const list = await deck.traffic(id, { window: '24h', filter: 'all', limit: 100 });
    const items = list.items || [];
    const calls = items.filter((item) => isModelCall(item) && item.has_body);
    // A body that was read once never costs a second request. Only the newest
    // calls still missing are fetched, a few per refresh like the deck's tab.
    let fetched = 0;
    for (const item of calls) {
      if (cache.has(item.id) || fetched >= PER_REFRESH) continue;
      fetched += 1;
      try {
        cache.set(item.id, turnOf(item, await deck.trafficBody(id, item.id)));
      } catch {
        cache.set(item.id, turnOf(item, null));
      }
    }
    // Drop what the 24 h window no longer contains, so a long-lived page does
    // not keep old conversations in memory.
    const live = new Set(calls.map((item) => item.id));
    for (const id of [...cache.keys()]) if (!live.has(id)) cache.delete(id);
    setTurns(calls.map((item) => cache.get(item.id)).filter(Boolean));
  }

  const poll = usePolling(refresh, {
    everyMs: POLL_MS,
    onData: () => setError(null),
    onError: (failure) => setError(failure.message),
  });

  // One telemetry event for this sandbox means a model call just happened. The
  // refresh waits a second so a burst of events costs one read.
  function schedule() {
    if (poll.paused()) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      poll.refresh();
    }, DEBOUNCE_MS);
  }

  onMount(() => {
    const stop = stream(null, {
      onEvent: (event) => {
        if (event.sandbox_id && event.sandbox_id === sandboxId()) schedule();
      },
    });
    onCleanup(stop);
    const id = sandboxId();
    if (id && !props.control) {
      policies.egressMode(id).then((answer) => setMode(answer.mode)).catch(() => setMode(null));
    }
  });
  onCleanup(() => timer && clearTimeout(timer));

  const groups = () => groupTurns(turns());

  const turnKey = (turn, part) => `${turn.id}:${part}`;
  const isOpen = (turn, part) => Boolean(open()[turnKey(turn, part)]);
  const toggle = (turn, part) => setOpen({ ...open(), [turnKey(turn, part)]: !isOpen(turn, part) });
  // The full text once "show more" was clicked, the short form before.
  const shown = (text, n, turn, part) => (isOpen(turn, part) ? flatten(text) : clip(text, n));

  const label = (group) =>
    group.title ||
    group.model ||
    (group.session === 'none' ? 'Calls without a session' : `Session ${group.session.slice(0, 8)}`);

  return (
    <section>
      <div class="controls">
        <h3 style="margin:0">Live chats</h3>
        <button class={poll.paused() ? 'secondary tiny' : 'primary tiny'} onClick={poll.toggle}>
          {poll.paused() ? 'Paused' : 'Live'}
        </button>
        <span class="muted small">{turns().length} model calls in the last 24 h</span>
      </div>

      <Show when={error()}><p class="error">{error()}</p></Show>

      <Show when={egressMode() && !readable()}>
        <p class="notice">
          Chats are not readable in this proxy mode. Choose 'Only agent chats' or 'Open everything' in the
          Network card.
        </p>
      </Show>

      <Show when={readable() && !groups().length}>
        <p class="muted">
          No model call has been recorded for this sandbox in the last 24 h.
        </p>
      </Show>

      <For each={groups()}>
        {(group) => (
          <article class="card" style="margin-bottom:1rem">
            <header class="controls">
              <b>{label(group)}</b>
              <span class="muted small">
                {group.turns.length} calls · last {String(group.last_ts).slice(11, 19)}
                {group.session !== 'none' ? ` · session ${group.session.slice(0, 8)}` : ''}
              </span>
            </header>

            <For each={group.turns.slice(0, TURNS_PER_GROUP)}>
              {(turn) => (
                <div style="margin-top:.75rem;border-top:1px solid var(--border);padding-top:.5rem">
                  <div class="controls">
                    <span class="mono small">{turn.ask.model || turn.host}</span>
                    <span class="muted small">{String(turn.ts).slice(11, 19)}</span>
                    <Show when={turn.status}>
                      <span class={`badge ${turn.status >= 400 ? 'warn' : 'ok'}`}><i />{turn.status}</span>
                    </Show>
                    <Show when={turn.answer?.tokens}>
                      <span class="muted small">
                        {turn.answer.tokens.input ?? '?'} in · {turn.answer.tokens.output ?? '?'} out
                      </span>
                    </Show>
                  </div>

                  <Show when={turn.unreadable}>
                    <p class="muted small">The recorded body could not be read.</p>
                  </Show>

                  <Show when={turn.ask?.prompt}>
                    <p>
                      <span class="muted small">asked</span><br />
                      <span style="white-space:pre-wrap">{shown(turn.ask.prompt, PROMPT_CHARS, turn, 'ask')}</span>{' '}
                      <Show when={hasMore(turn.ask.prompt, PROMPT_CHARS)}>
                        <button class="tiny ghost" onClick={() => toggle(turn, 'ask')}>
                          {isOpen(turn, 'ask') ? 'show less' : 'show more'}
                        </button>
                      </Show>
                    </p>
                  </Show>

                  <Show when={turn.answer?.text || turn.answer?.error}>
                    <p>
                      <span class="muted small">{turn.answer.error ? 'error' : 'answered'}</span><br />
                      <span style="white-space:pre-wrap">
                        {shown(turn.answer.error || turn.answer.text, ANSWER_CHARS, turn, 'answer')}
                      </span>{' '}
                      <Show when={hasMore(turn.answer.error || turn.answer.text, ANSWER_CHARS)}>
                        <button class="tiny ghost" onClick={() => toggle(turn, 'answer')}>
                          {isOpen(turn, 'answer') ? 'show less' : 'show more'}
                        </button>
                      </Show>
                    </p>
                  </Show>
                </div>
              )}
            </For>

            <Show when={group.turns.length > TURNS_PER_GROUP}>
              <p class="muted small">
                {group.turns.length - TURNS_PER_GROUP} older calls of this session are not shown.
              </p>
            </Show>
          </article>
        )}
      </For>
    </section>
  );
}
