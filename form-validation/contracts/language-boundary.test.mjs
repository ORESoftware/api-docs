import test from 'node:test';
import assert from 'node:assert/strict';
import { BOUNDARY_TARGETS, buildBoundaryEvidence, buildBoundaryManifest } from './language-boundary.mjs';

const boundary = Object.freeze({
  LANGUAGE_BOUNDARY_MANIFEST_SCHEMA: 'ores.typespec-json-schema-validator.language-boundaries/v1',
  LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA: 'ores.typespec-json-schema-validator.language-boundary-evidence/v1',
});
const sourceRevision = '1'.repeat(40);
const parityRunId = '2'.repeat(64);
const contractIrId = '3'.repeat(64);
const observationDigests = Object.freeze({
  'rust-native': '4'.repeat(64),
  'dart-vm': '5'.repeat(64),
  'dart-js': '6'.repeat(64),
});
const identities = Object.freeze({
  'rust-native': Object.freeze({
    toolchain: Object.freeze({ name: 'rustc', version: '1.95.0' }),
    generator: Object.freeze({ name: 'cargo', version: '1.95.0' }),
  }),
  'dart-vm': Object.freeze({
    toolchain: Object.freeze({ name: 'dart-vm', version: '3.6.0' }),
    generator: Object.freeze({ name: 'dart-run', version: '3.6.0' }),
  }),
  'dart-js': Object.freeze({
    toolchain: Object.freeze({ name: 'node', version: '22.23.1' }),
    generator: Object.freeze({ name: 'dart-compile-js', version: '3.6.0' }),
  }),
});
const build = (overrides = {}) => buildBoundaryEvidence({
  boundary,
  sourceRevision,
  parityRunId,
  contractIrId,
  observationDigests,
  identities,
  ...overrides,
});

test('manifest requires all three runtimes across two languages with peer authored authorities', () => {
  const manifest = buildBoundaryManifest(boundary);
  assert.equal(manifest.minimumDistinctLanguages, 2);
  assert.deepEqual(manifest.authorities, {
    typeSpec: 'peer',
    jsonSchema: 'peer',
    generatedWitness: 'evidence_only',
  });
  assert.equal(manifest.targets.length, 3);
  assert.deepEqual(new Set(manifest.targets.map(row => row.language)), new Set(['rust', 'dart']));
  assert.ok(manifest.targets.every(row => row.required && row.ingress && row.egress));
  assert.equal(new Set(manifest.targets.map(row => row.evidence)).size, 3);
});

test('evidence binds exact source, parity, Contract IR, runtime identities and observation digests', () => {
  const evidence = build();
  for (const target of BOUNDARY_TARGETS) {
    const row = evidence[target.evidence];
    assert.equal(row.schema, boundary.LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA);
    assert.equal(row.language, target.language);
    assert.equal(row.runtime, target.runtime);
    assert.equal(row.sourceRevision, sourceRevision);
    assert.equal(row.receiptRunId, parityRunId);
    assert.equal(row.contractIrId, contractIrId);
    assert.equal(row.artifactDigest, `sha256:${observationDigests[target.id]}`);
    assert.deepEqual(row.validation, { ingress: 'passed', egress: 'passed' });
  }
});

test('changing one observation digest changes only that runtime evidence binding', () => {
  const before = build();
  const after = build({ observationDigests: { ...observationDigests, 'dart-vm': '7'.repeat(64) } });
  assert.equal(before['runtime/rust-native.json'].artifactDigest, after['runtime/rust-native.json'].artifactDigest);
  assert.notEqual(before['runtime/dart-vm.json'].artifactDigest, after['runtime/dart-vm.json'].artifactDigest);
  assert.equal(before['runtime/dart-js.json'].artifactDigest, after['runtime/dart-js.json'].artifactDigest);
});

for (const [name, overrides] of [
  ['symbolic revision', { sourceRevision: 'main' }],
  ['short parity id', { parityRunId: '2'.repeat(63) }],
  ['short Contract IR id', { contractIrId: '3'.repeat(63) }],
  ['missing observation digest', { observationDigests: { ...observationDigests, 'dart-js': undefined } }],
  ['missing runtime identity', { identities: { ...identities, 'rust-native': undefined } }],
  ['blank toolchain', { identities: { ...identities, 'dart-vm': { ...identities['dart-vm'], toolchain: { name: '', version: '3.6.0' } } } }],
  ['blank generator version', { identities: { ...identities, 'dart-js': { ...identities['dart-js'], generator: { name: 'dart-compile-js', version: '' } } } }],
]) {
  test(`builder fails closed on ${name}`, () => assert.throws(() => build(overrides)));
}

test('target registry and generated envelopes are immutable', () => {
  assert.throws(() => BOUNDARY_TARGETS.push({}));
  const manifest = buildBoundaryManifest(boundary);
  assert.throws(() => manifest.targets.push({}));
  const evidence = build();
  assert.throws(() => { evidence['runtime/rust-native.json'].status = 'failed'; });
});
