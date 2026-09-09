import test from 'node:test';
import assert from 'node:assert/strict';
import { makeProbeRequest, assessProbeResponse, readProbeExecution, verifyOracleReceipt, REQUEST_SCHEMA, RESPONSE_SCHEMA, MAX_PROTOCOL_BYTES } from './tjsv-rpc-runtime-protocol.mjs';
const rows = () => [
  { name: 'valid-call', kind: 'call', encoded: '{"body":null}', instance: { body: null }, expected: true },
  { name: 'invalid-receipt', kind: 'receipt', encoded: '{}', instance: {}, expected: false },
];
const response = () => ({ schema: RESPONSE_SCHEMA, runtime: 'rust', results: [
  { name: 'valid-call', kind: 'call', accepted: true, encoded: '{"body":null}' },
  { name: 'invalid-receipt', kind: 'receipt', accepted: false },
] });
const assess = value => assessProbeResponse(rows(), value, 'rust');
test('request omits expectations and parsed instances', () => {
  const request = JSON.parse(makeProbeRequest(rows()));
  assert.equal(request.schema, REQUEST_SCHEMA);
  for (const row of request.cases) assert.deepEqual(Object.keys(row).sort(), ['encoded', 'kind', 'name']);
});
test('correct evidence passes deterministically', () => {
  assert.deepEqual(assess(response()), assess(response()));
  assert.equal(assess(response()).status, 'passed');
});
const mutations = [
  ['missing runtime', r => { delete r.runtime; }],
  ['wrong runtime', r => { r.runtime = 'go'; }],
  ['wrong schema', r => { r.schema = REQUEST_SCHEMA; }],
  ['extra report fields', r => { r.passed = true; }],
  ['empty evidence', r => { r.results = []; }],
  ['missing negative evidence', r => { r.results.pop(); }],
  ['extra evidence', r => { r.results.push(r.results[0]); }],
  ['wrong order', r => { r.results.reverse(); }],
  ['duplicate case', r => { r.results[1] = r.results[0]; }],
  ['wrong kind', r => { r.results[0].kind = 'receipt'; }],
  ['coerced acceptance', r => { r.results[1].accepted = 'false'; }],
  ['missing acceptance', r => { delete r.results[1].accepted; }],
  ['unexpected result field', r => { r.results[0].expected = true; }],
  ['missing accepted payload', r => { delete r.results[0].encoded; }],
  ['rejected payload', r => { r.results[1].encoded = '{}'; }],
  ['invalid JSON payload', r => { r.results[0].encoded = '{'; }],
  ['non-string payload', r => { r.results[0].encoded = {}; }],
];
for (const [name, mutate] of mutations) test(`refuses ${name}`, () => {
  const value = response(); mutate(value); assert.throws(() => assess(value));
});
for (const encoding of ['{}', '{"body":1}', '{"body":null,"extra":true}']) test(`detects decoded-value drift ${encoding}`, () => {
  const value = response(); value.results[0].encoded = encoding;
  assert.equal(assess(value).status, 'stopped_for_evaluation');
});
test('both false rejection and false acceptance fail', () => {
  const value = response(); delete value.results[0].encoded; value.results[0].accepted = false;
  value.results[1].accepted = true; value.results[1].encoded = '{}';
  assert.equal(assess(value).findings.length, 2);
});
for (const invalid of [null, [], false, {}, { status: null }, { status: 3, stdout: '{"accepted":false}' }, { status: 0, error: new Error('spawn') }, { status: 0, signal: 'SIGKILL' }, { status: 0, stdout: '' }, { status: 0, stdout: '{}\n{}' }]) {
  test('crashes, partial output and absent execution are never rejection evidence', () => assert.throws(() => readProbeExecution(invalid)));
}
test('execution output parses only after process success', () => assert.deepEqual(readProbeExecution({ status: 0, signal: null, stdout: JSON.stringify(response()) }), response()));
for (const invalid of [[], null, [{ ...rows()[0], kind: 'data' }], [rows()[0], rows()[0]], [{ ...rows()[0], encoded: false }]]) {
  test('invalid request fails before execution', () => assert.throws(() => makeProbeRequest(invalid)));
}
test('input and output byte budgets fail closed', () => {
  assert.throws(() => makeProbeRequest([{ ...rows()[0], encoded: 'x'.repeat(MAX_PROTOCOL_BYTES) }]));
  assert.throws(() => readProbeExecution({ status: 0, stdout: ' '.repeat(MAX_PROTOCOL_BYTES + 1) }));
});
const paths = ['examples/rpc-v1/conformance.json', 'json-schema/rpc-call.schema.json', 'json-schema/rpc-receipt.schema.json', 'idl/typespec/v1.tsp', 'runtime/v1-conformance.json', 'clients/typescript/src/rpc.js', 'scripts/tjsv-rpc-admission.mjs'];
const digests = Object.fromEntries(paths.map(path => [path, '1'.repeat(64)]));
function receipt() {
  return { schema: 'ores.api-docs.tjsv-rpc-admission/v1', sourceRevision: '2'.repeat(40), profile: 'ores-rpc-v1-call-receipt', status: 'passed', findings: [],
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: '3'.repeat(40) }, sourceDigests: { ...digests },
    results: rows().map(row => ({ name: row.name, kind: row.kind, expected: row.expected, tjsvAccepted: row.expected, typescriptAccepted: row.expected })) };
}
const verify = value => verifyOracleReceipt(value, rows(), '2'.repeat(40), digests, '3'.repeat(40));
test('oracle binds exact revision, inputs and all expected verdicts', () => assert.doesNotThrow(() => verify(receipt())));
for (const [name, mutate] of [
  ['stale source', r => { r.sourceRevision = '0'.repeat(40); }],
  ['wrong validator', r => { r.validator.revision = '0'.repeat(40); }],
  ['wrong repository', r => { r.validator.repository = 'fake'; }],
  ['missing digest', r => { delete r.sourceDigests[paths[0]]; }],
  ['changed digest', r => { r.sourceDigests[paths[0]] = '0'.repeat(64); }],
  ['extra digest', r => { r.sourceDigests.extra = '0'.repeat(64); }],
  ['missing fixture', r => { r.results.pop(); }],
  ['wrong result', r => { r.results[1].tjsvAccepted = true; }],
  ['wrong TS result', r => { r.results[1].typescriptAccepted = true; }],
  ['coerced expectation', r => { r.results[1].expected = 0; }],
  ['missing findings', r => { delete r.findings; }],
  ['failed oracle', r => { r.status = 'failed'; }],
  ['nonempty findings', r => { r.findings = [{}]; }],
]) test(`refuses oracle ${name}`, () => { const value = receipt(); mutate(value); assert.throws(() => verify(value)); });
