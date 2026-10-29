import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  blankRuleForm, blankSpec, coerce, dnsProblem, dnsToPayload, freshGroupName, renameGroup,
  ruleProblem, ruleToPayload, splitList, withField,
} from './policyForms.js';

describe('lists and coercion', () => {
  it('splits on commas and newlines and drops blanks', () => {
    assert.deepEqual(splitList('GET, post\n head ,,'), ['GET', 'post', 'head']);
    assert.deepEqual(splitList(''), []);
  });
  it('removes an empty optional field instead of sending nothing-valued junk', () => {
    assert.equal(coerce({ type: 'integer' }, ''), undefined);
    assert.equal(coerce({ type: 'integer' }, '443'), 443);
    assert.equal(coerce({ type: 'string' }, '  '), undefined);
    assert.deepEqual(coerce({ type: 'path-list' }, '/a, /b'), ['/a', '/b']);
    assert.equal(coerce({ type: 'enum-list' }, []), undefined);
  });
  it('withField deletes on undefined and never mutates', () => {
    const a = { x: 1, y: 2 };
    assert.deepEqual(withField(a, 'x', undefined), { y: 2 });
    assert.deepEqual(a, { x: 1, y: 2 });
  });
});

describe('network groups', () => {
  it('finds a free name', () => {
    assert.equal(freshGroupName({}), 'group');
    assert.equal(freshGroupName({ group: {}, 'group-2': {} }), 'group-3');
  });
  it('renames in place and refuses a clash', () => {
    const groups = { a: 1, b: 2 };
    assert.deepEqual(Object.keys(renameGroup(groups, 'a', 'c')), ['c', 'b']);
    assert.equal(renameGroup(groups, 'a', 'b'), groups);
  });
});

describe('rule form', () => {
  it('turns matchers into the flat fields older readers use', () => {
    const form = { ...blankRuleForm(), name: 'npm', matchers: [{ kind: 'host', value: 'registry.npmjs.org' }, { kind: 'port', value: '443' }], method: 'get', protocol: 'rest' };
    const payload = ruleToPayload(form);
    assert.equal(payload.host, 'registry.npmjs.org');
    assert.equal(payload.port, 443);
    assert.deepEqual(payload.methods, ['GET']);
    assert.equal(payload.matchers.length, 4);
    assert.equal(payload.protocol, 'rest');
  });
  it('keeps a port range out of the flat port', () => {
    const payload = ruleToPayload({ ...blankRuleForm(), name: 'r', matchers: [{ kind: 'port', value: '8000-8100' }] });
    assert.equal(payload.port, null);
  });
  it('says what is missing', () => {
    assert.match(ruleProblem(blankRuleForm()), /name/);
    assert.match(ruleProblem({ ...blankRuleForm(), name: 'x' }), /condition/);
    assert.equal(ruleProblem({ ...blankRuleForm(), name: 'x', matchers: [{ kind: 'any' }] }), null);
  });
});

describe('dns form', () => {
  it('checks the pattern and builds the body', () => {
    assert.match(dnsProblem({ pattern: '' }), /name/);
    assert.match(dnsProblem({ pattern: 'a b' }), /spaces/);
    assert.equal(dnsProblem({ pattern: '.example.net' }), null);
    assert.equal(dnsToPayload({ action: 'deny', pattern: ' .example.net ', priority: '5' }).pattern, '.example.net');
  });
});

describe('blank spec', () => {
  it('is a four-section document the server accepts as a start', () => {
    const spec = blankSpec();
    assert.ok(spec.filesystem_policy && spec.network_policies && spec.process);
  });
});
