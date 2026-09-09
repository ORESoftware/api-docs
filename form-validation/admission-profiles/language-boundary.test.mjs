import test from 'node:test';
import assert from 'node:assert/strict';
import { BOUNDARY_TARGETS, buildBoundaryEvidence, buildBoundaryManifest } from './language-boundary.mjs';

const boundary = {
  LANGUAGE_BOUNDARY_MANIFEST_SCHEMA: 'ores.typespec-json-schema-validator.language-boundaries/v1',
  LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA: 'ores.typespec-json-schema-validator.language-boundary-evidence/v1',
};
const sourceRevision = '1'.repeat(40);
const parityRunId = '2'.repeat(64);
const contractIrId = '3'.repeat(64);
const outputs = () => ({
  'rust-native': 'rust-output',
  'dart-vm': 'dart-vm-output',
  'dart-javascript': 'dart-js-output',
  'typescript-zod': 'typescript-output',
});
const identities = () => ({
  'rust-native': { toolchain: { name: 'rustc', version: '1.95.0' }, generator: { name: 'cargo-test', version: '1.95.0' } },
  'dart-vm': { toolchain: { name: 'dart', version: '3.6.0' }, generator: { name: 'dart-run', version: '3.6.0' } },
  'dart-javascript': { toolchain: { name: 'node', version: 'v22.23.1' }, generator: { name: 'dart-compile-js', version: '3.6.0' } },
  'typescript-zod': { toolchain: { name: 'node', version: 'v22.23.1' }, generator: { name: 'typescript-zod', version: 'typescript-5.9.3+zod-4.5.4' } },
});

const input = () => ({ boundary, sourceRevision, parityRunId, contractIrId, outputs: outputs(), identities: identities() });

test('manifest requires all four runtimes across three languages', () => {
  const manifest = buildBoundaryManifest(boundary);
  assert.equal(manifest.schema, boundary.LANGUAGE_BOUNDARY_MANIFEST_SCHEMA);
  assert.equal(manifest.minimumDistinctLanguages, 3);
  assert.deepEqual(manifest.authorities, {
    typeSpec: 'peer',
    jsonSchema: 'peer',
    generatedWitness: 'evidence_only',
  });
  assert.equal(manifest.targets.length, 4);
  assert.equal(new Set(manifest.targets.map(target => target.language)).size, 3);
  assert.equal(manifest.targets.every(target => target.required && target.ingress && target.egress), true);
  assert.equal(new Set(manifest.targets.map(target => target.evidence)).size, 4);
});

test('evidence binds exact revision, parity, Contract IR, runtime identity and output digest', () => {
  const evidence = buildBoundaryEvidence(input());
  assert.deepEqual(Object.keys(evidence).sort(), BOUNDARY_TARGETS.map(target => target.evidence).sort());
  for (const target of BOUNDARY_TARGETS) {
    const row = evidence[target.evidence];
    assert.equal(row.schema, boundary.LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA);
    assert.equal(row.language, target.language);
    assert.equal(row.runtime, target.runtime);
    assert.equal(row.status, 'passed');
    assert.equal(row.sourceRevision, sourceRevision);
    assert.equal(row.receiptRunId, parityRunId);
    assert.equal(row.contractIrId, contractIrId);
    assert.match(row.artifactDigest, /^sha256:[a-f0-9]{64}$/u);
    assert.deepEqual(row.validation, { ingress: 'passed', egress: 'passed' });
  }
});

test('different runtime output changes only that evidence artifact digest', () => {
  const first = buildBoundaryEvidence(input());
  const changedOutputs = outputs();
  changedOutputs['dart-vm'] += '-changed';
  const second = buildBoundaryEvidence({ boundary, sourceRevision, parityRunId, contractIrId, outputs: changedOutputs, identities: identities() });
  for (const target of BOUNDARY_TARGETS) {
    const changed = target.evidence === 'runtime/dart-vm.json';
    assert.equal(first[target.evidence].artifactDigest === second[target.evidence].artifactDigest, !changed);
  }
});

for (const [name, mutate] of [
  ['symbolic revision', value => { value.sourceRevision = 'main'; }],
  ['short parity id', value => { value.parityRunId = '2'.repeat(40); }],
  ['short Contract IR id', value => { value.contractIrId = '3'.repeat(40); }],
  ['missing runtime output', value => { delete value.outputs['rust-native']; }],
  ['extra runtime output', value => { value.outputs['python-cpython'] = 'unexpected'; }],
  ['missing runtime identity', value => { delete value.identities['dart-vm']; }],
  ['extra runtime identity', value => { value.identities['python-cpython'] = { toolchain: { name: 'python', version: '3.14.0' }, generator: { name: 'pytest', version: '9.0.0' } }; }],
  ['missing toolchain identity', value => { delete value.identities['dart-javascript'].toolchain; }],
  ['missing generator identity', value => { delete value.identities['typescript-zod'].generator; }],
  ['extra identity field', value => { value.identities['rust-native'].receipt = 'stale'; }],
  ['extra toolchain field', value => { value.identities['rust-native'].toolchain.command = 'rustc --version'; }],
  ['blank toolchain name', value => { value.identities['rust-native'].toolchain.name = ''; }],
  ['whitespace toolchain version', value => { value.identities['rust-native'].toolchain.version = ' 1.95.0 '; }],
  ['control character in generator name', value => { value.identities['dart-vm'].generator.name = 'dart\u0000run'; }],
  ['oversized generator version', value => { value.identities['typescript-zod'].generator.version = 'x'.repeat(257); }],
]) test(`builder fails closed on ${name}`, () => {
  const value = input();
  mutate(value);
  assert.throws(() => buildBoundaryEvidence(value));
});

test('builder refuses prototype-inherited runtime output', () => {
  const value = input();
  const inherited = { 'rust-native': value.outputs['rust-native'] };
  value.outputs = Object.assign(Object.create(inherited), value.outputs);
  delete value.outputs['rust-native'];
  assert.equal(value.outputs['rust-native'], 'rust-output');
  assert.throws(() => buildBoundaryEvidence(value));
});

test('builder refuses prototype-inherited runtime identity', () => {
  const value = input();
  const inherited = { 'dart-vm': value.identities['dart-vm'] };
  value.identities = Object.assign(Object.create(inherited), value.identities);
  delete value.identities['dart-vm'];
  assert.equal(value.identities['dart-vm'].toolchain.name, 'dart');
  assert.throws(() => buildBoundaryEvidence(value));
});

test('builder refuses inherited toolchain token fields', () => {
  const value = input();
  value.identities['rust-native'].toolchain = Object.create({ name: 'rustc', version: '1.95.0' });
  assert.throws(() => buildBoundaryEvidence(value));
});

test('target registry is immutable', () => {
  assert.equal(Object.isFrozen(BOUNDARY_TARGETS), true);
  assert.equal(BOUNDARY_TARGETS.every(Object.isFrozen), true);
  assert.throws(() => BOUNDARY_TARGETS.push({}));
});
