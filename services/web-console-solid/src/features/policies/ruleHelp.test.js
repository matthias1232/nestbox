import test from 'node:test';
import assert from 'node:assert/strict';
import { RULE_HELP, MATCHER_HELP, DNS_HELP } from './ruleHelp.js';
import { MATCHER_KINDS } from './policyForms.js';

test('every condition kind of the rule form has an explanation', () => {
  for (const { kind } of MATCHER_KINDS) {
    assert.ok(MATCHER_HELP[kind] && MATCHER_HELP[kind].length > 20, `no help for ${kind}`);
  }
});

test('every field of the rule and DNS forms has an explanation with substance', () => {
  for (const group of [RULE_HELP, DNS_HELP]) {
    for (const [field, text] of Object.entries(group)) {
      assert.ok(text.length > 30, `${field} is not explained`);
    }
  }
});
