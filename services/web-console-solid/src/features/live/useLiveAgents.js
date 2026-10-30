// Live agent situation state.
//
// Two sources, one view: the snapshot says who exists and where the counters
// stood when it was read; the stream says what is happening now. A node is
// created from an event when the snapshot has never heard of the agent, because
// dropping that event would hide the most interesting case (a new or rogue
// agent).

import { createMemo, createSignal, onCleanup } from 'solid-js';
import * as live from '../../api/live.js';
import { toMillis } from './format.js';
import { organisationTotals } from './metrics.js';

const MAX_EVENTS_PER_AGENT = 20;
const MAX_PULSES = 32;
const MAX_TOKEN_WINDOW = 200;

function keyOf(sandbox, agent) {
  return `${sandbox ?? ''}::${agent ?? ''}`;
}

function count(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function blankAgent(sandbox, agent) {
  return {
    key: keyOf(sandbox, agent),
    sandbox: sandbox ?? 'unknown',
    agent: agent ?? 'unknown',
    pack: null,
    status: 'unknown',
    current: null,
    // Snapshot counters and the delta since that snapshot. They are added at
    // read time so a refreshed snapshot can replace its half without touching
    // the event log.
    baseRequests: 0,
    baseCost: 0,
    liveRequests: 0,
    liveCost: 0,
    // Whether any source has reported a cost. Until it has, the panel says
    // `n/a` instead of claiming the agent spent nothing.
    costReported: false,
    tokenWindow: [],
    events: [],
  };
}

export function useLiveAgents() {
  const [agents, setAgents] = createSignal([]);
  const [selectedKey, setSelectedKey] = createSignal(null);
  const [pulses, setPulses] = createSignal([]);
  const [connection, setConnection] = createSignal('connecting');
  const [snapshotError, setSnapshotError] = createSignal(null);
  const [loading, setLoading] = createSignal(true);

  let close = null;
  let sequence = 0;
  // Set on an error. The next successful open refreshes the snapshot, because
  // EventSource reconnects on its own but cannot replay what it missed.
  let missedWhileDown = false;

  function start() {
    if (close) return;
    setConnection('connecting');
    if (loading()) loadSnapshot();

    try {
      close = live.stream({
        onOpen: () => {
          setConnection('live');
          if (missedWhileDown) {
            missedWhileDown = false;
            loadSnapshot();
          }
        },
        onFrame: applyFrame,
        onError: () => {
          missedWhileDown = true;
          setConnection('lost');
        },
      });
    } catch (failure) {
      missedWhileDown = true;
      setConnection('lost');
      setSnapshotError(failure.message);
    }
  }

  // A manual retry replaces the source rather than waiting out the browser's
  // backoff, and asks for a fresh snapshot for the time it was down.
  function retry() {
    close?.();
    close = null;
    missedWhileDown = true;
    setConnection('connecting');
    start();
  }

  function stop() {
    close?.();
    close = null;
    setConnection('connecting');
  }

  async function loadSnapshot() {
    try {
      const payload = await live.snapshot();
      applySnapshot(payload);
      setSnapshotError(null);
    } catch (failure) {
      setSnapshotError(failure.message);
      // "No snapshot" is not "still loading": the stream may still work, and
      // the page must say which half is degraded.
      setLoading(false);
    }
  }

  function applyFrame(frame) {
    if (!frame || typeof frame !== 'object') return;
    if (frame.type === 'snapshot') {
      applySnapshot(frame);
      return;
    }
    if (frame.type === 'request') {
      applyRequest(frame);
      return;
    }
    // A frame type this build does not know is ignored rather than breaking
    // the stream; the next known frame still arrives.
  }

  function applySnapshot(payload) {
    const rows = Array.isArray(payload)
      ? payload
      : Array.isArray(payload?.agents)
        ? payload.agents
        : [];

    const currentAgents = agents();
    const byKey = new Map(currentAgents.map((agent) => [agent.key, agent]));
    const merged = [];

    for (const row of rows) {
      if (!row || (!row.agent && !row.sandbox)) continue;
      const key = keyOf(row.sandbox, row.agent);
      const existing = byKey.get(key);
      byKey.delete(key);
      const base = existing || blankAgent(row.sandbox, row.agent);
      merged.push({
        ...base,
        sandbox: row.sandbox ?? base.sandbox,
        agent: row.agent ?? base.agent,
        pack: row.pack ?? base.pack,
        status: row.status ?? base.status,
        // The snapshot's "now" wins when it reports one; otherwise the last
        // event seen remains the current activity.
        current: row.current ?? base.current,
        baseRequests: count(row.requests_5m),
        baseCost: count(row.cost_micros_5m),
        costReported: typeof row.cost_micros_5m === 'number'
          ? true
          : (existing?.costReported ?? false),
        // The snapshot already contains the events counted before it was read,
        // so the live delta restarts. Otherwise those requests would count
        // twice after every refresh.
        liveRequests: 0,
        liveCost: 0,
      });
    }

    // An agent only the stream knows about survives a snapshot that does not
    // mention it yet: the event is newer evidence than the snapshot.
    for (const leftover of byKey.values()) merged.push(leftover);

    setAgents(merged);
    setLoading(false);
    if (!selectedKey() && merged.length > 0) setSelectedKey(merged[0].key);
  }

  function applyRequest(frame) {
    const key = keyOf(frame.sandbox, frame.agent);
    const atMs = toMillis(frame.at) ?? Date.now();
    const event = {
      id: ++sequence,
      kind: frame.kind || 'unknown',
      target: frame.target || '',
      model: frame.model ?? null,
      status: frame.status ?? null,
      tokens: typeof frame.tokens === 'number' ? frame.tokens : null,
      cost_micros: typeof frame.cost_micros === 'number' ? frame.cost_micros : null,
      at: frame.at ?? null,
      atMs,
    };

    const currentAgents = agents();
    const index = currentAgents.findIndex((agent) => agent.key === key);
    const agent = index === -1 ? blankAgent(frame.sandbox, frame.agent) : currentAgents[index];

    const tokenWindow = event.tokens === null
      ? agent.tokenWindow
      : [...agent.tokenWindow, { atMs, tokens: event.tokens }].slice(-MAX_TOKEN_WINDOW);

    const updated = {
      ...agent,
      current: {
        kind: event.kind,
        target: event.target,
        model: event.model,
        at: event.at,
      },
      events: [event, ...agent.events].slice(0, MAX_EVENTS_PER_AGENT),
      tokenWindow,
      liveRequests: agent.liveRequests + 1,
      liveCost: agent.liveCost + (event.cost_micros ?? 0),
      costReported: agent.costReported || event.cost_micros !== null,
    };

    setAgents(
      index === -1
        ? [...currentAgents, updated]
        : currentAgents.map((item, at) => (at === index ? updated : item)),
    );
    setLoading(false);
    // A first event on an otherwise empty page is the only thing to look at, so
    // it selects its own agent instead of leaving the panel asking for a click.
    if (!selectedKey()) setSelectedKey(key);

    // The queue only feeds the animation; the log above is the durable record.
    setPulses((current) => [
      { id: event.id, agentKey: key, kind: event.kind, target: event.target },
      ...current,
    ].slice(0, MAX_PULSES));
  }

  const totals = createMemo(() => organisationTotals(agents()));

  const selected = createMemo(() => {
    const key = selectedKey();
    if (!key) return null;
    return agents().find((agent) => agent.key === key) || null;
  });

  function select(key) {
    setSelectedKey(key);
  }

  onCleanup(stop);

  return {
    agents,
    totals,
    selected,
    selectedKey,
    select,
    pulses,
    connection,
    snapshotError,
    loading,
    start,
    retry,
    stop,
  };
}
