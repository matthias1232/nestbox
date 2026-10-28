import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { groupStacks, possibleLeads, worstStatus } from './stacks.js';

const sb = (id, name, parent_id = null, status = 'running') => ({ id, name, parent_id, status });

describe('groupStacks', () => {
  it('puts members under their lead and keeps leads in server order', () => {
    const rows = groupStacks([sb('1', 'hermes'), sb('2', 'model', '1'), sb('3', 'solo'), sb('4', 'cache', '1')]);
    assert.deepEqual(rows.map((r) => [r.sandbox.name, r.role]), [['hermes', 'lead'], ['solo', 'single']]);
    assert.deepEqual(rows[0].members.map((m) => m.name), ['cache', 'model']);
  });

  it('shows a member whose lead is on another page as a single row that names its lead', () => {
    const rows = groupStacks([sb('2', 'model', '9')]);
    assert.equal(rows.length, 1);
    assert.equal(rows[0].role, 'single');
    assert.equal(rows[0].orphanOf, '9');
  });
});

describe('possibleLeads', () => {
  it('offers standalone sandboxes and leads, never itself or a member', () => {
    const items = [sb('1', 'a'), sb('2', 'b', '1'), sb('3', 'c')];
    assert.deepEqual(possibleLeads(items, items[2]).map((s) => s.id), ['1']);
  });

  it('offers nothing to a lead that has members', () => {
    const items = [sb('1', 'a'), sb('2', 'b', '1'), sb('3', 'c')];
    assert.deepEqual(possibleLeads(items, items[0]), []);
  });
});

describe('worstStatus', () => {
  it('reports the failure of any member', () => {
    assert.equal(worstStatus(['running', 'failed', 'stopped']), 'failed');
    assert.equal(worstStatus(['running', 'running']), 'running');
  });
});
