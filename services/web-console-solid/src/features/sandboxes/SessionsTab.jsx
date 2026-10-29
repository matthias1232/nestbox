// Who is signed in to this sandbox's agent, and who is connected to the sandbox.
//
// Two ways to sign in exist and they can coexist: the gateway key (the appliance's model
// endpoint, set in the environment and mediated by the supervisor) and the user's own
// account (stored on the persistent home). The tab says which is in use, checks what it
// can, and is honest about what it cannot: an account is checked for expiry, not asked.

import { Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';
import { usePolling } from '../../components/Poll.js';
import StatusBadge from '../../components/StatusBadge.jsx';
import LoginBadge from '../../components/LoginBadge.jsx';

export default function SessionsTab(props) {
  const [entry, setEntry] = createSignal(null);
  const [error, setError] = createSignal(null);
  const poll = usePolling(() => deck.loginSessions(), {
    everyMs: 15000,
    onData: (data) => {
      setEntry((data.items || []).find((item) => item.sandbox_id === props.sandbox.id) || null);
      setError(null);
    },
    onError: (failure) => setError(failure.message),
  });
  const login = () => entry()?.login;

  return (
    <section class="tab-panel supervisor-tab">
      <p class="explain">
        <b>Two ways to sign in.</b> The <b>gateway key</b> sends the agent's model calls through the
        appliance's model endpoint; the supervisor allows that connection and the key never has to be
        typed. An <b>own account</b> is a login you make yourself inside the sandbox; it is kept on the
        sandbox's persistent home, so it survives a restart or a recreate. If both are present the key is
        used and the account waits.
      </p>
      <div class="controls">
        <button class="secondary tiny" onClick={poll.refresh}>Refresh</button>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={!entry() && !error()}>
        <p class="muted">This sandbox is not running, or has not answered yet.</p>
      </Show>
      <Show when={entry()}>
        <div class="supervisor-grid">
          <div class="card">
            <h3>Agent login</h3>
            <p><LoginBadge entry={entry()} /></p>
            <p>{login().detail}</p>
            <dl class="facts">
              <dt>Mode</dt><dd>{login().mode.replace('_', ' ')}</dd>
              <dt>Check</dt><dd><StatusBadge status={login().check === 'ok' ? 'ok' : login().check === 'warn' ? 'warn' : 'denied'} label={login().check} /></dd>
              <Show when={login().account}>
                <dt>Account</dt><dd>{login().account.email || 'signed in (no address stored)'}</dd>
                <dt>Provider</dt><dd class="mono">{login().account.provider}</dd>
                <dt>Expires</dt>
                <dd>{login().account.expires_at_ms ? new Date(login().account.expires_at_ms).toLocaleString() : 'not stated'}</dd>
              </Show>
            </dl>
          </div>
          <div class="card">
            <h3>Connections</h3>
            <dl class="facts">
              <dt>OpenShell supervisor</dt>
              <dd><StatusBadge status={entry().openshell_connected ? 'ok' : 'denied'} label={entry().openshell_connected ? 'connected' : 'not connected'} /></dd>
              <dt>SSH sessions open</dt><dd>{entry().ssh_sessions}</dd>
              <dt>Browser terminals open</dt><dd>{entry().terminal_sessions}</dd>
            </dl>
            <p class="muted small">
              Sessions are recorded as who, when and how long; never what was typed.
            </p>
          </div>
        </div>
      </Show>
    </section>
  );
}
