import test from 'node:test';
import assert from 'node:assert/strict';
import { OPENSHELL_MARK, RING, SEGMENTS, classify, isSandbox, pushRow, toRow } from './liveEvents.js';

test('a row without an OpenShell or MCP mark is a proxy row', () => {
  assert.equal(classify({ kind: 'network', listener: 'proxy' }), 'proxy');
  assert.equal(classify({ kind: 'network' }), 'proxy');
  assert.equal(classify(null), 'proxy');
});

test('the mark of an OpenShell row is the listener the control API writes', () => {
  assert.equal(OPENSHELL_MARK, 'openshell');
  assert.equal(classify({ kind: 'network', listener: 'openshell' }), 'openshell');
  // A name lookup has no listener; the supervisor writes the resolver instead.
  assert.equal(classify({ kind: 'dns', resolver: 'openshell' }), 'openshell');
});

test('an MCP call is an MCP row even when OpenShell performed it', () => {
  const event = { kind: 'network', listener: 'openshell', mcp_method: 'tools/call', mcp_tool: 'read_file' };
  assert.equal(classify(event), 'mcp');
  assert.equal(classify({ kind: 'network', mcp_tool: 'read_file' }), 'mcp');
});

test('the three lists are the panel segment in a fixed order', () => {
  assert.deepEqual(
    SEGMENTS.map((item) => item.value),
    ['proxy', 'openshell', 'mcp'],
  );
});

test('a connection row names host and port', () => {
  const row = toRow({
    id: 'e1',
    ts: '2026-11-01T10:00:00Z',
    kind: 'network',
    sandbox_id: 's1',
    listener: 'proxy',
    destination: 'api.anthropic.com',
    port: 443,
    protocol: 'tls',
    decision: 'allow',
    method: 'POST',
    rule_name: 'models',
  });
  assert.equal(row.id, 'e1');
  assert.equal(row.kind, 'proxy');
  assert.equal(row.target, 'api.anthropic.com:443');
  assert.equal(row.host, 'api.anthropic.com');
  assert.equal(row.decision, 'allow');
  assert.equal(row.method, 'POST');
  assert.equal(row.reason, 'models');
  assert.equal(row.resolved_ip, null);
  assert.equal(row.error, null);
});

test('a row without an HTTP method says how the traffic left', () => {
  const row = toRow({ kind: 'network', listener: 'openshell', destination: '1.1.1.1', port: 53, protocol: 'udp', decision: 'deny' });
  assert.equal(row.method, 'udp');
  assert.equal(row.target, '1.1.1.1:53');
});

test('a name lookup names the hostname it asked for', () => {
  const row = toRow({ kind: 'dns', resolver: 'openshell', hostname: 'pypi.org', decision: 'allow' });
  assert.equal(row.kind, 'openshell');
  assert.equal(row.kind === 'openshell', true);
  assert.equal(row.target, 'pypi.org');
  assert.equal(row.method, null);
});

test('an MCP row is named by its tool, with the method beside it', () => {
  const row = toRow({
    kind: 'network',
    destination: 'mcp.example.com',
    port: 443,
    mcp_method: 'tools/call',
    mcp_tool: 'search',
    decision: 'allow',
  });
  assert.equal(row.kind, 'mcp');
  assert.equal(row.target, 'search');
  assert.equal(row.tool, 'search');
  assert.equal(row.method, 'tools/call');
});

test('a resolved address is kept, and the upstream one stands in for it', () => {
  assert.equal(toRow({ kind: 'network', destination: 'a.test', port: 443, resolved_ip: '10.0.0.1' }).resolved_ip, '10.0.0.1');
  assert.equal(toRow({ kind: 'network', destination: 'a.test', port: 443, upstream_ip: '10.0.0.2' }).resolved_ip, '10.0.0.2');
});

test('a failure travels with its message', () => {
  const row = toRow({ kind: 'network', destination: 'a.test', port: 443, error: 'connection refused' });
  assert.equal(row.error, 'connection refused');
});

test('an event only belongs to its own sandbox', () => {
  assert.equal(isSandbox({ sandbox_id: 's1' }, 's1'), true);
  assert.equal(isSandbox({ sandbox_id: 's2' }, 's1'), false);
  assert.equal(isSandbox({ sandbox_id: null }, 's1'), false);
  assert.equal(isSandbox({ sandbox_id: 's1' }, undefined), false);
  assert.equal(isSandbox(null, 's1'), false);
});

test('the newest row is first and the same id replaces the old row', () => {
  let rows = [];
  rows = pushRow(rows, { id: 'a', ts: '1' });
  rows = pushRow(rows, { id: 'b', ts: '2' });
  assert.deepEqual(rows.map((row) => row.id), ['b', 'a']);
  rows = pushRow(rows, { id: 'a', ts: '3' });
  assert.deepEqual(rows.map((row) => row.id), ['a', 'b']);
  assert.equal(rows[0].ts, '3');
});

test('the ring buffer stops at its limit', () => {
  let rows = [];
  for (let index = 0; index < RING + 25; index += 1) rows = pushRow(rows, { id: `e${index}` });
  assert.equal(rows.length, RING);
  assert.equal(rows[0].id, `e${RING + 24}`);
});

test('dropping a missing row leaves the list alone', () => {
  const rows = [{ id: 'a' }];
  assert.equal(pushRow(rows, null), rows);
});
