// The selected agent: who it is, what it is doing, what it cost, and what it
// just did. The log is the stream's own history, bounded in the hook.

import { For, Show } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import { kindClass, kindLabel } from './kinds.js';
import { costMicros5m, requests5m, tokens5m } from './metrics.js';
import { formatCost, formatCount, formatTime, shortenTarget, truncate } from './format.js';

const EVENT_TONE = {
  ok: 'ok', allowed: 'ok', success: 'ok', succeeded: 'ok',
  pending: 'warn', retried: 'warn', throttled: 'warn',
  denied: 'bad', blocked: 'bad', error: 'bad', failed: 'bad',
};

function eventTone(status) {
  return EVENT_TONE[String(status || '').toLowerCase()] || 'muted';
}

export default function AgentPanel(props) {
  const agent = () => props.agent;

  return (
    <aside class="live-panel" aria-label="Selected agent">
      <Show
        when={agent()}
        fallback={(
          <div class="live-panel-empty">
            <h2>Agent</h2>
            <p class="muted small">Select an agent node in the map to see its activity.</p>
          </div>
        )}
      >
        <header class="live-panel-head">
          <div>
            <h2>{agent().agent}</h2>
            <p class="muted small mono" title={agent().sandbox}>
              {truncate(agent().sandbox, 34)}
            </p>
          </div>
          <StatusBadge
            status={agent().status}
            label={agent().status === 'unknown' ? 'not in snapshot' : undefined}
          />
        </header>

        <dl class="facts live-facts">
          <dt>Pack</dt>
          <dd>{agent().pack || <span class="muted">not reported</span>}</dd>
          <dt>Current</dt>
          <dd class="live-current">
            <Show when={agent().current} fallback={<span class="muted">Idle between requests</span>}>
              <span class={`kind-chip ${kindClass(agent().current.kind)}`}>
                {kindLabel(agent().current.kind)}
              </span>
              <span class="live-current-target mono" title={agent().current.target}>
                {shortenTarget(agent().current.target, 46) || '(no target)'}
              </span>
              <Show when={agent().current.model}>
                <span class="muted small"> · {agent().current.model}</span>
              </Show>
            </Show>
          </dd>
        </dl>

        <div class="live-metrics">
          <div>
            <span>Requests · 5 min</span>
            <b>{formatCount(requests5m(agent()))}</b>
          </div>
          <div>
            <span>Tokens · 5 min</span>
            <b>{tokens5m(agent()) === null ? 'n/a' : formatCount(tokens5m(agent()))}</b>
          </div>
          <div>
            <span>Cost · 5 min</span>
            <b>{formatCost(costMicros5m(agent()))}</b>
          </div>
        </div>

        <h3>Last events</h3>
        <Show
          when={agent().events.length > 0}
          fallback={<p class="muted small">No events from this agent yet.</p>}
        >
          <ol class="live-log">
            <For each={agent().events}>
              {(event) => (
                <li>
                  <div class="live-log-top">
                    <time datetime={typeof event.at === 'string' ? event.at : undefined}>
                      {formatTime(event.at)}
                    </time>
                    <span class={`kind-chip ${kindClass(event.kind)}`}>
                      {kindLabel(event.kind)}
                    </span>
                    <span class={`live-status ${eventTone(event.status)}`}>
                      {event.status || 'unreported'}
                    </span>
                  </div>
                  <div class="live-log-target mono" title={event.target}>
                    {shortenTarget(event.target, 64) || '(no target)'}
                  </div>
                  <div class="live-log-meta muted small">
                    <span>{event.model || 'no model'}</span>
                    <span>{event.tokens === null ? 'tokens n/a' : `${formatCount(event.tokens)} tok`}</span>
                    <span>{event.cost_micros === null ? 'cost n/a' : formatCost(event.cost_micros)}</span>
                  </div>
                </li>
              )}
            </For>
          </ol>
        </Show>
      </Show>
    </aside>
  );
}
