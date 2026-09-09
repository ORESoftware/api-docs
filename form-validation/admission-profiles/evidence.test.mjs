import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PROFILES, RUNTIMES, RESULT_SCHEMA, MARKER, readCases, readRuntime, compareEvidence } from './evidence.mjs';

const corpus = JSON.parse(readFileSync(new URL('./corpus.json', import.meta.url)));
const cases = readCases(corpus);
const envelope = () => ({ schema: RESULT_SCHEMA, results: cases.map(row => ({ id: row.id, profile: row.profile, accepted: row.expected, preserved: row.expected ? true : null })) });
const encode = value => `${MARKER}${JSON.stringify(value)}\n`;
const outputs = () => Object.fromEntries(RUNTIMES.map(name => [name, encode(envelope())]));
const schema = (profile, input) => {
  const row = cases.find(row => row.profile === profile && JSON.stringify(row.input) === JSON.stringify(input));
  assert.ok(row);
  return { valid: row.expected, errors: row.expected ? [] : [{ code: 'rejected' }] };
};

test('complete shared fixtures agree and receipts never repeat submitted values', () => {
  const result = compareEvidence(corpus, schema, outputs());
  assert.equal(result.status, 'passed');
  assert.equal(result.results.length, 78);
  assert.equal(result.findings.length, 0);
  assert.equal(JSON.stringify(result).includes('+12025550123'), false);
  assert.equal(PROFILES.length, 3);
});

for (const [name, mutate] of [
  ['empty', c => { c.cases = []; }],
  ['unknown root key', c => { c.extra = true; }],
  ['wrong version', c => { c.schema = 'ores.form-admission.corpus/v2'; }],
  ['duplicate id', c => { c.cases.push(c.cases[0]); }],
  ['missing expectation', c => { delete c.cases[0].expected; }],
  ['nonboolean expectation', c => { c.cases[0].expected = 'true'; }],
  ['unknown profile', c => { c.cases[0].profile = 'NoSuchProfile'; }],
  ['path traversal id', c => { c.cases[0].id = '../bad'; }],
  ['unknown case key', c => { c.cases[0].ignored = true; }],
  ['missing negative coverage', c => { c.cases = c.cases.filter(row => row.expected); }],
  ['oversized input', c => { c.cases[0].input = 'x'.repeat(100001); }],
]) test(`reject corpus: ${name}`, () => {
  const copy = structuredClone(corpus);
  mutate(copy);
  assert.throws(() => readCases(copy));
});

for (const [name, mutate] of [
  ['missing row', e => { e.results.pop(); }],
  ['duplicate row', e => { e.results[1] = e.results[0]; }],
  ['unknown row', e => { e.results[0].id = 'invented'; }],
  ['wrong profile', e => { e.results[0].profile = 'PhoneSubmission'; }],
  ['string verdict', e => { e.results[0].accepted = 'true'; }],
  ['normalized value', e => { e.results[0].preserved = false; }],
  ['rejection claims preservation', e => { e.results.find(row => !row.accepted).preserved = true; }],
  ['unexpected raw value', e => { e.results[0].value = 'private'; }],
  ['wrong version', e => { e.schema = 'ores.form-admission.runtime/v2'; }],
  ['unknown envelope key', e => { e.cache = true; }],
]) test(`reject runtime evidence: ${name}`, () => {
  const value = envelope();
  mutate(value);
  assert.throws(() => readRuntime(encode(value), cases));
});

test('missing, duplicate and malformed envelopes are not successful rejections', () => {
  for (const stdout of ['', 'test passed', encode(envelope()).repeat(2), `${MARKER}{not-json}`]) {
    assert.throws(() => readRuntime(stdout, cases));
  }
});

test('runtime result order is independent from corpus order', () => {
  const e = envelope();
  e.results.reverse();
  assert.equal(readRuntime(encode(e), cases).size, cases.length);
});

test('missing runtime cannot masquerade as complete parity', () => {
  const value = outputs();
  delete value['dart-javascript'];
  assert.throws(() => compareEvidence(corpus, schema, value));
});

test('schema exceptions and refusals are infrastructure failures, not negative passes', () => {
  assert.throws(() => compareEvidence(corpus, () => { throw new Error('unsupported keyword'); }, outputs()), /unsupported keyword/);
  for (const verdict of [null, true, { valid: false, errors: [] }, { valid: true, errors: ['bad'] }, { valid: 'false', errors: [] }]) {
    assert.throws(() => compareEvidence(corpus, () => verdict, outputs()));
  }
});

test('schema and every runtime must match the independently declared expectation', () => {
  assert.equal(compareEvidence(corpus, () => ({ valid: true, errors: [] }), outputs()).status, 'stopped_for_evaluation');
  for (const name of RUNTIMES) {
    const value = outputs();
    const e = envelope();
    const rejected = e.results.find(row => !row.accepted);
    rejected.accepted = true;
    rejected.preserved = true;
    value[name] = encode(e);
    const result = compareEvidence(corpus, schema, value);
    assert.equal(result.status, 'stopped_for_evaluation');
    assert.equal(result.findings.length, 1);
  }
});
