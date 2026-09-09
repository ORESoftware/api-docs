import test from 'node:test';
import assert from 'node:assert/strict';
import { compareGoEvidence } from './tjsv-rpc-go.mjs';

const rows = () => [
  { name: 'call', kind: 'call', expected: true, instance: { v: 1, body: null } },
  { name: 'receipt', kind: 'receipt', expected: false, instance: { ok: null } },
];
const evidence = () => ({
  schema: 'ores.api-docs.rpc-adapter/v1', runtime: 'go', toolchain: 'go1.23.2',
  results: [
    { name: 'call', kind: 'call', accepted: true, encoded: '{"body":null,"v":1}' },
    { name: 'receipt', kind: 'receipt', accepted: false },
  ],
});
test('accepts complete native evidence without depending on object key order', () => {
  assert.equal(compareGoEvidence(rows(), evidence()).status, 'passed');
});
for (const [name, mutate] of [
  ['unknown envelope field', e => { e.extra = true; }],
  ['missing schema', e => { delete e.schema; }],
  ['wrong profile', e => { e.schema = 'wrong'; }],
  ['wrong runtime', e => { e.runtime = 'typescript'; }],
  ['missing toolchain', e => { delete e.toolchain; }],
  ['wrong toolchain', e => { e.toolchain = 'unknown'; }],
  ['absent results', e => { delete e.results; }],
  ['missing result', e => { e.results.pop(); }],
  ['extra result', e => { e.results.push(e.results[0]); }],
  ['null results', e => { e.results = null; }],
  ['reordered results', e => { e.results.reverse(); }],
  ['duplicate identity', e => { e.results[1] = e.results[0]; }],
  ['wrong kind', e => { e.results[0].kind = 'receipt'; }],
  ['null verdict', e => { e.results[0] = null; }],
  ['numeric acceptance', e => { e.results[0].accepted = 1; }],
  ['missing acceptance', e => { delete e.results[0].accepted; }],
  ['unknown result field', e => { e.results[0].error = 'ignored'; }],
  ['missing encoded value', e => { delete e.results[0].encoded; }],
  ['nonstring encoded value', e => { e.results[0].encoded = {}; }],
  ['malformed JSON output', e => { e.results[0].encoded = '{'; }],
  ['sanitized null body', e => { e.results[0].encoded = '{"v":1}'; }],
  ['changed value', e => { e.results[0].encoded = '{"v":2,"body":null}'; }],
  ['encoded rejected value', e => { e.results[1].encoded = '{"ok":null}'; }],
]) {
  test(`fails closed: ${name}`, () => { const e = evidence(); mutate(e); assert.throws(() => compareGoEvidence(rows(), e)); });
}
test('a real rejection of a valid fixture stops evaluation', () => {
  const e = evidence(); e.results[0].accepted = false; delete e.results[0].encoded;
  assert.equal(compareGoEvidence(rows(), e).status, 'stopped_for_evaluation');
});
test('acceptance of an invalid fixture stops evaluation', () => {
  const e = evidence(); e.results[1].accepted = true; e.results[1].encoded = '{"ok":null}';
  assert.equal(compareGoEvidence(rows(), e).status, 'stopped_for_evaluation');
});
test('empty input is not successful evidence', () => assert.throws(() => compareGoEvidence([], evidence())));
