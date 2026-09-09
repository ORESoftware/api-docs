import test from 'node:test';
import assert from 'node:assert/strict';
import {
  assessRuntimeToolchains,
  EXPECTED_TOOLCHAIN_VERSIONS,
  TOOLCHAIN_RECEIPT_SCHEMA,
  TOOLCHAIN_SOURCE_INPUTS,
  verifyRuntimeToolchainReceipt,
} from './tjsv-rpc-runtime-toolchains.mjs';

const observed = () => ({
  node: 'v22.23.1',
  rust: 'rustc 1.95.0 (deadbeef 2026-08-01)',
  go: 'go version go1.24.7 linux/amd64',
  dart: 'Dart SDK version: 3.6.0 (stable) on "linux_x64"',
});

const sourceRevision = '2'.repeat(40);
const validatorRevision = '3'.repeat(40);
const sourceDigests = Object.fromEntries(TOOLCHAIN_SOURCE_INPUTS.map(path => [path, 'a'.repeat(64)]));

function receipt() {
  return {
    schema: TOOLCHAIN_RECEIPT_SCHEMA,
    profile: 'ores-rpc-v1-call-receipt',
    status: 'passed',
    sourceRevision,
    validator: {
      repository: 'ORESoftware/typespec-json-schema-validator',
      revision: validatorRevision,
    },
    sourceDigests: { ...sourceDigests },
    expectedVersions: { ...EXPECTED_TOOLCHAIN_VERSIONS },
    observedToolchains: observed(),
    observedVersions: { ...EXPECTED_TOOLCHAIN_VERSIONS },
    findings: [],
    limits: {
      purpose: 'bind actual runtime compiler/interpreter identity to the TJSV cross-runtime evidence run',
      reproducibleBuildAttestation: false,
      universalEquivalenceProven: false,
    },
  };
}

const verifyReceipt = value => verifyRuntimeToolchainReceipt(value, {
  sourceRevision,
  validatorRevision,
  sourceDigests,
});

test('exact reviewed runtime versions pass', () => {
  const result = assessRuntimeToolchains(observed());
  assert.equal(result.status, 'passed');
  assert.deepEqual(result.findings, []);
  assert.deepEqual(result.versions, EXPECTED_TOOLCHAIN_VERSIONS);
});

for (const runtime of ['node', 'rust', 'go', 'dart']) {
  test(`version drift stops evaluation for ${runtime}`, () => {
    const value = observed();
    value[runtime] = runtime === 'node'
      ? 'v22.23.2'
      : runtime === 'rust'
        ? 'rustc 1.95.1 (deadbeef 2026-08-01)'
        : runtime === 'go'
          ? 'go version go1.24.8 linux/amd64'
          : 'Dart SDK version: 3.6.1 (stable) on "linux_x64"';
    const result = assessRuntimeToolchains(value);
    assert.equal(result.status, 'stopped_for_evaluation');
    assert.equal(result.findings.length, 1);
    assert.equal(result.findings[0].runtime, runtime);
    assert.equal(result.findings[0].rule, 'toolchain-version-drift');
  });
}

test('unrecognized but bounded output stops evaluation', () => {
  const value = observed();
  value.go = 'custom-go-wrapper';
  const result = assessRuntimeToolchains(value);
  assert.equal(result.status, 'stopped_for_evaluation');
  assert.deepEqual(result.findings, [{
    runtime: 'go',
    rule: 'unrecognized-toolchain-output',
    expectedVersion: '1.24.7',
    observedVersion: null,
  }]);
});

for (const [name, mutate] of [
  ['missing runtime', value => { delete value.dart; }],
  ['extra runtime', value => { value.python = '3.12'; }],
  ['non-string output', value => { value.rust = 1950; }],
  ['empty output', value => { value.node = ''; }],
  ['oversized output', value => { value.go = 'x'.repeat(513); }],
  ['multiline output', value => { value.dart += '\nsecret=unexpected'; }],
]) {
  test(`malformed evidence fails closed: ${name}`, () => {
    const value = observed();
    mutate(value);
    assert.throws(() => assessRuntimeToolchains(value));
  });
}

test('a closed, current, passing toolchain receipt verifies', () => {
  const result = verifyReceipt(receipt());
  assert.deepEqual(result.versions, EXPECTED_TOOLCHAIN_VERSIONS);
  assert.equal(Object.isFrozen(result), true);
  assert.equal(Object.isFrozen(result.versions), true);
});

for (const [name, mutate] of [
  ['wrong schema', value => { value.schema = 'other/v1'; }],
  ['extra envelope field', value => { value.accepted = true; }],
  ['wrong profile', value => { value.profile = 'other'; }],
  ['stale source revision', value => { value.sourceRevision = '0'.repeat(40); }],
  ['wrong validator repository', value => { value.validator.repository = 'other/repo'; }],
  ['wrong validator revision', value => { value.validator.revision = '0'.repeat(40); }],
  ['missing source digest', value => { delete value.sourceDigests[TOOLCHAIN_SOURCE_INPUTS[0]]; }],
  ['extra source digest', value => { value.sourceDigests.extra = 'a'.repeat(64); }],
  ['stale source digest', value => { value.sourceDigests[TOOLCHAIN_SOURCE_INPUTS[0]] = 'b'.repeat(64); }],
  ['changed expected version', value => { value.expectedVersions.go = '1.24.8'; }],
  ['changed observed version', value => { value.observedVersions.go = '1.24.8'; }],
  ['raw toolchain drift', value => { value.observedToolchains.go = 'go version go1.24.8 linux/amd64'; }],
  ['nonempty findings', value => { value.findings = [{ runtime: 'go' }]; }],
  ['stopped status', value => { value.status = 'stopped_for_evaluation'; }],
  ['changed purpose', value => { value.limits.purpose = 'other'; }],
  ['reproducible-build overclaim', value => { value.limits.reproducibleBuildAttestation = true; }],
  ['universal-equivalence overclaim', value => { value.limits.universalEquivalenceProven = true; }],
]) {
  test(`toolchain receipt fails closed on ${name}`, () => {
    const value = receipt();
    mutate(value);
    assert.throws(() => verifyReceipt(value));
  });
}

test('toolchain receipt rejects an incomplete current source snapshot', () => {
  const current = { ...sourceDigests };
  delete current[TOOLCHAIN_SOURCE_INPUTS[0]];
  assert.throws(() => verifyRuntimeToolchainReceipt(receipt(), {
    sourceRevision,
    validatorRevision,
    sourceDigests: current,
  }));
});

test('expected version ledger and source closure are immutable and complete', () => {
  assert.equal(Object.isFrozen(EXPECTED_TOOLCHAIN_VERSIONS), true);
  assert.equal(Object.isFrozen(TOOLCHAIN_SOURCE_INPUTS), true);
  assert.deepEqual(Object.keys(EXPECTED_TOOLCHAIN_VERSIONS).sort(), ['dart', 'go', 'node', 'rust']);
  assert.deepEqual(TOOLCHAIN_SOURCE_INPUTS, [...TOOLCHAIN_SOURCE_INPUTS].sort());
  assert.equal(new Set(TOOLCHAIN_SOURCE_INPUTS).size, TOOLCHAIN_SOURCE_INPUTS.length);
  for (const path of [
    '.github/workflows/tjsv-rpc-cross-runtime.yml',
    'examples/rpc-v1/conformance.json',
    'scripts/projection-evidence-io.mjs',
    'scripts/tjsv-rpc-admission.mjs',
    'scripts/tjsv-rpc-cross-runtime.mjs',
    'scripts/tjsv-rpc-runtime-protocol.mjs',
    'scripts/tjsv-rpc-runtime-toolchains.mjs',
    'scripts/test_tjsv_rpc_runtime_toolchains.mjs',
  ]) assert.ok(TOOLCHAIN_SOURCE_INPUTS.includes(path), `missing toolchain source ${path}`);
  assert.throws(() => { EXPECTED_TOOLCHAIN_VERSIONS.go = '0.0.0'; });
  assert.throws(() => TOOLCHAIN_SOURCE_INPUTS.push('unreviewed-source'));
});
