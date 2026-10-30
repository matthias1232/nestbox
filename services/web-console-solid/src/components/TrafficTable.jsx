// A dense event table for the live console.
//
// Deliberately plain tables rather than a virtual list: at the row counts the
// console keeps (a few hundred in view), a table is faster to render and far
// easier to make accessible. The buffer is capped by the caller.
//
// The rows are already normalised by `useTrafficStream`, so this component knows
// one shape and no domain rule. Every row names its sandbox: the question this
// table answers is "what is happening, and where", and an id alone does not
// answer it.

import { Show } from 'solid-js';
import StatusBadge from './StatusBadge.jsx';

function formatTime(ts) {
  if (!ts) return '—';
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return '—';
  return (
    date.toLocaleTimeString([], { hour12: false }) +
    '.' +
    String(date.getMilliseconds()).padStart(3, '0')
  );
}

// A binary path is long and the interesting part is the end.
function shortBinary(path) {
  if (!path) return null;
  const parts = String(path).split('/');
  return parts[parts.length - 1];
}

/** What the request was, in the little space a dense row has. */
function request(row) {
  if (row.kind === 'dns') return 'name lookup';
  if (row.tool) return `tool ${row.tool}`;
  if (row.method || row.path) return `${row.method || ''} ${row.path || ''}`.trim();
  return row.channel === 'tunnel' ? 'tunnel' : '—';
}

export default function TrafficTable(props) {
  return (
    <div class="table-scroll">
      <table class="dense traffic">
        <thead>
          <tr>
            <th class="narrow">Time</th>
            <th>Sandbox</th>
            <th>Decision</th>
            <th class="narrow">Channel</th>
            <th>Destination</th>
            <th>Request</th>
            <th>Why</th>
            <th class="narrow" />
          </tr>
        </thead>
        <tbody>
          {props.events.map((row) => (
            <tr class={row.decision === 'deny' ? 'row-deny' : ''}>
              <td class="mono small">{formatTime(row.ts)}</td>
              <td class="small">
                <span class="origin">{row.sandbox}</span>
                <Show when={shortBinary(row.binary)}>
                  <small class="muted block mono">{shortBinary(row.binary)}</small>
                </Show>
              </td>
              <td>
                <StatusBadge
                  status={row.decision === 'allow' ? 'running' : row.decision === 'audit' ? 'degraded' : 'denied'}
                  label={row.decision}
                />
              </td>
              <td class="narrow small">{row.channel || '—'}</td>
              <td class="mono small">
                {row.host}
                {row.port ? `:${row.port}` : ''}
                {row.country ? <small class="muted block">{row.country}</small> : null}
              </td>
              <td class="mono small">{request(row)}</td>
              <td class="small muted">{row.reason || '—'}</td>
              <td class="narrow">
                <Show when={row.decision === 'deny'}>
                  <button
                    class="tiny"
                    onClick={() => props.onDraft?.(row)}
                    title="Draft a rule from this event"
                  >
                    +rule
                  </button>
                </Show>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export { formatTime, shortBinary };
