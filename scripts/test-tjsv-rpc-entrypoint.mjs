import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, appendFile, chmod, link, mkdir, mkdtemp, readFile, realpath, rename, rm, stat, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { TJSV_REVISION } from './tjsv-rpc-admission.mjs';

const exec = promisify(execFile);
const sourceRoot = new URL('../', import.meta.url);
const environment = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_')));
const git = async (root, ...args) => (await exec('git', ['-C', root, ...args], { env: environment })).stdout.trim();
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const receiptPath = 'tmp/tjsv-rpc-admission.json';
const schemaPath = 'json-schema/rpc-call.schema.json';
const validatorPath = 'src/index.mjs';

function fixture(name, kind, accepted) {
  const encoded = JSON.stringify({ accepted, kind });
  return { name, kind, encoded, ...(accepted ? { tcp_prefix_hex: Buffer.byteLength(encoded).toString(16).padStart(8, '0') } : {}) };
}

// Synthetic oracles exercise the entrypoint's filesystem/source boundary, not
// TJSV or Rust semantics. Production CI separately imports the immutable real
// TJSV pin and compiles/runs the real Rust client oracle.
const validatorSource = `
import { appendFileSync, writeFileSync } from 'node:fs';
writeFileSync(new URL('../imported.marker', import.meta.url), 'imported');
export class SchemaResolver { addDocument(_schema, path) { return { base: path }; } }
export function validateJsonSchemaDocument() { return []; }
export function validateInstance({ instance }) {
  return { valid: instance.accepted, errors: instance.accepted ? [] : [{}] };
}
`;
const runtimeSource = `
export class RpcV1Error extends Error {}
export function decodeCall(encoded) {
  const value = JSON.parse(encoded);
  if (!value.accepted) throw new RpcV1Error();
  return value;
}
export const decodeReceipt = decodeCall;
`;
const cargoOracle = `#!/bin/sh
node - <<'NODE'
const { readFileSync } = require('node:fs');
const corpus = JSON.parse(readFileSync('examples/rpc-v1/conformance.json', 'utf8'));
const results = [];
for (const [group, accepted] of [['valid', true], ['invalid', false]]) {
  for (const entry of corpus[group]) {
    const row = { name: entry.name, kind: entry.kind, accepted };
    if (accepted) row.decoded = JSON.parse(entry.encoded);
    results.push(row);
  }
}
process.stdout.write(JSON.stringify({ schema: 'ores.api-docs.rust-rpc-admission/v1', results }));
NODE
`;

async function setup(t, { validator = validatorSource, runtime = runtimeSource } = {}) {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'tjsv-rpc-entrypoint-')));
  t.after(() => rm(root, { recursive: true, force: true })); // Only this test's owned mkdtemp root.
  const put = async (path, bytes) => {
    await mkdir(dirname(join(root, path)), { recursive: true });
    await writeFile(join(root, path), bytes);
  };
  for (const path of [
    'scripts/tjsv-source-integrity.mjs',
    'scripts/projection-evidence-io.mjs',
    'scripts/tjsv-rust-admission.mjs',
    'scripts/test_tjsv_rpc_admission.mjs',
    'scripts/test_tjsv_rust_admission.mjs',
    'scripts/test-tjsv-rpc-entrypoint.mjs',
    'scripts/test-projection-evidence-io.mjs',
    '.github/workflows/tjsv-rpc-admission.yml',
  ]) {
    await put(path, await readFile(new URL(path, sourceRoot)));
  }
  await put('tmp/tjsv/src/index.mjs', validator);
  await put('tmp/tjsv/.gitignore', 'src/ignored.mjs\n');
  const validatorRoot = join(root, 'tmp/tjsv');
  await git(validatorRoot, 'init', '-q');
  await git(validatorRoot, 'add', '--', 'src/index.mjs', '.gitignore');
  await git(validatorRoot, '-c', 'user.name=Boundary Test', '-c', 'user.email=boundary@example.invalid', '-c', 'commit.gpgSign=false', 'commit', '-qm', 'synthetic validator');
  const revision = await git(validatorRoot, 'rev-parse', 'HEAD');
  const original = await readFile(new URL('scripts/tjsv-rpc-admission.mjs', sourceRoot), 'utf8');
  assert.equal(original.split(TJSV_REVISION).length, 2, 'exactly one immutable pin must be substituted in the owned fixture');
  // No production flag or environment override is added. Only this copied test
  // runner's constant changes so it can verify a real, local synthetic Git tree.
  await put('scripts/tjsv-rpc-admission.mjs', original.replace(TJSV_REVISION, revision));
  await put('package.json', '{"type":"module"}\n');
  await put('.gitignore', 'tmp/\ntemp/\n');
  await put('clients/typescript/src/rpc.js', runtime);
  await put('Cargo.toml', '[workspace]\nresolver = "2"\n');
  await put('Cargo.lock', '# Synthetic evidence only; the test-owned cargo executable never parses this file.\nversion = 4\n');
  await put('rust/src/lib.rs', '// Synthetic tracked Rust-core evidence for source-inventory admission.\n');
  await put('clients/rust/Cargo.toml', '[package]\nname = "synthetic-rust-admission"\nversion = "0.0.0"\nedition = "2021"\n');
  await put('clients/rust/examples/tjsv_admission.rs', '// Synthetic tracked Rust-oracle evidence; execution is supplied by the test-owned cargo shim.\nfn main() {}\n');
  await put('toolchain/cargo', cargoOracle);
  await chmod(join(root, 'toolchain/cargo'), 0o755);
  const schema = { $schema: 'https://json-schema.org/draft/2020-12/schema', type: 'object' };
  for (const kind of ['call', 'receipt']) await put(`json-schema/rpc-${kind}.schema.json`, JSON.stringify(schema));
  await put('idl/typespec/v1.tsp', '// independent TypeSpec fixture\n');
  await put('runtime/v1-conformance.json', '{}\n');
  await put('examples/rpc-v1/conformance.json', JSON.stringify({
    schemaVersion: 1, profile: 'ores-rpc-v1-call-receipt', maxFrameBytes: 8388608, tcpLengthPrefixBytes: 4,
    valid: [fixture('valid-call', 'call', true), fixture('valid-receipt', 'receipt', true)],
    invalid: [fixture('invalid-call', 'call', false), fixture('invalid-receipt', 'receipt', false)],
  }));
  await git(root, 'init', '-q');
  await git(root, 'add', '--', 'scripts', 'package.json', '.gitignore', 'clients', 'json-schema', 'idl', 'runtime', 'examples', '.github', 'Cargo.toml', 'Cargo.lock', 'rust');
  await git(root, '-c', 'user.name=Boundary Test', '-c', 'user.email=boundary@example.invalid', '-c', 'commit.gpgSign=false', 'commit', '-qm', 'synthetic consumer');
  const runEnvironment = {
    ...environment,
    PATH: [join(root, 'toolchain'), environment.PATH].filter(Boolean).join(delimiter),
  };
  const run = async (...args) => {
    try {
      const result = await exec(process.execPath, ['scripts/tjsv-rpc-admission.mjs', ...args], { cwd: root, env: runEnvironment, timeout: 20000 });
      return { code: 0, ...result };
    } catch (error) {
      if (typeof error.code !== 'number') throw error;
      return { code: error.code, stdout: error.stdout, stderr: error.stderr };
    }
  };
  return { root, validatorRoot, revision, put, run };
}

async function failed(f, pattern, { beforeImport = false, existingOutput = false } = {}) {
  const result = await f.run();
  assert.equal(result.code, 3, result.stdout + result.stderr);
  const failure = JSON.parse(result.stderr.trim());
  assert.equal(failure.status, 'failed');
  assert.match(failure.error, pattern);
  if (!existingOutput) await assert.rejects(access(join(f.root, receiptPath)), { code: 'ENOENT' });
  if (beforeImport) await assert.rejects(access(join(f.validatorRoot, 'imported.marker')), { code: 'ENOENT' });
}

test('entrypoint emits digest-bound deterministic evidence with synthetic oracles', async t => {
  const f = await setup(t);
  assert.equal((await f.run()).code, 0);
  const first = await readFile(join(f.root, receiptPath));
  const report = JSON.parse(first);
  assert.equal(report.status, 'passed');
  assert.equal(report.validator.revision, f.revision);
  assert.equal(report.sourceRevision, await git(f.root, 'rev-parse', 'HEAD'));
  assert.equal(report.coverage.fixtures, 4);
  assert.equal(report.coverage.universalEquivalenceProven, false);
  for (const path of ['scripts/tjsv-source-integrity.mjs', 'scripts/projection-evidence-io.mjs', 'scripts/tjsv-rust-admission.mjs']) {
    assert.equal(report.sourceDigests[path], digest(await readFile(join(f.root, path))));
  }
  if (process.platform !== 'win32') assert.equal((await stat(join(f.root, receiptPath))).mode & 0o777, 0o600);
  await rm(join(f.root, receiptPath));
  assert.equal((await f.run()).code, 0);
  assert.deepEqual(await readFile(join(f.root, receiptPath)), first);
});

for (const flag of ['--assume-unchanged', '--skip-worktree']) test(`rejects hidden validator edits (${flag}) before import`, async t => {
  const f = await setup(t);
  await git(f.validatorRoot, 'update-index', flag, '--', validatorPath);
  await appendFile(join(f.validatorRoot, validatorPath), '\n// hidden edit\n');
  assert.equal(await git(f.validatorRoot, 'status', '--porcelain', '--untracked-files=no'), '');
  await failed(f, /tracked source differs/, { beforeImport: true });
});

test('rejects ignored untracked validator source before import', async t => {
  const f = await setup(t);
  await f.put('tmp/tjsv/src/ignored.mjs', 'export const injected = true;\n');
  assert.equal(await git(f.validatorRoot, 'status', '--porcelain'), '');
  await failed(f, /untracked files/, { beforeImport: true });
});

test('rejects wrong validator revision before import', async t => {
  const f = await setup(t);
  await git(f.validatorRoot, '-c', 'user.name=Boundary Test', '-c', 'user.email=boundary@example.invalid', '-c', 'commit.gpgSign=false', 'commit', '--allow-empty', '-qm', 'different revision');
  await failed(f, /pinned revision/, { beforeImport: true });
});

test('rechecks validator bytes after module execution', async t => {
  const f = await setup(t, { validator: validatorSource + `\nappendFileSync(new URL('./index.mjs', import.meta.url), '\\n// changed during import\\n');\n` });
  await failed(f, /tracked source differs/);
  await access(join(f.validatorRoot, 'imported.marker'));
});

test('rechecks consumer evidence after runtime execution', async t => {
  const f = await setup(t, { runtime: runtimeSource + `\nimport { appendFileSync } from 'node:fs';\nappendFileSync(new URL('../../../idl/typespec/v1.tsp', import.meta.url), '// changed\\n');\n` });
  await failed(f, /source changed during admission/);
});

for (const kind of ['symbolic', 'hard']) test(`rejects ${kind}-linked schema evidence`, async t => {
  const f = await setup(t);
  const path = join(f.root, schemaPath);
  const target = join(f.root, 'schema-target.json');
  await rename(path, target);
  if (kind === 'symbolic') await symlink(target, path);
  else await link(target, path);
  await failed(f, /singly linked regular file/, { beforeImport: true });
});

test('rejects symlinked evidence ancestors', async t => {
  const f = await setup(t);
  await rename(join(f.root, 'examples'), join(f.root, 'examples-target'));
  await symlink(join(f.root, 'examples-target'), join(f.root, 'examples'), 'dir');
  await failed(f, /ancestors must be real directories/, { beforeImport: true });
});

test('rejects evidence beyond the byte bound before import', async t => {
  const f = await setup(t);
  await f.put(schemaPath, Buffer.alloc(8 * 1024 * 1024 + 1, 0x20));
  await failed(f, /byte limit/, { beforeImport: true });
});

test('rejects malformed UTF-8 rather than replacing bytes during JSON decoding', async t => {
  const f = await setup(t);
  await f.put(schemaPath, Buffer.concat([
    Buffer.from('{"$schema":"https://json-schema.org/draft/2020-12/schema","description":"'),
    Buffer.from([0xff]), Buffer.from('"}'),
  ]));
  await failed(f, /valid UTF-8 JSON/);
});

for (const schema of ['ores.api-docs.tjsv-rpc-admission/v1', 'other-tool/v1']) test(`preserves existing output (${schema})`, async t => {
  const f = await setup(t);
  const bytes = JSON.stringify({ schema, status: 'passed', sentinel: 'do not overwrite' });
  await f.put(receiptPath, bytes);
  await failed(f, /refusing to replace/, { existingOutput: true });
  assert.equal(await readFile(join(f.root, receiptPath), 'utf8'), bytes);
});

test('does not follow output symlinks or change their targets', async t => {
  const f = await setup(t);
  await f.put('sentinel.json', '{"sentinel":true}');
  await symlink(join(f.root, 'sentinel.json'), join(f.root, receiptPath));
  await failed(f, /singly linked regular file/, { existingOutput: true });
  assert.equal(await readFile(join(f.root, 'sentinel.json'), 'utf8'), '{"sentinel":true}');
});

test('unknown CLI arguments fail before importing the validator', async t => {
  const f = await setup(t);
  const result = await f.run('--skip-validation');
  assert.equal(result.code, 3);
  assert.match(JSON.parse(result.stderr).error, /accepts no arguments/);
  await assert.rejects(access(join(f.validatorRoot, 'imported.marker')), { code: 'ENOENT' });
});

test('runtime disagreement is stopped_for_evaluation, not a passing receipt', async t => {
  const f = await setup(t, { runtime: `export class RpcV1Error extends Error {}\nexport const decodeCall = JSON.parse;\nexport const decodeReceipt = JSON.parse;\n` });
  assert.equal((await f.run()).code, 2);
  const report = JSON.parse(await readFile(join(f.root, receiptPath)));
  assert.equal(report.status, 'stopped_for_evaluation');
  assert.equal(report.findings.length, 2);
});
