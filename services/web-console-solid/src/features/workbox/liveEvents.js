// One telemetry feed, three lists: proxy, OpenShell and MCP events.
//
// The control API does not publish three streams with three names. It publishes
// one telemetry stream (`/api/v1/telemetry/stream`, carried by
// `api/telemetry.js`) whose rows are labelled by `kind` (`network`, `dns`,
// `inference`) and, for a row that came out of a sandbox's own supervisor, by
// the listener that wrote it. Deciding which of the three lists a row belongs
// to, and what the row says, is presentation logic, so it lives here as plain
// functions and is tested without a DOM (`liveEvents.test.js`).

// The mark of an OpenShell row.
//
// Read from `services/control-api-rust/src/services/telemetry_service.rs`
// (`pull_openshell_events`): a network row it stores is built with
// `listener: Some("openshell")`, a DNS row with `resolver: Some("openshell")`.
// The events table has a `source` column as well, but the read path in
// `store/postgres/telemetry.rs` (`list`) does not select it, and on a network
// row `source` is the client binary — so neither is usable here.
export const OPENSHELL_MARK = 'openshell';

// How many rows the panel keeps. A live list is read by a person, not exported;
// 200 rows cover the last few minutes of a busy sandbox in one screen of scroll.
export const RING = 200;

// The three lists, in the order the panel shows them. A row belongs to exactly
// one of them, so the switch never changes a number without showing fewer rows.
export const SEGMENTS = [
  { value: 'proxy', label: 'Proxy' },
  { value: 'openshell', label: 'OpenShell' },
  { value: 'mcp', label: 'MCP' },
];

// MCP calls win over the listener: a tool call seen through OpenShell is still
// an MCP call, and "which tool did the agent use" is the question this list
// answers. Everything else that the supervisor reported is OpenShell; the rest
// came through the egress proxy.
export function classify(event) {
  if (!event) return 'proxy';
  if (event.mcp_method || event.mcp_tool) return 'mcp';
  if (event.listener === OPENSHELL_MARK || event.resolver === OPENSHELL_MARK) return 'openshell';
  return 'proxy';
}

/** True when the event belongs to this sandbox. The stream is not per sandbox. */
export function isSandbox(event, sandboxId) {
  if (!event || !sandboxId) return false;
  return event.sandbox_id === sandboxId;
}

function hostOf(event) {
  return event.destination || event.hostname || event.host || '';
}

// The address a connection actually reached, when the row carries one.
//
// The console reads `upstream_ip` (`prepare_traffic` writes it into the stored
// detail) and, if a future ingest adds `resolved_ip`, that name wins. OpenShell
// rows have no upstream address yet, so the cell stays empty rather than
// inventing one.
function resolvedOf(event) {
  return event.resolved_ip || event.upstream_ip || null;
}

/**
 * One event as a table row.
 *
 * `target` is what the reader scans for: a host and port for a connection, the
 * hostname for a name lookup, the tool for an MCP call. `method` falls back to
 * the wire protocol, because an OpenShell connection has no HTTP method and
 * "tcp" is still the answer to "how did it get out".
 */
export function toRow(event) {
  if (!event) return null;
  const kind = classify(event);
  const host = hostOf(event);
  const tool = event.mcp_tool || null;
  const withPort = event.port != null && event.port !== '' ? `${host}:${event.port}` : host;
  const target = tool || withPort || event.sni || '';
  const ts = event.ts || null;
  return {
    // The id is the table's row key and the ring buffer's identity. A live row
    // without one gets a stable composite so it is not duplicated on re-render.
    id: event.id || `${ts}|${kind}|${target}`,
    ts,
    kind,
    host: host || null,
    target,
    resolved_ip: resolvedOf(event),
    decision: event.decision || null,
    reason: event.rule_name || event.reason || null,
    method: event.mcp_method || event.method || event.protocol || null,
    tool,
    error: event.error || event.error_message || null,
  };
}

/**
 * `row` at the top of `rows`, the same id replaced rather than repeated.
 *
 * Newest first, because a live list is read at the top. The list is cut to
 * `limit`, so a client that talks for hours cannot grow the panel without bound.
 */
export function pushRow(rows, row, limit = RING) {
  if (!row) return rows;
  const rest = rows.filter((entry) => entry.id !== row.id);
  return [row, ...rest].slice(0, limit);
}
