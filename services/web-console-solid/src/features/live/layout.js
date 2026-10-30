// Deterministic placement for the agent map.
//
// No force simulation: the same agents and targets must produce the same
// picture on every load, and a node must not drift under the cursor while an
// operator is reading it. Agents get one vertical band each, sized by the
// number of targets they show; targets stack inside that band.

import { shortenTarget, truncate } from './format.js';

export const MAP_WIDTH = 720;
export const MAX_TARGETS = 4;

const PAD = 16;
const TOP = 28;
const AGENT_X = 16;
const AGENT_WIDTH = 180;
const AGENT_HEIGHT = 48;
const TARGET_X = 424;
const TARGET_WIDTH = 280;
const TARGET_HEIGHT = 34;
const TARGET_GAP = 8;
const BAND_GAP = 22;

// Recent targets of one agent, deduplicated by kind and target.
//
// The current activity is considered first, then the newest events. The four
// survivors are sorted by key, so a new event changes the set only when it
// introduces a new target, not the position of the ones already drawn.
export function targetsFor(agent, limit = MAX_TARGETS) {
  const seen = new Map();
  const consider = (kind, target, model) => {
    if (!target) return;
    const key = `${kind || 'unknown'}|${target}`;
    if (!seen.has(key)) {
      seen.set(key, { key, kind: kind || 'unknown', target, model: model || null });
    }
  };

  if (agent.current) consider(agent.current.kind, agent.current.target, agent.current.model);
  for (const event of agent.events || []) consider(event.kind, event.target, event.model);

  const recent = [...seen.values()].slice(0, limit);
  recent.sort((left, right) => left.key.localeCompare(right.key));
  return recent;
}

// The whole picture: agent nodes, target nodes, and their edges.
export function buildMap(agents) {
  const ordered = [...agents].sort((left, right) => {
    const bySandbox = String(left.sandbox).localeCompare(String(right.sandbox));
    return bySandbox !== 0 ? bySandbox : String(left.agent).localeCompare(String(right.agent));
  });

  const nodes = [];
  const targets = [];
  const edges = [];
  let y = TOP;

  for (const agent of ordered) {
    const list = targetsFor(agent);
    const band = Math.max(AGENT_HEIGHT, list.length * (TARGET_HEIGHT + TARGET_GAP));
    const node = {
      key: agent.key,
      agent,
      x: AGENT_X,
      y: y + (band - AGENT_HEIGHT) / 2,
      width: AGENT_WIDTH,
      height: AGENT_HEIGHT,
    };
    nodes.push(node);

    list.forEach((target, index) => {
      const placed = {
        ...target,
        agentKey: agent.key,
        agent,
        x: TARGET_X,
        y: y + index * (TARGET_HEIGHT + TARGET_GAP),
        width: TARGET_WIDTH,
        height: TARGET_HEIGHT,
      };
      const edge = edgeFor(node, placed);
      placed.edgeId = edge.id;
      targets.push(placed);
      edges.push(edge);
    });

    y += band + BAND_GAP;
  }

  return {
    width: MAP_WIDTH,
    height: Math.max(y + PAD, 160),
    nodes,
    targets,
    edges,
  };
}

// A cubic that leaves the agent's right edge and enters the target's left edge.
// The control points are stored, not just the path string, because the moving
// dot has to be placed on the same curve the line is drawn along.
export function edgeFor(node, target) {
  const from = { x: node.x + node.width, y: node.y + node.height / 2 };
  const to = { x: target.x, y: target.y + target.height / 2 };
  const bend = Math.max(56, (to.x - from.x) / 2);
  return {
    id: `${node.key}|${target.key}`,
    agentKey: node.agent.key,
    targetKey: target.key,
    kind: target.kind,
    p0: from,
    p1: { x: from.x + bend, y: from.y },
    p2: { x: to.x - bend, y: to.y },
    p3: to,
  };
}

export function pathFor(edge) {
  const { p0, p1, p2, p3 } = edge;
  return `M ${p0.x} ${p0.y} C ${p1.x} ${p1.y}, ${p2.x} ${p2.y}, ${p3.x} ${p3.y}`;
}

// Point at `t` (0 at the agent, 1 at the target) on the edge's curve.
export function pointOnCurve(edge, t) {
  const u = 1 - t;
  const a = u * u * u;
  const b = 3 * u * u * t;
  const c = 3 * u * t * t;
  const d = t * t * t;
  return {
    x: a * edge.p0.x + b * edge.p1.x + c * edge.p2.x + d * edge.p3.x,
    y: a * edge.p0.y + b * edge.p1.y + c * edge.p2.y + d * edge.p3.y,
  };
}

// Labels are prepared here so the map component stays markup.
export function nodeLabel(agent) {
  return truncate(agent.agent, 22);
}

export function nodeMeta(agent) {
  return truncate(agent.pack || agent.status || 'unknown', 26);
}

export function targetLabel(target) {
  return shortenTarget(target.target, 40);
}

export function targetMeta(target) {
  return target.model ? truncate(target.model, 28) : target.kind;
}
