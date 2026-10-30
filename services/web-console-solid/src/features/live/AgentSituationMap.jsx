// The agent map: one node per agent, one edge per recent target.
//
// A dot travels an edge only when an event for that agent and target actually
// arrived. There is no idle pulse, no looping shimmer, and no replay: if the
// map is still, nothing is happening. With `prefers-reduced-motion` the dot is
// replaced by a highlight of the same edge, target, and agent for a moment.

import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  onMount,
} from 'solid-js';
import { KINDS, kindClass, kindLabel } from './kinds.js';
import {
  buildMap,
  nodeLabel,
  nodeMeta,
  pathFor,
  pointOnCurve,
  targetLabel,
  targetMeta,
} from './layout.js';

const DOT_MS = 900;
const FLASH_MS = 1500;

const STATUS_TONE = {
  running: 'ok', healthy: 'ok', active: 'ok', ok: 'ok',
  starting: 'warn', degraded: 'warn', pending: 'warn', throttled: 'warn',
  failed: 'bad', unhealthy: 'bad', offline: 'bad', blocked: 'bad', denied: 'bad',
  stopped: 'muted', idle: 'muted', unknown: 'muted',
};

function tone(status) {
  return STATUS_TONE[String(status || '').toLowerCase()] || 'muted';
}

export default function AgentSituationMap(props) {
  const layout = createMemo(() => buildMap(props.agents || []));

  const edgeIndex = createMemo(() => {
    const index = new Map();
    for (const edge of layout().edges) {
      index.set(`${edge.agentKey}||${edge.targetKey}`, edge);
    }
    return index;
  });

  const [dots, setDots] = createSignal([]);
  const [flashes, setFlashes] = createSignal([]);
  const [reduced, setReduced] = createSignal(false);

  const flashEdges = createMemo(() => {
    const ids = new Set();
    for (const flash of flashes()) {
      if (flash.edgeId) ids.add(flash.edgeId);
    }
    return ids;
  });

  const flashAgents = createMemo(() => {
    const keys = new Set();
    for (const flash of flashes()) {
      if (flash.agentKey) keys.add(flash.agentKey);
    }
    return keys;
  });

  let active = [];
  let frameHandle = 0;
  let seen = new Set();
  const flashTimers = new Set();

  onMount(() => {
    const query = window.matchMedia?.('(prefers-reduced-motion: reduce)');
    if (!query) return;
    const apply = () => setReduced(query.matches);
    apply();
    query.addEventListener?.('change', apply);
    onCleanup(() => query.removeEventListener?.('change', apply));
  });

  // If the preference flips to reduced while dots are in flight, they stop
  // immediately rather than finishing in defiance of the setting.
  createEffect(() => {
    if (reduced() && active.length > 0) {
      active = [];
      setDots([]);
    }
  });

  createEffect(on(() => props.pulses, (list) => {
    for (const pulse of list || []) {
      if (seen.has(pulse.id)) continue;
      seen.add(pulse.id);

      const edge = edgeIndex().get(`${pulse.agentKey}||${pulse.kind}|${pulse.target}`);
      // A target that is not on the map (older than the four shown) still
      // happened: highlight the agent instead of inventing an edge.
      if (reduced() || !edge) {
        addFlash(pulse, edge);
        continue;
      }

      active.push({ id: pulse.id, edge, kind: pulse.kind, start: performance.now() });
      startLoop();
    }
    if (seen.size > 400) {
      seen = new Set([...seen].slice(-100));
    }
  }, { defer: true }));

  function addFlash(pulse, edge) {
    const entry = {
      id: pulse.id,
      edgeId: edge?.id || null,
      agentKey: pulse.agentKey,
    };
    setFlashes((current) => [...current, entry]);

    const timer = window.setTimeout(() => {
      flashTimers.delete(timer);
      setFlashes((current) => current.filter((item) => item.id !== pulse.id));
    }, FLASH_MS);
    flashTimers.add(timer);
  }

  function startLoop() {
    if (frameHandle) return;
    frameHandle = requestAnimationFrame(tick);
  }

  function tick(now) {
    frameHandle = 0;
    const still = [];
    const shown = [];

    for (const item of active) {
      const progress = (now - item.start) / DOT_MS;
      if (progress >= 1) continue;
      const point = pointOnCurve(item.edge, progress);
      still.push(item);
      shown.push({ id: item.id, kind: item.kind, x: point.x, y: point.y });
    }

    active = still;
    setDots(shown);
    if (active.length > 0) frameHandle = requestAnimationFrame(tick);
  }

  onCleanup(() => {
    if (frameHandle) cancelAnimationFrame(frameHandle);
    for (const timer of flashTimers) window.clearTimeout(timer);
    flashTimers.clear();
  });

  return (
    <div class="live-map">
      <div class="live-legend">
        <For each={KINDS}>
          {(kind) => (
            <span>
              <i class={`live-swatch ${kindClass(kind.id)}`} />
              {kind.label}
            </span>
          )}
        </For>
        <span class="muted">
          {reduced() ? 'highlights mark real events' : 'one dot = one real event'}
        </span>
      </div>

      <svg
        viewBox={`0 0 ${layout().width} ${layout().height}`}
        width="100%"
        height={layout().height}
        role="group"
        aria-label="Agent situation map: one node per agent, connected to its recent targets"
        class="live-map-svg"
      >
        <For each={layout().edges}>
          {(edge) => (
            <path
              class={`live-edge ${kindClass(edge.kind)} ${flashEdges().has(edge.id) ? 'flash' : ''}`}
              d={pathFor(edge)}
            />
          )}
        </For>

        <For each={layout().targets}>
          {(target) => (
            <g
              class={`live-target ${kindClass(target.kind)} ${flashEdges().has(target.edgeId) ? 'flash' : ''}`}
              transform={`translate(${target.x}, ${target.y})`}
            >
              <title>{target.target}</title>
              <rect width={target.width} height={target.height} rx="5" />
              <text class="live-target-name" x="10" y="14">{targetLabel(target)}</text>
              <text class="live-target-meta" x="10" y="26">
                {kindLabel(target.kind)}{target.model ? ` · ${targetMeta(target)}` : ''}
              </text>
            </g>
          )}
        </For>

        <For each={layout().nodes}>
          {(node) => (
            <g
              class={`live-node ${tone(node.agent.status)} ${props.selectedKey === node.key ? 'selected' : ''} ${flashAgents().has(node.key) ? 'flash' : ''}`}
              transform={`translate(${node.x}, ${node.y})`}
              role="button"
              tabindex="0"
              aria-pressed={props.selectedKey === node.key ? 'true' : 'false'}
              aria-label={`${node.agent.agent} on ${node.agent.sandbox}`}
              onClick={() => props.onSelect?.(node.key)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  props.onSelect?.(node.key);
                }
              }}
            >
              <rect width={node.width} height={node.height} rx="7" />
              <text class="live-node-name" x="11" y="19">{nodeLabel(node.agent)}</text>
              <text class="live-node-meta" x="11" y="34">{nodeMeta(node.agent)}</text>
              <Show when={node.agent.current}>
                <circle
                  class={`live-node-live ${kindClass(node.agent.current.kind)}`}
                  cx={node.width - 12}
                  cy="12"
                  r="3.5"
                />
              </Show>
            </g>
          )}
        </For>

        <For each={dots()}>
          {(dot) => (
            <circle
              class={`live-dot ${kindClass(dot.kind)}`}
              cx={dot.x}
              cy={dot.y}
              r="4.5"
            />
          )}
        </For>
      </svg>
    </div>
  );
}
