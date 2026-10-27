// Telemetry history and the live stream.

import { request, query } from './client.js';

export function history(params = {}) {
  return request(`telemetry/events${query(params)}`);
}

export function facets(minutes = 60) {
  return request(`telemetry/facets${query({ minutes })}`);
}

export function seedDemo() {
  return request('telemetry/seed-demo', { method: 'POST' });
}

// Open the server-sent event stream.
//
// EventSource cannot send headers, so the session cookie is what authenticates
// it. That is why the stream works from a browser session but not from an API
// token; automation reads history instead.
export function stream(kind, handlers) {
  const source = new EventSource(`/api/v1/telemetry/stream`);

  const onMessage = (event) => {
    try {
      const payload = JSON.parse(event.data);
      if (!kind || payload.kind === kind) handlers.onEvent(payload);
    } catch {
      // A malformed frame is dropped rather than breaking the stream.
    }
  };

  for (const name of ['network', 'dns', 'inference']) {
    source.addEventListener(name, onMessage);
  }
  source.onerror = () => handlers.onError?.();

  return () => source.close();
}

export function stateStream(handlers) {
  const source = new EventSource('/api/v1/events/stream');
  for (const name of ['sandbox', 'gateway', 'job', 'policy_rollout']) {
    source.addEventListener(name, (event) => {
      try {
        handlers.onEvent(JSON.parse(event.data));
      } catch {
        /* ignore a malformed frame */
      }
    });
  }
  source.onerror = () => handlers.onError?.();
  return () => source.close();
}
