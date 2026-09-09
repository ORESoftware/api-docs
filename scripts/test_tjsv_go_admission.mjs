import test from 'node:test';
import assert from 'node:assert/strict';
import { GO_INPUTS, mergeGoAdmission } from './tjsv-go-admission.mjs';

const rows = () => [
  { name: 'valid-call', kind: 'call', expected: true, tjsvAccepted: true, typescriptAccepted: true, rustAccepted: true },
  { name: 'invalid-receipt', kind: 'receipt', expected: false, tjsvAccepted: false, typescriptAccepted: false, rustAccepted: false },
];
const prior = () => ({ status: 'passed', results: rows(), findings: [] });
const go = () => ({
  status: 'passed',
  results: [
    { name: 'valid-call', kind: 'call', expected: true, accepted: true, preserved: true },
    { name: 'invalid-receipt', kind: 'receipt', expected: false, accepted: false, preserved: null },
  ],
  findings: [],
});

test('Go admission gates status without changing the stable top-level result schema', () => {
  const result = mergeGoAdmission(prior(), go());
  assert.equal(result.status, 'passed');
  assert.deepEqual(result.results, rows());
  assert.deepEqual(Object.keys(result.results[0]).sort(), ['expected', 'kind', 'name', 'rustAccepted', 'tjsvAccepted', 'typescriptAccepted']);
});

test('Go acceptance or value-preservation drift stops evaluation', () => {
  for (const mutate of [
    evidence => {
      evidence.results[1].accepted = true;
      evidence.results[1].preserved = true;
      evidence.findings = [evidence.results[1]];
      evidence.status = 'stopped_for_evaluation';
    },
    evidence => {
      evidence.results[0].preserved = false;
      evidence.findings = [evidence.results[0]];
      evidence.status = 'stopped_for_evaluation';
    },
  ]) {
    const evidence = go();
    mutate(evidence);
    const result = mergeGoAdmission(prior(), evidence);
    assert.equal(result.status, 'stopped_for_evaluation');
    assert.equal(result.findings.length, 1);
    assert.equal(result.findings[0].runtime, 'go');
  }
});

test('prior Rust/TJSV drift remains blocking when Go passes', () => {
  const existing = prior();
  existing.status = 'stopped_for_evaluation';
  existing.findings = [existing.results[1]];
  assert.equal(mergeGoAdmission(existing, go()).status, 'stopped_for_evaluation');
});

for (const [name, mutate] of [
  ['missing results', evidence => { delete evidence.results; }],
  ['coverage mismatch', evidence => { evidence.results.pop(); }],
  ['wrong identity', evidence => { evidence.results[0].name = 'other'; }],
  ['string verdict', evidence => { evidence.results[0].accepted = 'true'; }],
  ['inconsistent pass', evidence => { evidence.findings = [{}]; }],
  ['inconsistent stop', evidence => { evidence.status = 'stopped_for_evaluation'; }],
]) test(`malformed Go evidence fails closed: ${name}`, () => {
  const evidence = go();
  mutate(evidence);
  assert.throws(() => mergeGoAdmission(prior(), evidence));
});

test('Go source manifest is sorted, closed and covers client, probe, workflow, helper and protocol', () => {
  assert.equal(Object.isFrozen(GO_INPUTS), true);
  assert.deepEqual(GO_INPUTS, [...GO_INPUTS].sort());
  assert.equal(new Set(GO_INPUTS).size, GO_INPUTS.length);
  for (const path of [
    '.github/workflows/tjsv-rpc-admission.yml',
    'clients/go/decode.go',
    'clients/go/go.mod',
    'clients/go/testdata/tjsv_probe/main.go',
    'scripts/tjsv-go-admission.mjs',
    'scripts/tjsv-rpc-runtime-protocol.mjs',
  ]) assert.ok(GO_INPUTS.includes(path), `missing ${path}`);
  assert.throws(() => GO_INPUTS.push('unreviewed'));
});
