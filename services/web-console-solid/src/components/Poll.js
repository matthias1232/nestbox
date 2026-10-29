// Poll a loader while a component is mounted. `load` returns a promise; each
// answer goes to `onData`, a failure to `onError`. Pausing stops the timer.
import { createSignal, onCleanup, onMount } from 'solid-js';

export function usePolling(load, { everyMs = 5000, onData, onError } = {}) {
  const [paused, setPaused] = createSignal(false);
  let timer = null;
  let alive = true;

  async function tick() {
    if (!alive) return;
    try {
      const data = await load();
      if (alive) onData?.(data);
    } catch (failure) {
      if (alive) onError?.(failure);
    }
  }

  function start() {
    stop();
    if (!paused()) timer = setInterval(tick, everyMs);
  }
  function stop() {
    if (timer) clearInterval(timer);
    timer = null;
  }

  onMount(() => {
    tick();
    start();
  });
  onCleanup(() => {
    alive = false;
    stop();
  });

  return {
    refresh: tick,
    paused,
    toggle() {
      setPaused(!paused());
      paused() ? stop() : start();
    },
  };
}
