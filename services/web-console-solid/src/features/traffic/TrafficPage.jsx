// The live traffic console.

import { For, Show, onMount, createSignal } from 'solid-js';
import TrafficTable from '../../components/TrafficTable.jsx';
import { useTrafficStream } from './useTrafficStream.js';
import * as policies from '../../api/policies.js';

const KINDS = [
  { id: '', label: 'All' },
  { id: 'network', label: 'Network' },
  { id: 'dns', label: 'DNS' },
  { id: 'inference', label: 'Model requests' },
];

export default function TrafficPage() {
  const traffic = useTrafficStream();
  const [draft, setDraft] = createSignal(null);
  const [draftError, setDraftError] = createSignal(null);

  onMount(() => traffic.start());

  async function draftRule(event) {
    setDraftError(null);
    try {
      const result = await policies.draftFromEvent(event, event.sandbox_id || null);
      setDraft(result);
    } catch (failure) {
      setDraftError(failure.message);
    }
  }

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Live traffic</h1>
          <p class="muted">
            {traffic.connected() ? 'streaming' : 'disconnected'} ·
            {' '}{traffic.events().length} events in view
            {traffic.paused() ? ` · ${traffic.pending().length} paused` : ''}
          </p>
        </div>
        <div class="controls">
          <button class={traffic.paused() ? 'primary' : 'secondary'} onClick={traffic.togglePause}>
            {traffic.paused() ? `Resume (${traffic.pending().length})` : 'Pause'}
          </button>
          <button class="secondary" onClick={traffic.clear}>Clear</button>
          <button class="secondary" onClick={traffic.seed}>Generate samples</button>
        </div>
      </header>

      <Show when={traffic.error()}>
        <p class="error">{traffic.error()}</p>
      </Show>
      <Show when={draftError()}>
        <p class="error">{draftError()}</p>
      </Show>

      <div class="facets">
        <For each={KINDS}>
          {(item) => (
            <button
              class={traffic.kind() === item.id ? 'chip active' : 'chip'}
              onClick={() => traffic.setKind(item.id)}
            >
              {item.label}
              {item.id && <b>{traffic.counts()[item.id]}</b>}
            </button>
          )}
        </For>
      </div>

      <Show
        when={traffic.visible().length > 0}
        fallback={
          <div class="empty">
            <strong>Waiting for events</strong>
            <p class="muted">
              A gateway reports traffic here as it happens. Without a connected
              gateway, “Generate samples” fills the view so the console can be
              evaluated.
            </p>
          </div>
        }
      >
        <TrafficTable events={traffic.visible()} onDraft={draftRule} />
      </Show>

      <Show when={draft()}>
        <div class="draft-card">
          <h3>Rule draft</h3>
          <p class="muted small">
            Nothing is enforced yet. A draft must be reviewed and approved, and
            only then does it become a rule.
          </p>
          <pre>{JSON.stringify(draft().draft, null, 2)}</pre>
          <div class="actions">
            <button onClick={async () => {
              await policies.decideDraft(draft().id, true, null);
              setDraft(null);
            }}>Approve and enforce</button>
            <button class="secondary" onClick={async () => {
              await policies.decideDraft(draft().id, false, 'rejected from console');
              setDraft(null);
            }}>Reject</button>
            <button class="ghost" onClick={() => setDraft(null)}>Dismiss</button>
          </div>
        </div>
      </Show>
    </section>
  );
}
