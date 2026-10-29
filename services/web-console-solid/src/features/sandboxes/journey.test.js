import test from 'node:test';
import assert from 'node:assert/strict';
import { journeySteps } from './journey.js';

const base = { host: 'example.com', port: 443, protocol: 'tcp', decision: 'deny', binary: '/usr/bin/curl', reason_text: 'transparent_tcp_policy_denied' };

test('a denied connection stops at the decision and never contacts the destination', () => {
  const steps = journeySteps(base);
  assert.equal(steps.length, 5);
  assert.deepEqual(steps.map((s) => s.n), [1, 2, 3, 4, 5]);
  assert.match(steps[0].title, /bin\/curl|curl/);
  assert.equal(steps[3].title, 'Denied');
  assert.match(steps[3].says, /transparent_tcp_policy_denied/);
  assert.match(steps[4].title, /never contacted/);
});

test('an allowed connection names the policy that matched and the tool that was called', () => {
  const steps = journeySteps({ ...base, decision: 'allow', host: 'host.openshell.internal', port: 18088, protocol: 'mcp', mcp_tool: 'run_command', reason_text: 'policy nb1_mcp_server__nestbox_hub', status: 200 });
  assert.match(steps[2].title, /nb1_mcp_server__nestbox_hub/);
  assert.equal(steps[3].title, 'Allowed');
  assert.match(steps[3].says, /run_command/);
  assert.match(steps[4].title, /is reached.*200/);
});

test('an audit rule says it lets the connection through and records it', () => {
  assert.match(journeySteps({ ...base, decision: 'audit' })[3].says, /audit mode/);
});

test('an event without a program still reads sensibly', () => {
  const steps = journeySteps({ ...base, binary: null });
  assert.match(steps[0].title, /A program opens/);
});
