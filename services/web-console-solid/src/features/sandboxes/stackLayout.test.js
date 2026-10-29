import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { bands, drawnEdges, headline, logLoader, neighbours, tone } from './stackLayout.js';

const topology = {
  layers: [
    { index: 0, key: 'operator', label: 'You' },
    { index: 1, key: 'ports', label: 'Ports' },
    { index: 2, key: 'sandboxes', label: 'Sandboxes' },
    { index: 3, key: 'tools', label: 'Tools' },
  ],
  nodes: [
    { id: 'operator', layer: 0, name: 'You', status: 'ok' },
    { id: 'port:1', layer: 1, name: ':22201', status: 'warn' },
    { id: 'sandbox:a', layer: 2, name: 'hermes', status: 'ok', logs: { kind: 'sandbox', id: 'a' } },
  ],
  edges: [
    { from: 'operator', to: 'port:1', kind: 'ingress', status: 'ok', requests: 0, denied: 0 },
    { from: 'port:1', to: 'sandbox:a', kind: 'forward', status: 'warn', requests: 0, denied: 0 },
    { from: 'port:1', to: 'sandbox:a', kind: 'forward', status: 'error', requests: 2, denied: 1 },
  ],
  counts: { ok: 2, warn: 1 },
};

describe('bands', () => {
  it('keeps only the layers that have cards, in layer order', () => {
    assert.deepEqual(bands(topology).map((band) => band.key), ['operator', 'ports', 'sandboxes']);
  });
  it('is empty before the first answer', () => {
    assert.deepEqual(bands(null), []);
  });
});

describe('neighbours', () => {
  it('splits what a card receives from what it sends, with names', () => {
    const around = neighbours(topology, 'port:1');
    assert.equal(around.incoming[0].otherName, 'You');
    assert.equal(around.outgoing.length, 2);
  });
});

describe('drawnEdges', () => {
  it('merges a repeated pair and keeps the worse status', () => {
    const drawn = drawnEdges(topology);
    assert.equal(drawn.length, 2);
    const forward = drawn.find((edge) => edge.to === 'sandbox:a');
    assert.equal(forward.status, 'error');
    assert.equal(forward.requests, 2);
    assert.equal(forward.denied, 1);
  });
});

describe('headline', () => {
  it('says so when everything is fine', () => {
    assert.equal(headline({ counts: { ok: 5 } }), 'Everything in this stack answers.');
  });
  it('names what is wrong', () => {
    assert.equal(headline(topology), '1 need attention.');
    assert.equal(headline({ counts: { error: 2, warn: 1 } }), '2 failing, 1 need attention.');
  });
});

describe('tone', () => {
  it('colours by meaning and treats an unknown word as neutral', () => {
    assert.equal(tone('error'), 'bad');
    assert.equal(tone('ok'), 'ok');
    assert.equal(tone('whatever'), 'muted');
  });
});

describe('logLoader', () => {
  const api = {
    sandboxLogs: async (id) => ({ content: `sandbox ${id}` }),
    componentLogs: async (id) => ({ content: `component ${id}` }),
  };
  it('reads a sandbox log and a component log through the right call', async () => {
    assert.equal(await logLoader({ logs: { kind: 'sandbox', id: 'a' } }, api)(), 'sandbox a');
    assert.equal(await logLoader({ logs: { kind: 'component', id: 'egress-proxy' } }, api)(), 'component egress-proxy');
  });
  it('returns null for a card with no log', () => {
    assert.equal(logLoader({ name: 'Internet' }, api), null);
  });
});
