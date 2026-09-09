import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parseProfile } from '../tmp/dist/profiles.js';

const corpus = JSON.parse(readFileSync(new URL('../../admission-profiles/corpus.json', import.meta.url)));
for (const row of corpus.cases) test(`shared contract: ${row.id}`, () => {
  const parsed = parseProfile(row.profile, row.input);
  assert.equal(parsed.success, row.expected);
  assert.ok(Object.isFrozen(parsed));
  if (parsed.success) {
    assert.deepEqual(parsed.data, row.input);
    assert.ok(Object.isFrozen(parsed.data));
  } else {
    assert.deepEqual(parsed, { success: false, code: 'invalid_submission' });
  }
});

test('Zod counts code points without normalizing or silently truncating input', () => {
  for (const value of ['😀'.repeat(80), 'e\u0301'.repeat(40), '-0', ' a ']) {
    const result = parseProfile('TextSubmission', { value });
    assert.equal(result.success, true);
    assert.equal(result.data.value, value);
  }
  for (const value of ['😀'.repeat(81), 'e\u0301'.repeat(41), '\ud800', '\udc00']) {
    assert.equal(parseProfile('TextSubmission', { value }).success, false);
  }
});

test('error result contains no submitted value, unknown key or library error', () => {
  const secret = 'synthetic-sensitive-value';
  const result = parseProfile('PhoneSubmission', { value: secret, [secret]: true });
  assert.deepEqual(result, { success: false, code: 'invalid_submission' });
  assert.equal(JSON.stringify(result).includes(secret), false);
});

test('no ordinary getters, inherited values, class instances or symbol fields', () => {
  let reads = 0;
  const accessor = { get value() { reads++; return 'valid'; } };
  class Model { value = 'valid'; }
  for (const input of [accessor, new Model(), Object.create({ value: 'valid' }), { value: 'valid', [Symbol('hidden')]: true }]) {
    assert.equal(parseProfile('TextSubmission', input).success, false);
  }
  assert.equal(reads, 0);
});

test('input is not mutated; parsed data is a separate readonly value', () => {
  const input = { value: 'name' };
  const result = parseProfile('TextSubmission', input);
  assert.equal(result.success, true);
  assert.notEqual(result.data, input);
  assert.equal(Object.isFrozen(input), false);
  input.value = 'changed';
  assert.equal(result.data.value, 'name');
  assert.throws(() => { result.data.value = 'mutated'; }, TypeError);
});

test('unknown profile is a configuration error, not a negative verdict', () => {
  for (const profile of ['Unknown', 'toString', '__proto__']) {
    assert.throws(() => parseProfile(profile, {}), /unsupported admission profile/);
  }
});
