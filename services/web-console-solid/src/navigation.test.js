import test from 'node:test';
import assert from 'node:assert/strict';
import { GROUPS, ROUTES, menuFor } from './navigation.js';

const flat = (role) => menuFor(role).flatMap((group) => group.routes.map((route) => route.id));

test('every route sits in exactly one known group', () => {
  const groups = new Set(GROUPS.map((group) => group.id));
  for (const route of ROUTES) assert.ok(groups.has(route.group), route.id);
  const ids = ROUTES.map((route) => route.id);
  assert.equal(new Set(ids).size, ids.length);
});

test('a viewer does not see admin entries, an admin does', () => {
  assert.ok(!flat('viewer').includes('users'));
  assert.ok(!flat('viewer').includes('providers'));
  assert.ok(flat('admin').includes('users'));
});

test('a group with nothing to show is dropped', () => {
  assert.ok(!menuFor('viewer').some((group) => group.id === 'admin'));
});

test('an unknown role sees only unrestricted entries', () => {
  assert.ok(!flat('nobody').includes('users'));
  assert.ok(flat('nobody').includes('sandboxes'));
});
