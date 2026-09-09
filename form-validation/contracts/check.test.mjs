import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fieldCases, inspectObservation, message, wireCases } from './corpus.mjs';

const fields = Array.from({ length: 85 }, (_, i) => ({ id: `case-${i}`, rules: {}, value: 'synthetic', errors: [] }));
const wire = wireCases(fieldCases(fields));
const valid = () => ({
  messages: wire.map(row => ({ id: row.id, accepted: row.expected, value: row.expected ? structuredClone(row.instance) : null })),
  fields: fields.map(row => ({ id: row.id, message: message(row.errors) })),
});

test('complete observation and independent positive/negative corpus are accepted', () => {
  assert.ok(wire.some(row => row.expected));
  assert.ok(wire.some(row => !row.expected));
  assert.doesNotThrow(() => inspectObservation(wire, fields, valid()));
});
test('truncated, duplicate, unknown and incomplete evidence is not a rejection verdict', () => {
  for (const mutate of [
    value => value.messages.pop(),
    value => value.messages.push(value.messages[0]),
    value => { value.messages[1] = value.messages[0]; },
    value => { value.messages[0].id = 'missing'; },
    value => { value.messages[0].accepted = 'true'; },
    value => { value.extra = true; },
    value => value.fields.pop(),
    value => { value.fields[1] = value.fields[0]; },
  ]) {
    const output = valid(); mutate(output);
    assert.throws(() => inspectObservation(wire, fields, output));
  }
});
test('altered round trips, error output and rejected payload leakage fail', () => {
  for (const mutate of [
    value => { value.messages[0].value.schema_version = 'next'; },
    value => { value.messages.find(row => !row.accepted).value = 'synthetic-secret'; },
    value => { value.fields[0].message = message(['required']); },
    value => { value.fields[0].message.value = 'synthetic-secret'; },
  ]) {
    const output = valid(); mutate(output);
    assert.throws(() => inspectObservation(wire, fields, output));
  }
});
test('field fixture count, IDs, shapes and known error codes are mandatory', () => {
  for (const mutate of [
    value => value.pop(),
    value => { value[1].id = value[0].id; },
    value => { value[0].id = '../escape'; },
    value => { value[0].errors = ['unknown']; },
    value => { value[0].extra = true; },
  ]) {
    const input = structuredClone(fields); mutate(input);
    assert.throws(() => fieldCases(input));
  }
});
