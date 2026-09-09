import test from 'node:test';
import assert from 'node:assert/strict';
import { compareRustResults, RUST_REPORT_SCHEMA } from './tjsv-rust-admission.mjs';

function fixture() {
  const rows = [
    { name: 'call-valid', kind: 'call', expected: true, instance: { v: 1, op: 'call', id: 'c1', key: 'get_item' } },
    { name: 'receipt-valid', kind: 'receipt', expected: true, instance: { v: 1, op: 'receipt', id: 'c1', key: 'get_item', ok: true, body: null } },
    { name: 'call-invalid', kind: 'call', expected: false, instance: { v: 2, op: 'call', id: 'c1', key: 'get_item' } },
    { name: 'receipt-invalid', kind: 'receipt', expected: false, instance: { v: 1, op: 'receipt', id: 'c1', key: 'get_item', ok: false } },
  ];
  const schema = { status: 'passed', findings: [], results: rows.map(row => ({
    name: row.name, kind: row.kind, expected: row.expected,
    tjsvAccepted: row.expected, typescriptAccepted: row.expected,
  })) };
  const rust = { schema: RUST_REPORT_SCHEMA, results: rows.map(row => ({
    name: row.name, kind: row.kind, accepted: row.expected,
    ...(row.expected ? { decoded: structuredClone(row.instance) } : {}),
  })) };
  return { rows, schema, rust };
}
const compare = value => compareRustResults(value.rows, value.schema, value.rust);

test('all three real-verdict columns must match the corpus expectation', () => {
  const value = fixture();
  const before = structuredClone(value);
  const result = compare(value);
  assert.equal(result.status, 'passed');
  assert.equal(result.results.length, 4);
  assert.deepEqual(result.findings, []);
  assert.deepEqual(result.results.map(row => row.rustAccepted), [true, true, false, false]);
  assert.deepEqual(value, before, 'input evidence must not be mutated');
});

for (const [name, mutate] of [
  ['absent report', value => { value.rust = undefined; }],
  ['wrong report version', value => { value.rust.schema = 'other/v1'; }],
  ['extra summary pass flag', value => { value.rust.passed = true; }],
  ['empty results', value => { value.rust.results = []; }],
  ['missing row', value => { value.rust.results.pop(); }],
  ['extra row', value => { value.rust.results.push(value.rust.results[0]); }],
  ['duplicate identity', value => { value.rust.results[1] = structuredClone(value.rust.results[0]); }],
  ['reordered results', value => { value.rust.results.reverse(); }],
  ['wrong runtime kind', value => { value.rust.results[0].kind = 'receipt'; }],
  ['coerced boolean', value => { value.rust.results[0].accepted = 'true'; }],
  ['missing accepted value', value => { delete value.rust.results[0].accepted; }],
  ['missing decoded value', value => { delete value.rust.results[0].decoded; }],
  ['decoded value on rejection', value => { value.rust.results[2].decoded = null; }],
  ['unexpected verdict field', value => { value.rust.results[0].ignored = true; }],
  ['changed correlation id', value => { value.rust.results[0].decoded.id = 'other'; }],
  ['absent body changed to null', value => { value.rust.results[0].decoded.body = null; }],
  ['null body dropped', value => { delete value.rust.results[1].decoded.body; }],
  ['schema evidence missing', value => { value.schema = undefined; }],
  ['schema evidence incomplete', value => { value.schema.results.pop(); }],
  ['schema identity changed', value => { value.schema.results[0].name = 'other'; }],
  ['schema expected changed', value => { value.schema.results[0].expected = false; }],
  ['schema boolean coerced', value => { value.schema.results[0].tjsvAccepted = 1; }],
  ['TypeScript verdict missing', value => { delete value.schema.results[0].typescriptAccepted; }],
  ['empty input', value => { value.rows = []; }],
  ['duplicate input identity', value => { value.rows[1].name = value.rows[0].name; }],
]) {
  test(`fails closed: ${name}`, () => {
    const value = fixture();
    mutate(value);
    assert.throws(() => compare(value));
  });
}

for (const runtime of ['tjsvAccepted', 'typescriptAccepted', 'rustAccepted']) {
  for (const index of [0, 2]) {
    test(`${runtime} drift on ${index === 0 ? 'valid' : 'invalid'} fixture stops admission`, () => {
      const value = fixture();
      if (runtime === 'rustAccepted') {
        const row = value.rust.results[index];
        row.accepted = !row.accepted;
        if (row.accepted) row.decoded = structuredClone(value.rows[index].instance);
        else delete row.decoded;
      } else value.schema.results[index][runtime] = !value.rows[index].expected;
      const result = compare(value);
      assert.equal(result.status, 'stopped_for_evaluation');
      assert.equal(result.findings.length, 1);
      assert.equal(result.findings[0].name, value.rows[index].name);
    });
  }
}

test('agreement on the wrong result is still drift despite a forged passed summary', () => {
  const value = fixture();
  value.schema.results[0].tjsvAccepted = false;
  value.schema.results[0].typescriptAccepted = false;
  value.rust.results[0].accepted = false;
  delete value.rust.results[0].decoded;
  const result = compare(value);
  assert.equal(result.status, 'stopped_for_evaluation');
  assert.equal(result.findings.length, 1);
});
