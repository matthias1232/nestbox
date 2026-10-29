import { Show } from 'solid-js';

// How an agent is signed in, as one badge: gateway key, own account, both, or neither.
// The tooltip carries the sentence that explains what the badge means.

const LABELS = {
  gateway: 'gateway key',
  own_account: 'own account',
  both: 'key + account',
  none: 'not signed in',
  unknown: 'unknown',
};
const TONES = { ok: 'running', warn: 'degraded', error: 'denied', unknown: 'unknown' };

export default function LoginBadge(props) {
  const entry = () => props.entry;
  return (
    <Show when={entry()} fallback={<span class="muted small">—</span>}>
      <span class={`login-badge ${TONES[entry().login.check] || 'unknown'}`} title={entry().login.detail}>
        {LABELS[entry().login.mode] || entry().login.mode}
        {entry().login.account?.email ? ` · ${entry().login.account.email}` : ''}
      </span>
      <Show when={entry().ssh_sessions + entry().terminal_sessions > 0}>
        <span class="muted small" title="Open sessions: SSH and browser terminal">
          {' '}· {entry().ssh_sessions} ssh {entry().terminal_sessions} term
        </span>
      </Show>
    </Show>
  );
}

