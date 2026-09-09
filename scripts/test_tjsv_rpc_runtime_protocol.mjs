import test from 'node:test';
import assert from 'node:assert/strict';
import { makeProbeRequest, assessProbeResponse, readProbeExecution, verifyOracleReceipt, REQUEST_SCHEMA, RESPONSE_SCHEMA, MAX_PROTOCOL_BYTES, ORACLE_INPUTS } from './tjsv-rpc-runtime-protocol.mjs';
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
const paths = ['examples/rpc-v1/conformance.json', 'json-schema/rpc-call.schema.json', 'json-schema/rpc-receipt.schema.json', 'idl/typespec/v1.tsp', 'runtime/v1-conformance.json', 'clients/typescript/src/rpc.js', 'scripts/tjsv-rpc-admission.mjs', 'scripts/tjsv-source-integrity.mjs', 'scripts/projection-evidence-io.mjs'];
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


test('oracle refuses empty or untyped expectation sets', () => {
  const empty = receipt(); empty.results = [];
  assert.throws(() => verifyOracleReceipt(empty, [], '2'.repeat(40), digests, '3'.repeat(40)));
  assert.throws(() => verifyOracleReceipt(receipt(), rows().map(row => ({ ...row, expected: undefined })), '2'.repeat(40), digests, '3'.repeat(40)));
});

test('real child process does not inherit credential-like environment', async () => {
  const { mkdtemp, writeFile, rm } = await import('node:fs/promises');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const { invokeProbe } = await import('./tjsv-rpc-runtime-protocol.mjs');
  const directory = await mkdtemp(join(tmpdir(), 'tjsv-process-test-'));
  const executable = join(directory, 'probe');
  const original = process.env.TJSV_TEST_CANARY;
  try {
    await writeFile(executable, `#!${process.execPath}\nprocess.stdin.resume(); process.stdin.on('end', () => console.log(JSON.stringify(process.env)));\n`, { mode: 0o700 });
    process.env.TJSV_TEST_CANARY = 'must-not-propagate';
    const environment = readProbeExecution(invokeProbe(executable, '{}'));
    assert.equal(environment.TJSV_TEST_CANARY, undefined);
    assert.deepEqual(Object.keys(environment).sort(), ['LANG', 'LC_ALL']);
    assert.throws(() => invokeProbe(executable, null));
    assert.throws(() => invokeProbe(executable, 'x'.repeat(MAX_PROTOCOL_BYTES + 1)));
    await writeFile(executable, `#!${process.execPath}\nprocess.stdin.resume(); process.stdin.on('end', () => { console.log('{}'); process.exitCode = 3; });\n`);
    assert.throws(() => readProbeExecution(invokeProbe(executable, '{}')), /execution failed/);
    assert.throws(() => readProbeExecution(invokeProbe(join(directory, 'missing'), '{}')), /execution failed/);
  } finally {
    if (original === undefined) delete process.env.TJSV_TEST_CANARY;
    else process.env.TJSV_TEST_CANARY = original;
    await rm(directory, { recursive: true, force: true });
  }
});


test('oracle source manifest is closed, immutable and includes both integrity helpers', () => {
  assert.deepEqual(ORACLE_INPUTS, paths);
  assert.equal(Object.isFrozen(ORACLE_INPUTS), true);
  assert.throws(() => ORACLE_INPUTS.push('unreviewed-input'));
});
for (const path of ['scripts/tjsv-source-integrity.mjs', 'scripts/projection-evidence-io.mjs']) {
  test(`refuses missing oracle helper digest: ${path}`, () => {
    const value = receipt();
    delete value.sourceDigests[path];
    assert.throws(() => verify(value), /oracle digest/);
  });
  test(`refuses stale oracle helper digest: ${path}`, () => {
    const value = receipt();
    value.sourceDigests[path] = '0'.repeat(64);
    assert.throws(() => verify(value), /oracle input changed/);
  });
  test(`refuses an unsnapshotted current helper: ${path}`, () => {
    const current = { ...digests };
    delete current[path];
    assert.throws(() => verifyOracleReceipt(receipt(), rows(), '2'.repeat(40), current, '3'.repeat(40)), /oracle input changed/);
  });
}
test('legacy seven-input receipts cannot certify the hardened oracle', () => {
  const value = receipt();
  delete value.sourceDigests['scripts/tjsv-source-integrity.mjs'];
  delete value.sourceDigests['scripts/projection-evidence-io.mjs'];
  assert.throws(() => verify(value), /oracle digest/);
});
