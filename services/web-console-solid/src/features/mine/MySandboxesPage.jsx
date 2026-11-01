// The end user's own corner: their sandboxes, and what is being stopped in them.
//
// Deliberately narrower than the admin console: only the sandboxes this person
// owns, with the actions that matter day to day — open, start, stop, terminal,
// desktop — and the blocked destinations of those sandboxes, which can be opened
// for a while or for good without leaving the page. Admins see this page too; it
// still shows their own sandboxes, not everybody's, because that is what it is for.
import { For, Show, createMemo, createSignal } from 'solid-js';
import * as sandboxes from '../../api/sandboxes.js';
import * as policies from '../../api/policies.js';
import * as trafficApi from '../../api/traffic.js';
import StatusBadge from '../../components/StatusBadge.jsx';
import { usePolling } from '../../components/Poll.js';
import { useSession } from '../../state/session.jsx';

const STATE_OF = { running: 'running', stopped: 'idle', failed: 'denied', created: 'degraded' };

export default function MySandboxesPage(props) {
  const session = useSession();
  const me = () => session.identity()?.user_id;
  const [items, setItems] = createSignal([]);
  const [blocked, setBlocked] = createSignal([]);
  const [busy, setBusy] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);

  const mine = createMemo(() => items().filter((item) => !me() || item.owner_user_id === me()));

  const poll = usePolling(
    async () => {
      const [list, decisions] = await Promise.all([
        sandboxes.list({ limit: 200 }),
        trafficApi.decisions({ window: '1h' }).catch(() => ({ items: [] })),
      ]);
      return { list, decisions };
    },
    {
      everyMs: 6000,
      onData: ({ list, decisions }) => {
        setItems(list.items || []);
        const own = new Set((list.items || []).filter((item) => !me() || item.owner_user_id === me()).map((item) => item.id));
        setBlocked((decisions.items || []).filter((item) => own.has(item.sandbox_id)));
        setError(null);
      },
      onError: (failure) => setError(failure.message),
    },
  );

  async function act(sandbox, action) {
    setBusy(`${sandbox.id}:${action}`);
    setError(null);
    try {
      await sandboxes.act(sandbox.id, action);
      setNotice(`${sandbox.name}: ${action} requested.`);
      await poll.refresh();
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(null);
    }
  }

  async function allow(item, mode, minutes = 15) {
    setBusy(`${item.sandbox_id}:${item.host}`);
    setError(null);
    setNotice(null);
    try {
      if (mode === 'permanent') {
        await policies.allowDestination(item.sandbox_id, {
          host: item.host,
          port: item.port,
          protocol: item.protocol,
        });
        setNotice(`${item.host}${item.port ? `:${item.port}` : ''} is allowed for good; the rule is in the sandbox's Policies tab.`);
      } else {
        await policies.openWindow(item.sandbox_id, minutes, 'opened from My sandboxes', 'bypass', 'all');
        setNotice(`${item.host} is open for ${minutes} minutes.`);
      }
      await poll.refresh();
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(null);
    }
  }

  const nameOf = (id) => items().find((item) => item.id === id)?.name || 'sandbox';

  return (
    <div class="page">
      <h2>My sandboxes</h2>
      <p class="muted small">
        Your own sandboxes and the destinations that were just stopped in them. Everything here
        applies only to sandboxes you own.
      </p>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <Show when={mine().length} fallback={<p class="muted">You do not own a sandbox yet. Create one under Sandboxes.</p>}>
        <div class="controls" style="flex-wrap:wrap;gap:.75rem">
          <For each={mine()}>
            {(sandbox) => (
              <div class="card" style="min-width:280px;flex:1">
                <div class="controls">
                  <b>{sandbox.name}</b>
                  <StatusBadge status={STATE_OF[sandbox.status] || 'degraded'} label={sandbox.status} />
                  <span class="muted small">{sandbox.pack_key}</span>
                </div>
                <div class="controls" style="flex-wrap:wrap;margin-top:.5rem">
                  <button class="tiny primary" onClick={() => props.onOpen?.(sandbox.id)}>Open</button>
                  <button class="tiny secondary" onClick={() => props.onOpen?.(sandbox.id, 'terminal')}>Terminal</button>
                  <button class="tiny secondary" onClick={() => props.onOpen?.(sandbox.id, 'desktop')}>Desktop</button>
                  <button class="tiny secondary" onClick={() => props.onOpen?.(sandbox.id, 'chats')}>Chats</button>
                  <button class="tiny secondary" onClick={() => props.onOpen?.(sandbox.id, 'usage')}>Usage</button>
                  <Show when={sandbox.status === 'running'} fallback={
                    <button class="tiny" disabled={busy() === `${sandbox.id}:start`} onClick={() => act(sandbox, 'start')}>Start</button>
                  }>
                    <button class="tiny danger" disabled={busy() === `${sandbox.id}:stop`} onClick={() => act(sandbox, 'stop')}>Stop</button>
                  </Show>
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>

      <h3>Stopped in your sandboxes (last hour)</h3>
      <Show when={blocked().length} fallback={<p class="muted small">Nothing is blocked right now.</p>}>
        <div class="table-scroll">
          <table class="dense">
            <thead><tr><th>Sandbox</th><th>Destination</th><th class="num">Attempts</th><th>Why</th><th /></tr></thead>
            <tbody>
              <For each={blocked()}>
                {(item) => (
                  <tr>
                    <td class="mono small">{nameOf(item.sandbox_id)}</td>
                    <td class="mono small">{item.host}{item.port ? `:${item.port}` : ''} <span class="muted">{item.protocol}</span></td>
                    <td class="num">{item.count ?? ''}</td>
                    <td class="small muted">{item.reason || item.reason_text || ''}</td>
                    <td class="actions">
                      <button class="tiny primary" disabled={busy() === `${item.sandbox_id}:${item.host}`}
                        onClick={() => allow(item, 'permanent')}>Allow</button>
                      <button class="tiny secondary" disabled={busy() === `${item.sandbox_id}:${item.host}`}
                        onClick={() => allow(item, 'minutes', 15)}>15 min</button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>
    </div>
  );
}
