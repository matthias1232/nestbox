import test from 'node:test';
import assert from 'node:assert/strict';

// The classifier and the splitter live in a .jsx file; load only the pure parts by reading them.
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./LogView.jsx', import.meta.url), 'utf8');
const pure = source.slice(source.indexOf('const LEVELS'), source.indexOf('export default function LogView'));
const { classify, splitLine } = new Function(`${pure.replaceAll('export ', '')}; return { classify, splitLine };`)();

test('refusals and errors are problems, allowed lines are good, the rest is plain', () => {
  assert.equal(classify('NET:OPEN [MED] DENIED /usr/bin/curl -> example.com:443'), 'bad');
  assert.equal(classify('policy loaded WARN something'), 'warn');
  assert.equal(classify('NET:OPEN ALLOWED host:443'), 'good');
  assert.equal(classify('hello world'), 'plain');
});

test('a line splits into time, source and message, and a bare line stays whole', () => {
  assert.deepEqual(splitLine('2026-10-29T13:55:01.123Z [sandbox] NET:OPEN ALLOWED x'), {
    time: '13:55:01', source: 'sandbox', text: 'NET:OPEN ALLOWED x',
  });
  assert.deepEqual(splitLine('no time here'), { time: '', source: '', text: 'no time here' });
});
