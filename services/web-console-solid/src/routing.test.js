import test from 'node:test';
import assert from 'node:assert/strict';
import { parsePath, boxPath, terminalPath } from './routing.js';

const ID = '11111111-2222-3333-4444-555555555555';

test('a workbox address names its sandbox', () => {
  assert.deepEqual(parsePath(boxPath(ID)), { kind: 'box', id: ID, tab: null });
});

test('a terminal pop-out carries its tab', () => {
  assert.deepEqual(parsePath(terminalPath(ID, 't-1')), { kind: 'terminal', id: ID, tab: 't-1' });
});

test('a desktop pop-out names its sandbox', () => {
  assert.deepEqual(parsePath(`/box/${ID}/desktop`), { kind: 'desktop', id: ID, tab: null });
});

test('my sandboxes and the admin center have their own kinds', () => {
  assert.equal(parsePath('/my').kind, 'my');
  assert.equal(parsePath(`/sandboxes/${ID}`).kind, 'admin');
});

test('anything else is a menu view', () => {
  assert.equal(parsePath('/').kind, null);
  assert.equal(parsePath('/box/xyz').kind, null);
  assert.equal(parsePath(`/box/${ID}/terminal/bad tab`).kind, null);
});
