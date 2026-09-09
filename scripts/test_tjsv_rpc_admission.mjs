import test from 'node:test';
import assert from 'node:assert/strict';
import { readCases, compareCorpus, PROFILE, TJSV_REVISION } from './tjsv-rpc-admission.mjs';

class Rejected extends Error {}
function fixture(name, kind, accepted) {
  const encoded = JSON.stringify({ accepted, kind });
  return { name, kind, encoded, ...(accepted ? { tcp_prefix_hex: Buffer.byteLength(encoded).toString(16).padStart(8, '0') } : {}) };
}
function corpus() {
  return { schemaVersion: 1, profile: PROFILE, maxFrameBytes: 8388608, tcpLengthPrefixBytes: 4,
    valid: [fixture('valid-call', 'call', true), fixture('valid-receipt', 'receipt', true)],
    invalid: [fixture('invalid-call', 'call', false), fixture('invalid-receipt', 'receipt', false)] };
}
const validate = (_, instance) => ({ valid: instance.accepted, errors: instance.accepted ? [] : [{ keyword: 'test' }] });
const decoder = encoded => { const value = JSON.parse(encoded); if (!value.accepted) throw new Rejected(); return value; };
const accept = encoded => JSON.parse(encoded);
const decode = { call: decoder, receipt: decoder };
const rejected = error => error instanceof Rejected;
const run = input => compareCorpus(input, validate, decode, rejected);

test('harness checks both kinds and both expectations, deterministically', () => {
  const report = run(corpus());
  assert.equal(report.status, 'passed');
  assert.equal(report.results.length, 4);
  assert.deepEqual(run(corpus()), report);
  assert.match(TJSV_REVISION, /^[a-f0-9]{40}$/);
});
const mutations = [
  ['empty positives', c => { c.valid = []; }],
  ['empty negatives', c => { c.invalid = []; }],
  ['missing group', c => { delete c.invalid; }],
  ['missing receipt coverage', c => { c.valid.pop(); }],
  ['missing invalid call coverage', c => { c.invalid.shift(); }],
  ['unknown profile', c => { c.profile = 'ridl-v2'; }],
  ['wrong version', c => { c.schemaVersion = 2; }],
  ['wrong frame bound', c => { c.maxFrameBytes = 1; }],
  ['wrong prefix size', c => { c.tcpLengthPrefixBytes = 8; }],
  ['extra corpus property', c => { c.skip = true; }],
  ['duplicate fixture', c => { c.invalid[0].name = c.valid[0].name; }],
  ['invalid fixture name', c => { c.valid[0].name = '../escape'; }],
  ['unknown fixture kind', c => { c.valid[0].kind = 'data'; }],
  ['extra fixture property', c => { c.valid[0].skip = true; }],
  ['malformed JSON is not a negative schema verdict', c => { c.invalid[0].encoded = '{'; }],
  ['missing encoded JSON', c => { delete c.invalid[0].encoded; }],
  ['missing TCP prefix', c => { delete c.valid[0].tcp_prefix_hex; }],
  ['wrong TCP byte count', c => { c.valid[0].tcp_prefix_hex = '00000001'; }],
];
for (const [name, mutate] of mutations) test(`refuses ${name}`, () => {
  const value = corpus(); mutate(value); assert.throws(() => run(value));
});
test('UTF-8 prefix measures bytes, not JavaScript code units', () => {
  const value = corpus();
  value.valid[0].encoded = JSON.stringify({ accepted: true, body: 'é😀' });
  value.valid[0].tcp_prefix_hex = Buffer.byteLength(value.valid[0].encoded).toString(16).padStart(8, '0');
  assert.equal(run(value).status, 'passed');
  value.valid[0].tcp_prefix_hex = value.valid[0].encoded.length.toString(16).padStart(8, '0');
  assert.throws(() => run(value));
});
test('both adapters agreeing on the wrong answer is a failure', () => {
  const result = compareCorpus(corpus(), () => ({ valid: true, errors: [] }), { call: accept, receipt: accept }, rejected);
  assert.equal(result.status, 'stopped_for_evaluation');
  assert.equal(result.findings.length, 2);
});
test('rejecting all valid fixtures fails', () => {
  const result = compareCorpus(corpus(), () => ({ valid: false, errors: [{}] }), decode, rejected);
  assert.equal(result.findings.length, 2);
});
test('runtime-only drift fails', () => {
  const result = compareCorpus(corpus(), validate, { call: accept, receipt: accept }, rejected);
  assert.equal(result.findings.length, 2);
});
test('validator refusal is an execution failure, not rejection', () => {
  assert.throws(() => compareCorpus(corpus(), () => { throw new Error('unsupported keyword'); }, decode, rejected), /unsupported keyword/);
});
test('unexpected decoder crashes are not accepted negative evidence', () => {
  assert.throws(() => compareCorpus(corpus(), validate, { call() { throw new TypeError('bug'); }, receipt: decoder }, rejected), /bug/);
});
for (const verdict of [null, {}, { valid: 'false', errors: [] }, { valid: true }, { valid: true, errors: [{}] }, { valid: false, errors: [] }, Promise.resolve({ valid: true, errors: [] })]) {
  test(`refuses malformed or asynchronous validator verdict ${String(verdict)}`, () => {
    assert.throws(() => compareCorpus(corpus(), () => verdict, decode, rejected), /verdict/);
  });
}
test('refuses missing adapters', () => {
  assert.throws(() => compareCorpus(corpus(), validate, {}, rejected), /adapter/);
  assert.throws(() => compareCorpus(corpus(), null, decode, rejected), /adapter/);
});
test('refuses non-object corpus', () => {
  for (const value of [null, [], false, 1, 'fixture']) assert.throws(() => readCases(value));
});

test('silent runtime sanitization and asynchronous decoders fail', () => {
  for (const call of [() => ({}), () => undefined, async encoded => JSON.parse(encoded)]) {
    assert.throws(() => compareCorpus(corpus(), validate, { call, receipt: decoder }, rejected), /changed decoded value/);
  }
});
