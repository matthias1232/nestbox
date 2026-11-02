// Four small buttons that allow one destination for a while.
//
// The rule is a normal allow rule with an end, so the same service creates it and
// the reconcile pass deletes it when the time is up. "Forever" is the one choice
// that outlives the day, so it asks once before it writes.
//
// This is the one component that calls the API itself, against the "no fetching"
// rule of `components/AGENTS.md`: it is a self-contained control dropped into a
// row, and its caller only needs to know that something changed.
import { For, Show, createSignal } from 'solid-js';

import * as policies from '../api/policies.js';
import ConfirmDialog from './ConfirmDialog.jsx';

const DURATIONS = [
  { label: '15 min', minutes: 15 },
  { label: '1 h', minutes: 60 },
  { label: '24 h', minutes: 1440 },
];

// "HH:MM" in the reader's own clock, because a time from the server is UTC and
// an operator comparing it to their watch is reading local time.
function clock(iso) {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return null;
  return at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
}

/// What the success line says for a returned rule: its end, or that it has none.
export function grantedText(rule) {
  if (!rule?.expires_at) return 'Dauerhaft freigegeben';
  const time = clock(rule.expires_at);
  return time ? `Freigegeben bis ${time}` : 'Freigegeben';
}

export default function QuickAllow(props) {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);
  const [done, setDone] = createSignal(null);
  const [asking, setAsking] = createSignal(false);

  /// Create the rule. `minutes` null is the permanent rule and only reaches here
  /// after the dialog was answered.
  async function grant(minutes) {
    setAsking(false);
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const rule = await policies.quickAllow(props.sandboxId, props.host, props.port ?? null, minutes);
      setDone(grantedText(rule));
      props.onDone?.(rule);
    } catch (failure) {
      setError(failure.message || 'Die Freigabe konnte nicht angelegt werden.');
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div class="controls">
        <For each={DURATIONS}>
          {(item) => (
            <button class="tiny secondary" disabled={busy()} onClick={() => grant(item.minutes)}>
              {item.label}
            </button>
          )}
        </For>
        <button class="tiny secondary" disabled={busy()} onClick={() => setAsking(true)}>Immer</button>
      </div>
      <Show when={error()}>
        <p class="error small">{error()}</p>
      </Show>
      <Show when={done()}>
        <p class="muted small">{done()}</p>
      </Show>

      <ConfirmDialog
        open={asking()}
        title={`${props.host} dauerhaft freigeben?`}
        onClose={() => setAsking(false)}
        actions={[
          { label: 'Dauerhaft freigeben', tone: 'danger', onClick: () => grant(null) },
          { label: 'Abbrechen', tone: 'secondary', onClick: () => setAsking(false) },
        ]}
      >
        <p>
          Die Regel bleibt, bis jemand sie unter Policies → Regeln entfernt. Für einen
          Test reicht in der Regel eine der befristeten Freigaben.
        </p>
      </ConfirmDialog>
    </div>
  );
}
