import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';
import * as runtime from '../clients/typescript/src/rpc.js';
import { readSafeJson } from './projection-evidence-io.mjs';
import { verifyValidatorSource } from './tjsv-source-integrity.mjs';

// This is a test entry point, not another CLI or runtime schema authority.
// Missing tooling is a failure, never a skipped or mock-backed conformance run.
const root = fileURLToPath(new URL('../', import.meta.url));
assert.ok(process.env.TSJSV_VALIDATOR_ROOT, 'TSJSV_VALIDATOR_ROOT is required');
const validatorRoot = resolve(process.env.TSJSV_VALIDATOR_ROOT);
const policy = await readSafeJson(root, 'idl/rpc-v1-projection-admission.policy.json');
await verifyValidatorSource(validatorRoot, policy.validator.revision);
const tjsv = await import(pathToFileURL(resolve(validatorRoot, 'src/index.mjs')).href);
const corpus = await readSafeJson(root, 'examples/rpc-v1/conformance.json');
const schemas = Object.fromEntries(await Promise.all(['call', 'receipt'].map(async (kind) => [
  kind, await readSafeJson(root, `json-schema/rpc-${kind}.schema.json`),
])));
const operations = {
  call: { validate: runtime.validateCall, decode: runtime.decodeCall, encode: runtime.encodeCall, ndjson: runtime.callFromNdjson },
  receipt: { validate: runtime.validateReceipt, decode: runtime.decodeReceipt, encode: runtime.encodeReceipt, ndjson: runtime.receiptFromNdjson },
};

function schemaAccepts(schema, instance) {
  const resolver = new tjsv.SchemaResolver();
  const { base } = resolver.addDocument(schema, 'rpc-schema.json');
  // Do not catch refusal/unresolved-reference errors and call them rejections.
  // A validator that could not evaluate a contract has supplied no verdict.
  return tjsv.validateInstance({ schema, instance, resolver, base, maxErrors: 20 }).valid;
}

function runtimeAccepts(operation) {
  try { operation(); return true; }
  catch (error) {
    // A programming error or missing dependency is not a contract rejection.
    if (!(error instanceof runtime.RpcV1Error)) throw error;
    return false;
  }
}

function assertDecision(kind, instance, expected, encoded = JSON.stringify(instance)) {
  assert.ok(Object.hasOwn(operations, kind), 'unknown fixture kind');
  const api = operations[kind];
  assert.equal(schemaAccepts(schemas[kind], instance), expected, 'TJSV disagrees with the expected decision');
  assert.equal(runtimeAccepts(() => api.validate(structuredClone(instance))), expected, 'runtime object validation disagrees');
  assert.equal(runtimeAccepts(() => api.decode(encoded)), expected, 'runtime JSON ingress disagrees');
  assert.equal(runtimeAccepts(() => api.decode(new TextEncoder().encode(encoded))), expected, 'runtime byte ingress disagrees');
  assert.equal(runtimeAccepts(() => api.ndjson(`${encoded}\n`)), expected, 'runtime NDJSON ingress disagrees');
  if (expected) {
    // Constructor defaults are deliberately not tested as wire admission:
    // encodeCall may supply v/op; a decoder must reject absent v/op.
    const canonical = api.encode(structuredClone(instance));
    assert.equal(schemaAccepts(schemas[kind], canonical), true, 'runtime egress violates its schema');
    const framed = runtime.encodeLengthPrefixed(canonical);
    const { frames, rest } = runtime.splitLengthPrefixed(framed);
    assert.equal(frames.length, 1);
    assert.equal(rest.length, 0);
    assert.deepEqual(api.decode(frames[0]), canonical, 'length-prefixed round trip changed the value');
  }
}

test('corpus remains closed, nonempty and covers both frame kinds and verdicts', () => {
  assert.equal(corpus.schemaVersion, 1);
  assert.equal(corpus.profile, 'ores-rpc-v1-call-receipt');
  assert.equal(corpus.maxFrameBytes, runtime.MAX_FRAME_BYTES);
  assert.equal(corpus.tcpLengthPrefixBytes, runtime.LENGTH_PREFIX_BYTES);
  const names = new Set();
  for (const lane of ['valid', 'invalid']) {
    assert.ok(Array.isArray(corpus[lane]) && corpus[lane].length > 0);
    assert.deepEqual([...new Set(corpus[lane].map((item) => item.kind))].sort(), ['call', 'receipt']);
    for (const item of corpus[lane]) {
      assert.equal(typeof item.name, 'string');
      assert.ok(item.name.length > 0 && !names.has(item.name), 'fixture names must be unique');
      names.add(item.name);
      assert.equal(typeof item.encoded, 'string');
    }
  }
});

for (const kind of ['call', 'receipt']) {
  test(`TJSV checks the authored ${kind} schema before executing instances`, () => {
    assert.equal(schemas[kind].$schema, tjsv.JSON_SCHEMA_DRAFT_2020_12);
    assert.deepEqual(tjsv.validateJsonSchemaDocument(schemas[kind], `rpc-${kind}.schema.json`), []);
  });
}
for (const [lane, expected] of [['valid', true], ['invalid', false]]) {
  assert.ok(Array.isArray(corpus[lane]) && corpus[lane].length > 0, `${lane} corpus is missing`);
  for (const item of corpus[lane]) {
    test(`shared four-language corpus: ${lane}/${item.name}`, () => {
      assertDecision(item.kind, JSON.parse(item.encoded), expected, item.encoded);
    });
  }
}

const call = { v: 1, op: 'call', id: 'probe', key: 'healthz' };
const receipt = { v: 1, op: 'receipt', id: 'probe', key: 'healthz', ok: true };
for (const [kind, seed] of [['call', call], ['receipt', receipt]]) {
  for (const name of Object.keys(seed)) {
    test(`${kind} ingress rejects missing required ${name}`, () => {
      const value = structuredClone(seed);
      delete value[name];
      assertDecision(kind, value, false);
    });
  }
  for (const [name, limit] of [['id', 128], ['traceId', 64], ['spanId', 32]]) {
    for (const [label, value, expected] of [
      ['empty', '', false], ['null', null, false], ['wrong-type', 42, false],
      ['unicode-limit', '😀'.repeat(limit), true], ['unicode-overflow', '😀'.repeat(limit + 1), false],
    ]) {
      test(`${kind} ${name} ${label}`, () => assertDecision(kind, { ...seed, [name]: value }, expected));
    }
  }
  for (const [value, expected] of [['http', true], ['tcp', true], ['websocket', true], ['nats', true], ['udp', false], ['', false], [null, false], [42, false]]) {
    test(`${kind} transport ${JSON.stringify(value)}`, () => assertDecision(kind, { ...seed, transport: value }, expected));
  }
  for (const [label, value] of [['null', null], ['array', []], ['string', 'frame'], ['boolean', true]]) {
    test(`${kind} rejects non-object ${label}`, () => assertDecision(kind, value, false));
  }
}
for (const ok of [true, false]) {
  for (const status of [99, 100, 199, 200, 399, 400, 599, 600, 200.5, '200', null, true]) {
    const expected = Number.isInteger(status) && (ok ? status >= 200 && status <= 399 : status >= 400 && status <= 599);
    test(`receipt ok=${ok} status=${JSON.stringify(status)}`, () => {
      assertDecision('receipt', { ...receipt, ok, status, ...(ok ? {} : { error: {} }) }, expected);
    });
  }
}
for (const [surface, value, expected] of [
  ['path', {}, true], ['path', [], false], ['query', {}, true], ['query', null, false],
  ['headers', {}, true], ['headers', [], false], ['headers', { 'x-id': ['a', 'b'] }, true],
  ['headers', { 'X-Id': 'a' }, false], ['headers', { 'bad name': 'a' }, false],
  ['headers', { ['a'.repeat(128)]: 'a' }, true], ['headers', { ['a'.repeat(129)]: 'a' }, false],
]) {
  test(`call request surface ${surface}: ${JSON.stringify(value)}`, () => assertDecision('call', { ...call, [surface]: value }, expected));
}

test('loosening the authored schema exposes runtime disagreement', () => {
  const loosened = { ...schemas.call, additionalProperties: true };
  const value = { ...call, undeclared: true };
  assert.equal(schemaAccepts(loosened, value), true);
  assert.equal(runtimeAccepts(() => runtime.decodeCall(JSON.stringify(value))), false);
});
test('tightening the authored schema exposes runtime disagreement', () => {
  const tightened = { ...schemas.call, required: [...schemas.call.required, 'body'] };
  assert.equal(schemaAccepts(tightened, call), false);
  assert.equal(runtimeAccepts(() => runtime.decodeCall(JSON.stringify(call))), true);
});
test('unsupported schema semantics are refusal, not an accepted negative case', () => {
  assert.throws(() => schemaAccepts({ ...schemas.call, $dynamicRef: '#missing' }, call), tjsv.UnsupportedKeywordError);
});
test('an unresolved schema reference is not an accepted negative case', () => {
  assert.throws(() => schemaAccepts({ ...schemas.call, $ref: '#/$defs/missing' }, call), tjsv.SchemaResolutionError);
});
