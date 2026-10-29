// MCP servers of this sandbox: what is attached, whether it answers, which tools
// it offers, and the calls the proxy saw on the wire.
import { For, Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';
import StatusBadge from '../../components/StatusBadge.jsx';
import { usePolling } from '../../components/Poll.js';

export default function McpTab(props) {
  const id = () => props.sandbox.id;
  const [attached, setAttached] = createSignal([]);
  const [registry, setRegistry] = createSignal([]);
  const [calls, setCalls] = createSignal([]);
  const [choice, setChoice] = createSignal('');
  const [error, setError] = createSignal(null);
  const [message, setMessage] = createSignal(null);

  const poll = usePolling(async () => {
    const [a, r, c] = await Promise.all([deck.mcpAttached(id()), deck.mcpRegistry(), deck.mcpCalls(id(), 30).catch(() => ({ items: [] }))]);
    return { a, r, c };
  }, { everyMs: 5000, onData: ({ a, r, c }) => { setAttached(a.items || []); setRegistry(r.items || []); setCalls(c.items || []); setError(null); }, onError: (e) => setError(e.message) });

  async function run(action, done) {
    setError(null); setMessage(null);
    try { await action(); if (done) setMessage(done); await poll.refresh(); } catch (e) { setError(e.message); }
  }
  const free = () => registry().filter((s) => !attached().some((a) => a.server_key === s.key) && s.enabled !== false);
  const toolCount = (s) => (Array.isArray(s.tools) ? s.tools.length : 0);

  return (
    <section class="tab-panel">
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={message()}><p class="notice">{message()}</p></Show>
      <div class="controls">
        <select value={choice()} onChange={(e) => setChoice(e.currentTarget.value)}>
          <option value="">attach a server…</option>
          <For each={free()}>{(s) => <option value={s.key}>{s.name} ({s.transport})</option>}</For>
        </select>
        <button class="primary tiny" disabled={!choice()} onClick={() => run(() => deck.mcpAttach(id(), choice()), 'Attached. Run “Discover tools” to read what it offers.')}>Attach</button>
      </div>

      <Show when={attached().length} fallback={<p class="muted">No MCP server is attached to this sandbox.</p>}>
        <div class="table-scroll"><table>
          <thead><tr><th>Server</th><th>Status</th><th>Tools</th><th>Last seen</th><th /></tr></thead>
          <tbody>
            <For each={attached()}>
              {(s) => (
                <tr>
                  <td class="mono">{s.server_key}<Show when={s.last_error}><small class="muted block">{s.last_error}</small></Show></td>
                  <td><StatusBadge status={s.enabled ? s.status : 'stopped'} label={s.enabled ? s.status : 'disabled'} /></td>
                  <td class="small">{toolCount(s)}<Show when={toolCount(s)}><small class="muted block">{s.tools.slice(0, 6).map((t) => t.name).join(', ')}</small></Show></td>
                  <td class="small">{s.last_seen_at ? String(s.last_seen_at).slice(0, 19) : '—'}</td>
                  <td class="actions">
                    <button class="tiny" onClick={() => run(() => deck.mcpDiscover(id(), s.server_key), 'Discovery finished.')}>Discover tools</button>
                    <button class="tiny" onClick={() => run(() => deck.mcpDetach(id(), s.server_key), 'Detached.')}>Detach</button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table></div>
      </Show>

      <h3>Calls seen on the wire</h3>
      <Show when={calls().length} fallback={<p class="muted small">No MCP call has crossed the proxy yet.</p>}>
        <table class="dense"><thead><tr><th>Time</th><th>Server</th><th>Method</th><th>Tool</th><th>ms</th><th /></tr></thead><tbody>
          <For each={calls()}>
            {(c) => <tr class={c.is_error ? 'row-deny' : ''}><td class="mono small">{String(c.ts).slice(11, 19)}</td><td class="mono small">{c.server_key || c.server_host}</td><td class="mono small">{c.method}</td><td class="mono small">{c.tool_name || ''}</td><td class="mono small">{c.duration_ms ?? ''}</td><td class="small">{c.is_error ? 'error' : ''}</td></tr>}
          </For>
        </tbody></table>
      </Show>
    </section>
  );
}
