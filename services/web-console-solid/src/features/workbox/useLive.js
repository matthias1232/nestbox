// One subscription to the state stream, filtered per sandbox.
//
// Every workbox panel wants the same events: an agent's sign-in changed, the
// proxy mode changed, a window opened, a model became ready. One EventSource per
// page instead of one per panel keeps the stream count at one and the listeners
// in a set.

import { stateStream } from '../../api/telemetry.js';

const listeners = new Set();
let close = null;

function ensure() {
  if (close) return;
  close = stateStream({
    onEvent: (event) => {
      for (const listener of listeners) listener(event);
    },
    onError: () => {},
  });
}

/**
 * Call `callback(event)` for control and model events of one sandbox.
 *
 * The accessor is a function so the caller may pass a signal. Returns a stop
 * function; when the last listener leaves, the stream is closed again.
 */
export function onControl(sandboxId, callback) {
  const listener = (event) => {
    if (event.type !== 'sandbox_control_changed' && event.type !== 'models_changed') return;
    if (event.sandbox_id && event.sandbox_id !== sandboxId()) return;
    callback(event);
  };
  listeners.add(listener);
  ensure();
  return () => {
    listeners.delete(listener);
    if (!listeners.size && close) {
      close();
      close = null;
    }
  };
}
