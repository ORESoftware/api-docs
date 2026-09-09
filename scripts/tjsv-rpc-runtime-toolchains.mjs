import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { readSafeBytes, writeOwnedJson } from './projection-evidence-io.mjs';
import { PROFILE, TJSV_REVISION } from './tjsv-rpc-admission.mjs';

const ROOT = fileURLToPath(new URL('../', import.meta.url));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const requireThat = (condition, message) => { if (!condition) throw new Error(message); };
const TOOLCHAIN_KEYS = Object.freeze(['dart', 'go', 'node', 'rust']);
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const REVISION_PATTERN = /^[0-9a-f]{40}$/;

export const TOOLCHAIN_RECEIPT_SCHEMA = 'ores.api-docs.tjsv-runtime-toolchains/v1';

export const EXPECTED_TOOLCHAIN_VERSIONS = Object.freeze({
  node: '22.23.1',
  rust: '1.95.0',
  go: '1.24.7',
  dart: '3.6.0',
});

export const TOOLCHAIN_SOURCE_INPUTS = Object.freeze([
  '.github/workflows/tjsv-rpc-cross-runtime.yml',
  'examples/rpc-v1/conformance.json',
  'scripts/projection-evidence-io.mjs',
  'scripts/tjsv-rpc-admission.mjs',
  'scripts/tjsv-rpc-cross-runtime.mjs',
  'scripts/tjsv-rpc-runtime-protocol.mjs',
  'scripts/tjsv-rpc-runtime-toolchains.mjs',
  'scripts/test_tjsv_rpc_runtime_toolchains.mjs',
].sort());

const VERSION_PATTERNS = Object.freeze({
  node: /^v(\d+\.\d+\.\d+)$/,
  rust: /^rustc (\d+\.\d+\.\d+)(?:\s|$)/,
  go: /^go version go(\d+\.\d+\.\d+)(?:\s|$)/,
  dart: /^Dart SDK version: (\d+\.\d+\.\d+)(?:\s|$)/,
});

function exactKeys(value, keys) {
  return value !== null
    && typeof value === 'object'
    && !Array.isArray(value)
    && isDeepStrictEqual(Object.keys(value).sort(), [...keys].sort());
}

function git(...args) {
  const env = { ...process.env, GIT_NO_REPLACE_OBJECTS: '1' };
  for (const key of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES']) delete env[key];
  return execFileSync('git', ['-C', ROOT, ...args], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], env, timeout: 30_000, maxBuffer: 8 * 1024 * 1024,
  }).trim();
}

function extractVersion(runtime, output) {
  const match = VERSION_PATTERNS[runtime].exec(output);
  return match?.[1] ?? null;
}

export function assessRuntimeToolchains(observed) {
  requireThat(observed !== null && typeof observed === 'object' && !Array.isArray(observed), 'toolchain evidence must be an object');
  requireThat(isDeepStrictEqual(Object.keys(observed).sort(), TOOLCHAIN_KEYS), 'toolchain evidence must contain exactly node, rust, go and dart');
  const findings = [];
  const versions = {};
  for (const runtime of TOOLCHAIN_KEYS) {
    const output = observed[runtime];
    requireThat(typeof output === 'string' && output.length > 0 && output.length <= 512, `invalid ${runtime} toolchain output`);
    requireThat(!/[\r\n]/.test(output), `${runtime} toolchain output must be one line`);
    const version = extractVersion(runtime, output);
    versions[runtime] = version;
    if (version !== EXPECTED_TOOLCHAIN_VERSIONS[runtime]) {
      findings.push({
        runtime,
        rule: version === null ? 'unrecognized-toolchain-output' : 'toolchain-version-drift',
        expectedVersion: EXPECTED_TOOLCHAIN_VERSIONS[runtime],
        observedVersion: version,
      });
    }
  }
  return {
    status: findings.length === 0 ? 'passed' : 'stopped_for_evaluation',
    versions,
    findings,
  };
}

export function verifyRuntimeToolchainReceipt(receipt, { sourceRevision, validatorRevision, sourceDigests }) {
  requireThat(exactKeys(receipt, [
    'schema', 'profile', 'status', 'sourceRevision', 'validator', 'sourceDigests',
    'expectedVersions', 'observedToolchains', 'observedVersions', 'findings', 'limits',
  ]), 'malformed runtime toolchain receipt envelope');
  requireThat(receipt.schema === TOOLCHAIN_RECEIPT_SCHEMA, 'unsupported runtime toolchain receipt schema');
  requireThat(receipt.profile === PROFILE, 'runtime toolchain receipt profile drift');
  requireThat(receipt.status === 'passed' && Array.isArray(receipt.findings) && receipt.findings.length === 0, 'runtime toolchain receipt did not pass');
  requireThat(REVISION_PATTERN.test(sourceRevision) && receipt.sourceRevision === sourceRevision, 'runtime toolchain receipt source revision drift');
  requireThat(REVISION_PATTERN.test(validatorRevision), 'invalid expected TJSV revision');
  requireThat(exactKeys(receipt.validator, ['repository', 'revision']), 'malformed runtime toolchain validator identity');
  requireThat(receipt.validator.repository === 'ORESoftware/typespec-json-schema-validator', 'runtime toolchain validator repository drift');
  requireThat(receipt.validator.revision === validatorRevision, 'runtime toolchain validator revision drift');

  requireThat(exactKeys(sourceDigests, TOOLCHAIN_SOURCE_INPUTS), 'current runtime toolchain source closure is incomplete');
  requireThat(exactKeys(receipt.sourceDigests, TOOLCHAIN_SOURCE_INPUTS), 'runtime toolchain receipt source closure drift');
  for (const path of TOOLCHAIN_SOURCE_INPUTS) {
    requireThat(typeof sourceDigests[path] === 'string' && SHA256_PATTERN.test(sourceDigests[path]), `invalid current runtime toolchain digest: ${path}`);
    requireThat(receipt.sourceDigests[path] === sourceDigests[path], `runtime toolchain source changed: ${path}`);
  }

  requireThat(exactKeys(receipt.expectedVersions, TOOLCHAIN_KEYS), 'runtime toolchain expected-version ledger drift');
  requireThat(isDeepStrictEqual(receipt.expectedVersions, EXPECTED_TOOLCHAIN_VERSIONS), 'runtime toolchain expected versions changed');
  requireThat(exactKeys(receipt.observedVersions, TOOLCHAIN_KEYS), 'runtime toolchain observed-version ledger drift');
  requireThat(isDeepStrictEqual(receipt.observedVersions, EXPECTED_TOOLCHAIN_VERSIONS), 'runtime toolchain observed versions do not match reviewed pins');
  const assessment = assessRuntimeToolchains(receipt.observedToolchains);
  requireThat(assessment.status === 'passed' && assessment.findings.length === 0, 'runtime toolchain raw evidence does not match reviewed pins');
  requireThat(isDeepStrictEqual(assessment.versions, receipt.observedVersions), 'runtime toolchain parsed evidence drift');

  requireThat(exactKeys(receipt.limits, ['purpose', 'reproducibleBuildAttestation', 'universalEquivalenceProven']), 'runtime toolchain receipt limits drift');
  requireThat(receipt.limits.purpose === 'bind actual runtime compiler/interpreter identity to the TJSV cross-runtime evidence run', 'runtime toolchain receipt purpose drift');
  requireThat(receipt.limits.reproducibleBuildAttestation === false, 'runtime toolchain receipt overclaims reproducible build attestation');
  requireThat(receipt.limits.universalEquivalenceProven === false, 'runtime toolchain receipt overclaims universal equivalence');

  return Object.freeze({
    versions: Object.freeze({ ...receipt.observedVersions }),
  });
}

function readCommandVersion(command, args) {
  const execution = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 30_000,
    maxBuffer: 64 * 1024,
  });
  requireThat(!execution.error && execution.signal === null && execution.status === 0, `${command} version command failed`);
  const output = [execution.stdout, execution.stderr].filter(Boolean).join('\n').trim();
  requireThat(output.length > 0 && !/[\r\n]/.test(output), `${command} version command returned malformed output`);
  return output;
}

export function captureRuntimeToolchains() {
  return {
    node: process.version,
    rust: readCommandVersion('rustc', ['--version']),
    go: readCommandVersion('go', ['version']),
    dart: readCommandVersion('dart', ['--version']),
  };
}

export async function main() {
  requireThat(process.argv.length === 2, 'fixed toolchain evidence entrypoint accepts no arguments');
  const sourceRevision = git('rev-parse', 'HEAD');
  const snapshots = Object.fromEntries(await Promise.all(TOOLCHAIN_SOURCE_INPUTS.map(async path => [path, await readSafeBytes(ROOT, path)])));
  const sourceDigests = Object.fromEntries(TOOLCHAIN_SOURCE_INPUTS.map(path => [path, sha256(snapshots[path])]));
  const observedToolchains = captureRuntimeToolchains();
  const assessment = assessRuntimeToolchains(observedToolchains);

  requireThat(git('rev-parse', 'HEAD') === sourceRevision, 'source revision changed during toolchain admission');
  for (const path of TOOLCHAIN_SOURCE_INPUTS) {
    requireThat(sha256(await readSafeBytes(ROOT, path)) === sourceDigests[path], `source changed during toolchain admission: ${path}`);
  }

  const report = {
    schema: TOOLCHAIN_RECEIPT_SCHEMA,
    profile: PROFILE,
    status: assessment.status,
    sourceRevision,
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: TJSV_REVISION },
    sourceDigests,
    expectedVersions: EXPECTED_TOOLCHAIN_VERSIONS,
    observedToolchains,
    observedVersions: assessment.versions,
    findings: assessment.findings,
    limits: {
      purpose: 'bind actual runtime compiler/interpreter identity to the TJSV cross-runtime evidence run',
      reproducibleBuildAttestation: false,
      universalEquivalenceProven: false,
    },
  };
  if (report.status === 'passed') {
    verifyRuntimeToolchainReceipt(report, { sourceRevision, validatorRevision: TJSV_REVISION, sourceDigests });
  }
  await writeOwnedJson(ROOT, 'tmp/tjsv-runtime-toolchains.json', `${JSON.stringify(report, null, 2)}\n`, new Set());
  console.log(JSON.stringify({ status: report.status, observedVersions: report.observedVersions, findings: report.findings }));
  return report.status === 'passed' ? 0 : 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().then(code => { process.exitCode = code; }).catch(error => {
    console.error(JSON.stringify({ status: 'failed', error: error instanceof Error ? error.message : String(error) }));
    process.exitCode = 3;
  });
}
