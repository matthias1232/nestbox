// The cluster graph: what runs where, and what it talks through.
//
// Rendered as an inline SVG rather than a graph library. The layout is a fixed
// column flow — cluster, gateway, sandbox, egress — because that is the mental
// model the product promises ("sandbox → OpenShell gateway → policy → egress"),
// and a force layout would make the same data look different on every load.
//
// Columns are read left to right as the control path (which cluster, which
// gateway, which sandbox). Traffic flows the other way, right to left, and the
// egress column makes that explicit: every sandbox's traffic leaves through its
// gateway, never directly.
//
// The component knows nothing about gateways: it receives nodes and edges.

import { For, Show, createMemo } from 'solid-js';

const COLUMN = { supervisor: 0, gateway: 1, sandbox: 2, egress: 3 };

const COLUMN_TITLE = {
  supervisor: 'Cluster',
  gateway: 'OpenShell gateway',
  sandbox: 'Sandbox',
  egress: 'Egress',
};

const COLUMN_ORDER = ['supervisor', 'gateway', 'sandbox', 'egress'];

const WIDTH = 940;
const COLUMN_WIDTH = 190;
const NODE_HEIGHT = 46;
const NODE_GAP = 14;
const HEADER = 44;

export default function TopologyGraph(props) {
  const columns = createMemo(() => {
    const buckets = { gateway: [], supervisor: [], sandbox: [], egress: [] };
    for (const node of props.nodes || []) {
      const column = COLUMN[node.kind];
      if (column === undefined) continue;
      buckets[node.kind].push(node);
    }
    // One egress node per gateway: the policy decision point. It is derived,
    // not served, because it is a property of "a gateway exists", not data.
    for (const gateway of buckets.gateway) {
      buckets.egress.push({
        id: `egress:${gateway.id}`,
        kind: 'egress',
        name: 'policy → internet',
        status: gateway.drifted ? 'degraded' : gateway.status,
        gatewayId: gateway.id,
      });
    }
    return buckets;
  });

  // Edges from each sandbox to its gateway's egress, so the traffic path is
  // visible as a line, not implied by adjacency.
  const egressEdges = createMemo(() => {
    const byGateway = new Map();
    for (const edge of props.edges || []) {
      if (edge.kind === 'hosts') byGateway.set(edge.to, edge.from);
    }
    const out = [];
    for (const sandbox of columns().sandbox) {
      const gateway = byGateway.get(sandbox.id);
      if (gateway) out.push({ from: sandbox.id, to: `egress:${gateway}`, kind: 'egress' });
    }
    return out;
  });

  const positions = createMemo(() => {
    const map = new Map();
    for (const [kind, nodes] of Object.entries(columns())) {
      nodes.forEach((node, index) => {
        map.set(node.id, {
          x: 24 + COLUMN[kind] * COLUMN_WIDTH,
          y: HEADER + index * (NODE_HEIGHT + NODE_GAP),
          node,
        });
      });
    }
    return map;
  });

  const height = createMemo(() => {
    const tallest = Math.max(
      1,
      ...Object.values(columns()).map((nodes) => nodes.length),
    );
    return HEADER + tallest * (NODE_HEIGHT + NODE_GAP) + 24;
  });

  return (
    <div class="topology">
      <div class="legend">
        <span><i class="dot ok" /> healthy</span>
        <span><i class="dot warn" /> drifted or starting</span>
        <span><i class="dot bad" /> failed</span>
        <span><i class="dot muted" /> not placed</span>
      </div>
      <Show
        when={(props.nodes || []).length > 0}
        fallback={<p class="muted">No nodes yet. Create a sandbox to see the graph.</p>}
      >
        <svg viewBox={`0 0 ${WIDTH} ${height()}`} width="100%" height={height()} role="img"
             aria-label="Cluster topology">
          <For each={COLUMN_ORDER}>
            {(kind) => (
              <text class="column-title" x={24 + COLUMN[kind] * COLUMN_WIDTH} y={24}>
                {COLUMN_TITLE[kind]}
              </text>
            )}
          </For>

          <For each={[...(props.edges || []), ...egressEdges()]}>
            {(edge) => {
              const from = () => positions().get(edge.from);
              const to = () => positions().get(edge.to);
              return (
                <Show when={from() && to()}>
                  <path
                    class={`edge ${edge.kind || ''}`}
                    d={curve(from(), to())}
                  />
                </Show>
              );
            }}
          </For>

          <For each={[...positions().values()]}>
            {(placed) => (
              <g
                class={`node ${placed.node.kind} ${tone(placed.node)}`}
                transform={`translate(${placed.x}, ${placed.y})`}
                onClick={() => props.onSelect?.(placed.node)}
                role="button"
                tabindex="0"
              >
                <rect width={COLUMN_WIDTH - 46} height={NODE_HEIGHT} rx="6" />
                <text class="node-name" x="12" y="19">{truncate(placed.node.name, 22)}</text>
                <text class="node-meta" x="12" y="35">
                  {placed.node.status || placed.node.kind}
                  {placed.node.drifted ? ' · drift' : ''}
                </text>
              </g>
            )}
          </For>
        </svg>
      </Show>
    </div>
  );
}

// A bezier that leaves the right edge of one node and enters the left edge of
// the next, so parallel edges do not overlap into a single line.
function curve(from, to) {
  const x1 = from.x + COLUMN_WIDTH - 46;
  const y1 = from.y + NODE_HEIGHT / 2;
  const x2 = to.x;
  const y2 = to.y + NODE_HEIGHT / 2;
  const mid = (x1 + x2) / 2;
  return `M ${x1} ${y1} C ${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}`;
}

function tone(node) {
  const status = String(node.status || '').toLowerCase();
  if (status === 'running' || status === 'healthy') return 'ok';
  if (status === 'failed' || status === 'unhealthy' || status === 'offline') return 'bad';
  if (status === 'starting' || status === 'provisioning' || status === 'degraded') return 'warn';
  if (node.drifted) return 'warn';
  return 'muted';
}

function truncate(text, max) {
  const value = String(text ?? '');
  return value.length > max ? `${value.slice(0, max - 1)}…` : value;
}
