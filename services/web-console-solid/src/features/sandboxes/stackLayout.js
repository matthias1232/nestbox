// Arranging the stack diagram's answer for drawing. Pure functions, no JSX:
// the server decides what is in the stack and what state each piece is in; this
// file only groups it by layer and answers "what touches this card?".

export const STATUS_TONE = {
  ok: 'ok',
  warn: 'warn',
  error: 'bad',
  idle: 'muted',
  unknown: 'muted',
  absent: 'muted',
};

export function tone(status) {
  return STATUS_TONE[status] || 'muted';
}

// The layers that have something in them, each with its cards in server order.
// Empty layers are kept out of the picture: a band with nothing in it is noise,
// and the server already draws an explicit "absent" card where an absence matters.
export function bands(topology) {
  if (!topology) return [];
  const byLayer = new Map();
  for (const node of topology.nodes || []) {
    if (!byLayer.has(node.layer)) byLayer.set(node.layer, []);
    byLayer.get(node.layer).push(node);
  }
  return (topology.layers || [])
    .filter((layer) => byLayer.has(layer.index))
    .map((layer) => ({ ...layer, nodes: byLayer.get(layer.index) }));
}

// The edges that start or end at a node, split by direction.
export function neighbours(topology, id) {
  const edges = topology?.edges || [];
  const names = new Map((topology?.nodes || []).map((node) => [node.id, node.name]));
  const label = (edge, other) => ({ ...edge, other, otherName: names.get(other) || other });
  return {
    outgoing: edges.filter((edge) => edge.from === id).map((edge) => label(edge, edge.to)),
    incoming: edges.filter((edge) => edge.to === id).map((edge) => label(edge, edge.from)),
  };
}

// Edges that are drawn. The same pair can appear twice (a stack member that also
// shows traffic); the worst status wins so the line never looks healthier than
// the thing it joins.
const RANK = { error: 5, warn: 4, unknown: 3, idle: 2, absent: 1, ok: 0 };

export function drawnEdges(topology) {
  const merged = new Map();
  for (const edge of topology?.edges || []) {
    const key = `${edge.from}>${edge.to}`;
    const seen = merged.get(key);
    if (!seen) {
      merged.set(key, { ...edge });
      continue;
    }
    if ((RANK[edge.status] ?? 0) > (RANK[seen.status] ?? 0)) seen.status = edge.status;
    seen.requests += edge.requests || 0;
    seen.denied += edge.denied || 0;
  }
  return [...merged.values()];
}

// One sentence for the header: how many things are fine and what is not.
export function headline(topology) {
  if (!topology) return '';
  const counts = topology.counts || {};
  const bad = (counts.error || 0) + (counts.warn || 0);
  if (bad === 0) return 'Everything in this stack answers.';
  const parts = [];
  if (counts.error) parts.push(`${counts.error} failing`);
  if (counts.warn) parts.push(`${counts.warn} need attention`);
  return parts.join(', ') + '.';
}

// Where a card's log is read from: a function the viewer calls, or null.
export function logLoader(node, api) {
  const source = node?.logs;
  if (!source) return null;
  if (source.kind === 'sandbox') return () => api.sandboxLogs(source.id).then((answer) => answer.content);
  if (source.kind === 'component') return () => api.componentLogs(source.id).then((answer) => answer.content);
  return null;
}
