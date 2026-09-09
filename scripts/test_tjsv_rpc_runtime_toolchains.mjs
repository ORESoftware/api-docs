import test from 'node:test';
import assert from 'node:assert/strict';
import { assessRuntimeToolchains, EXPECTED_TOOLCHAIN_VERSIONS } from './tjsv-rpc-runtime-toolchains.mjs';

const observed = () => ({
  node: 'v22.23.1',
  rust: 'rustc 1.95.0 (deadbeef 2026-08-01)',
  go: 'go version go1.24.7 linux/amd64',
  dart: 'Dart SDK version: 3.6.0 (stable) on "linux_x64"',
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

test('expected version ledger is immutable and complete', () => {
  assert.equal(Object.isFrozen(EXPECTED_TOOLCHAIN_VERSIONS), true);
  assert.deepEqual(Object.keys(EXPECTED_TOOLCHAIN_VERSIONS).sort(), ['dart', 'go', 'node', 'rust']);
  assert.throws(() => { EXPECTED_TOOLCHAIN_VERSIONS.go = '0.0.0'; });
});
