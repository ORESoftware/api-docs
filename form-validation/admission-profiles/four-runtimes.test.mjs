import test from 'node:test';
import assert from 'node:assert/strict';
import { PROFILES, RUNTIMES, CORPUS_SCHEMA, RESULT_SCHEMA, MARKER, compareEvidence } from './evidence.mjs';

// Synthetic verdicts test orchestration only, never substitute for the actual
// pinned-TJSV/native/Dart/Zod execution in check.mjs.
const corpus = { schema: CORPUS_SCHEMA, cases: PROFILES.flatMap((profile, n) => [
  { id: `valid-${n}`, profile, input: { value: 'valid' }, expected: true },
  { id: `invalid-${n}`, profile, input: { value: '' }, expected: false },
]) };
const response = () => ({ schema: RESULT_SCHEMA, results: corpus.cases.map(row => ({
  id: row.id, profile: row.profile, accepted: row.expected, preserved: row.expected ? true : null,
})) });
const encode = value => MARKER + JSON.stringify(value);
const outputs = () => Object.fromEntries(RUNTIMES.map(runtime => [runtime, encode(response())]));
const oracle = (_profile, input) => ({ valid: input.value.length > 0, errors: input.value.length > 0 ? [] : ['invalid'] });

test('TypeScript/Zod is mandatory alongside all three existing runtimes', () => {
  assert.deepEqual(RUNTIMES, ['rust-native', 'dart-vm', 'dart-javascript', 'typescript-zod']);
  assert.equal(compareEvidence(corpus, oracle, outputs()).status, 'passed');
});
for (const runtime of RUNTIMES) {
  test(`${runtime}: missing evidence fails closed`, () => {
    const values = outputs();
    delete values[runtime];
    assert.throws(() => compareEvidence(corpus, oracle, values), /missing or unexpected runtime/);
  });
  test(`${runtime}: accepted invalid input stops evaluation`, () => {
    const values = outputs();
    const value = response();
    value.results[1].accepted = true;
    value.results[1].preserved = true;
    values[runtime] = encode(value);
    assert.equal(compareEvidence(corpus, oracle, values).status, 'stopped_for_evaluation');
  });
  test(`${runtime}: normalization is not successful evidence`, () => {
    const values = outputs();
    const value = response();
    value.results[0].preserved = false;
    values[runtime] = encode(value);
    assert.throws(() => compareEvidence(corpus, oracle, values), /normalized/);
  });
}
