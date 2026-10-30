// The live traffic console: every sandbox, the last window, and what arrives.
//
// The page is a read view with one write: a blocked request can become a rule
// draft. It is never a rule — a draft is reviewed and approved (ADR 0011) — and
// the dialog says so, because a one-click allow would be the wrong promise for
// an audit trail.

import { For, Show, createMemo, onMount, createSignal } from 'solid-js';
import TrafficTable from '../../components/TrafficTable.jsx';
import { DECISIONS, KINDS, WINDOWS, useTrafficStream } from './useTrafficStream.js';
import * as policies from '../../api/policies.js';

export default function TrafficPage() {
  const traffic = useTrafficStream();
  const [draft, setDraft] = createSignal(null);
  const [draftError, setDraftError] = createSignal(null);

  onMount(() => {
    traffic.load();
    traffic.start();
  });

  /** The sandbox filter, with names resolved once from the same request. */
  const sandboxOptions = createMemo(() =>
    Object.entries(traffic.names())
      .map(([id, name]) => ({ id, name }))
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  async function draftRule(row) {
    setDraftError(null);
    try {
      const result = await policies.draftFromEvent(
        { host: row.host, port: row.port, protocol: row.protocol, method: row.method },
        row.sandboxId || null,
      );
      setDraft(result);
    } catch (failure) {
      setDraftError(failure.message);
    }
  }

  const summary = () => traffic.summary();

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Live traffic</h1>
          <p class="muted">
            {traffic.connected() ? 'streaming' : 'not streaming'} ·
            {' '}last {traffic.windowSpan()} · {traffic.visible().length} rows in view
            {traffic.paused() ? ` · ${traffic.pending().length} held while paused` : ''}
          </p>
        </div>
        <div class="controls">
          <select
            value={traffic.windowSpan()}
            onChange={(event) => {
              traffic.setWindowSpan(event.currentTarget.value);
              traffic.load();
            }}
          >
            <For each={WINDOWS}>{(span) => <option value={span}>last {span}</option>}</For>
          </select>
          <button class="secondary" onClick={traffic.load} disabled={traffic.loading()}>
            {traffic.loading() ? 'Reading…' : 'Reload window'}
          </button>
          <button class={traffic.paused() ? 'primary' : 'secondary'} onClick={traffic.togglePause}>
            {traffic.paused() ? `Resume (${traffic.pending().length})` : 'Pause'}
          </button>
          <button class="secondary" onClick={traffic.clear}>Clear</button>
        </div>
      </header>

      <Show when={traffic.error()}>
        <p class="error">{traffic.error()}</p>
      </Show>
      <Show when={draftError()}>
        <p class="error">{draftError()}</p>
      </Show>

      <Show when={summary()}>
        <p class="muted small">
          {summary().requests} requests · {summary().allowed} allowed ·{' '}
          <b>{summary().denied} blocked</b> · {summary().audit} audit ·{' '}
          {summary().destinations} destinations
        </p>
      </Show>

      <div class="facets">
        <For each={KINDS}>
          {(item) => (
            <button
              class={traffic.kind() === item.id ? 'chip active' : 'chip'}
              onClick={() => traffic.setKind(item.id)}
            >
              {item.label}
              <Show when={item.id}>
                <b>{traffic.counts()[item.id]}</b>
              </Show>
            </button>
          )}
        </For>
      </div>

      <div class="facets">
        <For each={DECISIONS}>
          {(item) => (
            <button
              class={traffic.decision() === item.id ? 'chip active' : 'chip'}
              onClick={() => traffic.applyDecision(item.id)}
            >
              {item.label}
              <Show when={item.id === 'blocked'}>
                <b>{traffic.counts().blocked}</b>
              </Show>
            </button>
          )}
        </For>
        <select
          value={traffic.sandbox()}
          onChange={(event) => traffic.applySandbox(event.currentTarget.value)}
        >
          <option value="">All sandboxes</option>
          <For each={sandboxOptions()}>
            {(item) => <option value={item.id}>{item.name}</option>}
          </For>
        </select>
        <input
          class="search"
          placeholder="Search host, sandbox, tool or reason"
          value={traffic.host()}
          onInput={(event) => traffic.setHost(event.currentTarget.value)}
        />
      </div>

      <Show
        when={traffic.visible().length > 0}
        fallback={
          <div class="empty">
            <strong>
              {traffic.loading() ? 'Reading the window…' : 'No request in this window'}
            </strong>
            <p class="muted">
              The table shows the last {traffic.windowSpan()} that were recorded, and then
              appends what arrives. An idle sandbox is quiet, not broken: run something that
              reaches the network and the row appears within seconds.
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
            Nothing is enforced yet. A draft must be reviewed and approved, and only then does
            it become a rule.
          </p>
          <pre>{JSON.stringify(draft().draft, null, 2)}</pre>
          <div class="actions">
            <button
              onClick={async () => {
                await policies.decideDraft(draft().id, true, null);
                setDraft(null);
              }}
            >
              Approve and enforce
            </button>
            <button
              class="secondary"
              onClick={async () => {
                await policies.decideDraft(draft().id, false, 'rejected from console');
                setDraft(null);
              }}
            >
              Reject
            </button>
            <button class="ghost" onClick={() => setDraft(null)}>Dismiss</button>
          </div>
        </div>
      </Show>
    </section>
  );
}
