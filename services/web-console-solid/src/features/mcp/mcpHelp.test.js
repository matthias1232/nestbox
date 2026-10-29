import test from 'node:test';
import assert from 'node:assert/strict';
import { MCP_HELP } from './mcpHelp.js';

test('every field of the MCP server form has an explanation with substance', () => {
  for (const field of ['key', 'name', 'transport', 'endpoint', 'command', 'args', 'auth_type', 'env_slots', 'packs', 'enabled', 'description']) {
    assert.ok(MCP_HELP[field] && MCP_HELP[field].length > 25, `${field} is not explained`);
  }
});
