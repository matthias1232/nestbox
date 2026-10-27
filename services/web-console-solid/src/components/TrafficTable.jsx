// A dense event table for the live console.
//
// Deliberately plain tables rather than a virtual list: at the row counts the
// console keeps (a few hundred in view), a table is faster to render and far
// easier to make accessible. The buffer is capped by the caller.

import StatusBadge from './StatusBadge.jsx';

function direction(event) {
  return event.direction === 'inbound' ? '▼' : '▲';
}

function decisionTone(decision) {
  if (decision === 'deny') return 'bad';
  if (decision === 'audit') return 'warn';
  return 'ok';
}

export default function TrafficTable(props) {
  return (
    <div class="table-scroll">
      <table class="dense">
        <thead>
          <tr>
            <th class="narrow">Time</th>
            <th class="narrow">Dir</th>
            <th>Protocol</th>
            <th>Source</th>
            <th>Destination</th>
            <th class="narrow">Port</th>
            <th>Binary</th>
            <th>Decision</th>
            <th class="narrow">ms</th>
            <th class="narrow" />
          </tr>
        </thead>
        <tbody>
          {props.events.map((event) => (
            <tr class={event.decision === 'deny' ? 'row-deny' : ''}>
              <td class="mono">{formatTime(event.ts)}</td>
              <td class="narrow">{direction(event)}</td>
              <td>{event.detail?.protocol || event.protocol || '—'}</td>
              <td class="mono">{event.detail?.source || event.source || '—'}</td>
              <td class="mono">{event.detail?.destination || event.destination || '—'}</td>
              <td class="narrow mono">{event.detail?.port ?? event.port ?? '—'}</td>
              <td class="mono small">{shortBinary(event.detail?.binary || event.binary)}</td>
              <td><StatusBadge status={event.detail?.decision || event.decision || 'allow'} /></td>
              <td class="narrow mono">{event.detail?.latency_ms ?? event.latency_ms ?? '—'}</td>
              <td class="narrow">
                <button class="tiny" onClick={() => props.onDraft?.(event)} title="Draft a rule from this event">
                  +rule
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function formatTime(ts) {
  if (!ts) return '—';
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleTimeString([], { hour12: false }) +
    '.' + String(date.getMilliseconds()).padStart(3, '0');
}

// A binary path is long and the interesting part is the end.
function shortBinary(path) {
  if (!path) return '—';
  const parts = String(path).split('/');
  return parts[parts.length - 1];
}

export { decisionTone };
