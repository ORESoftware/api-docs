import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { lstat, mkdir, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { readSafeBytes } from './projection-evidence-io.mjs';
import {
  assessProbeResponse,
  invokeProbe,
  makeProbeRequest,
  readProbeExecution,
  requireThat,
} from './tjsv-rpc-runtime-protocol.mjs';

export const GO_INPUTS = Object.freeze([
  '.github/workflows/tjsv-rpc-admission.yml',
  'clients/go/decode.go',
  'clients/go/decode_null_test.go',
  'clients/go/encode.go',
  'clients/go/framing.go',
  'clients/go/go.mod',
  'clients/go/rpc_test.go',
  'clients/go/testdata/tjsv_probe/main.go',
  'clients/go/types.go',
  'clients/go/validate.go',
  'scripts/test_tjsv_go_admission.mjs',
  'scripts/test_tjsv_rpc_runtime_protocol.mjs',
  'scripts/tjsv-go-admission.mjs',
  'scripts/tjsv-rpc-runtime-protocol.mjs',
]);

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

function git(root, ...args) {
  const env = { ...process.env, GIT_NO_REPLACE_OBJECTS: '1' };
  for (const key of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES']) delete env[key];
  return execFileSync('git', ['-C', root, ...args], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    env,
    timeout: 30000,
    maxBuffer: 8 * 1024 * 1024,
  });
}

async function sourceSnapshot(root, revision) {
  requireThat(/^[a-f0-9]{40}$/u.test(revision), 'invalid candidate revision');
  const tree = git(root, 'ls-tree', '-rz', '--full-tree', revision, '--', ...GO_INPUTS);
  const expected = new Set(GO_INPUTS);
  const seen = new Set();
  const digests = {};
  for (const entry of tree.split('\0').filter(Boolean)) {
    const match = /^(100644|100755) blob ([a-f0-9]{40})\t(.+)$/u.exec(entry);
    requireThat(match !== null, 'Go admission source must be a regular tracked file');
    const [, , blobSha, path] = match;
    requireThat(expected.has(path) && !seen.has(path), 'unexpected or duplicate Go admission source');
    seen.add(path);
    const bytes = await readSafeBytes(root, path);
    const actualBlob = createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
    requireThat(actualBlob === blobSha, `Go admission source differs from candidate commit: ${path}`);
    digests[path] = sha256(bytes);
  }
  requireThat(seen.size === GO_INPUTS.length && GO_INPUTS.every(path => seen.has(path)), 'missing Go admission source');
  return Object.fromEntries(GO_INPUTS.map(path => [path, digests[path]]));
}

async function executableDigest(path) {
  const stat = await lstat(path);
  requireThat(stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1, 'Go probe must be a singly linked regular file');
  requireThat(stat.size > 0 && stat.size <= 32 * 1024 * 1024, 'Go probe executable size is invalid');
  return sha256(await readFile(path));
}

function buildEnvironment(root) {
  const base = resolve(root, 'tmp/go-admission');
  return {
    PATH: process.env.PATH ?? '',
    HOME: resolve(base, 'home'),
    TMPDIR: resolve(base, 'tmp'),
    GOCACHE: resolve(base, 'build-cache'),
    GOMODCACHE: resolve(base, 'mod-cache'),
    GOENV: 'off',
    GOWORK: 'off',
    GOFLAGS: '',
    GOTOOLCHAIN: 'local',
    GOPROXY: 'off',
    GOSUMDB: 'off',
    CGO_ENABLED: '0',
    LANG: 'C.UTF-8',
    LC_ALL: 'C.UTF-8',
  };
}

/** Build and execute the already-reviewed Go probe against current candidate sources. */
export async function runGoAdmission(root, rows) {
  const request = makeProbeRequest(rows);
  const revision = git(root, 'rev-parse', 'HEAD').trim();
  const before = await sourceSnapshot(root, revision);
  const env = buildEnvironment(root);
  for (const path of [env.HOME, env.TMPDIR, env.GOCACHE, env.GOMODCACHE, resolve(root, 'tmp/tjsv-probes')]) {
    await mkdir(path, { recursive: true });
  }
  const executable = resolve(root, 'tmp/tjsv-probes/go-base-admission');
  const cwd = resolve(root, 'clients/go');
  execFileSync('go', ['build', '-trimpath', '-buildvcs=false', '-o', executable, './testdata/tjsv_probe'], {
    cwd,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 300000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const toolchain = execFileSync('go', ['version'], {
    cwd,
    env,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 30000,
    maxBuffer: 1024 * 1024,
  }).trim();
  requireThat(/^go version go[0-9]+\.[0-9]+/u.test(toolchain), 'unexpected Go toolchain identity');
  const binarySha256 = await executableDigest(executable);
  const response = readProbeExecution(invokeProbe(executable, request));
  const assessment = assessProbeResponse(rows, response, 'go');
  requireThat(git(root, 'rev-parse', 'HEAD').trim() === revision, 'candidate revision changed during Go admission');
  const after = await sourceSnapshot(root, revision);
  requireThat(JSON.stringify(after) === JSON.stringify(before), 'Go admission source changed during execution');
  requireThat(await executableDigest(executable) === binarySha256, 'Go probe executable changed during execution');
  return {
    status: assessment.status,
    toolchain,
    binarySha256,
    sourceDigests: before,
    results: assessment.results,
    findings: assessment.findings,
  };
}

/** Preserve the stable top-level TJSV/TypeScript/Rust result shape while Go gates status. */
export function mergeGoAdmission(prior, goEvidence) {
  requireThat(prior && Array.isArray(prior.results) && Array.isArray(prior.findings) && typeof prior.status === 'string', 'invalid prior admission evidence');
  requireThat(goEvidence && Array.isArray(goEvidence.results) && Array.isArray(goEvidence.findings) && typeof goEvidence.status === 'string', 'invalid Go admission evidence');
  requireThat(goEvidence.results.length === prior.results.length, 'Go admission coverage mismatch');
  requireThat((goEvidence.status === 'passed') === (goEvidence.findings.length === 0), 'inconsistent Go admission status');
  for (const [index, result] of prior.results.entries()) {
    const go = goEvidence.results[index];
    requireThat(go && go.name === result.name && go.kind === result.kind && go.expected === result.expected, 'Go admission identity mismatch');
    requireThat(typeof go.accepted === 'boolean' && (go.preserved === true || go.preserved === null || go.preserved === false), 'malformed Go admission result');
  }
  const findings = [
    ...prior.findings,
    ...goEvidence.findings.map(finding => ({ runtime: 'go', ...finding })),
  ];
  return {
    status: findings.length === 0 ? 'passed' : 'stopped_for_evaluation',
    results: prior.results,
    findings,
  };
}
