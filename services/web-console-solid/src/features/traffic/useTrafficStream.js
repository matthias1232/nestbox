// Live telemetry state.
//
// The buffer is capped: a live console that grows without bound is a memory leak
// that shows up an hour into an incident, which is the worst possible moment.

import { createSignal, onCleanup } from 'solid-js';
import * as telemetry from '../../api/telemetry.js';

const MAX_ROWS = 500;

export function useTrafficStream() {
  const [events, setEvents] = createSignal([]);
  const [paused, setPaused] = createSignal(false);
  const [connected, setConnected] = createSignal(false);
  const [kind, setKind] = createSignal('');
  const [error, setError] = createSignal(null);
  const [pending, setPending] = createSignal([]);

  let close = null;

  function start() {
    if (close) return;
    close = telemetry.stream(kind() || null, {
      onEvent: (event) => {
        // While paused the rows are held aside, so resuming shows what arrived
        // instead of silently skipping it.
        if (paused()) {
          setPending((current) => [event, ...current].slice(0, MAX_ROWS));
          return;
        }
        append(event);
      },
      onError: () => setConnected(false),
    });
    setConnected(true);
  }

  function append(event) {
    setEvents((current) => [event, ...current].slice(0, MAX_ROWS));
  }

  function stop() {
    close?.();
    close = null;
    setConnected(false);
  }

  function togglePause() {
    if (paused()) {
      // Flush what arrived while paused, newest last so the order is preserved.
      const buffered = [...pending()].reverse();
      setEvents((current) => [...buffered, ...current].slice(0, MAX_ROWS));
      setPending([]);
      setPaused(false);
    } else {
      setPaused(true);
    }
  }

  function clear() {
    setEvents([]);
    setPending([]);
  }

  async function seed() {
    setError(null);
    try {
      await telemetry.seedDemo();
    } catch (failure) {
      setError(failure.message);
    }
  }

  const visible = () => {
    const selected = kind();
    if (!selected) return events();
    return events().filter((event) => event.kind === selected);
  };

  const counts = () => {
    const result = { network: 0, dns: 0, inference: 0 };
    for (const event of events()) {
      if (result[event.kind] !== undefined) result[event.kind] += 1;
    }
    return result;
  };

  onCleanup(stop);

  return {
    events, visible, counts, paused, connected, kind, error, pending,
    start, stop, togglePause, clear, seed, setKind, setError,
  };
}
