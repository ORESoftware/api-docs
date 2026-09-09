import assert from 'node:assert/strict';
import { execFile as execFileCallback } from 'node:child_process';
import { link, lstat, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { readSafeBytes, readSafeJson, validRelativePath, writeOwnedJson } from './projection-evidence-io.mjs';
import { verifyValidatorSource } from './tjsv-source-integrity.mjs';

const execFile = promisify(execFileCallback);
const owned = new Set(['owned/v1']);
const output = '{"schema":"owned/v1","status":"passed"}\n';
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'api-docs-evidence-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, 'evidence'));
  await writeFile(join(root, 'evidence/value.json'), '{"value":1}\n');
  return root;
}

for (const path of ['', '.', '..', '../outside', '/outside', 'a/../b', 'a/./b', 'a//b', 'a/', 'a\\b', 'C:/outside', 'C:relative', '\\host\\share', 'a\u0000b', 'a\nb', 'a'.repeat(513)]) {
  test(`rejects non-portable evidence path ${JSON.stringify(path)}`, async (t) => {
    const root = await fixture(t);
    assert.equal(validRelativePath(path), false);
    await assert.rejects(readSafeBytes(root, path));
  });
}

test('reads exactly the current regular evidence bytes', async (t) => {
  const root = await fixture(t);
  assert.deepEqual(await readSafeJson(root, 'evidence/value.json'), { value: 1 });
  assert.equal((await readSafeBytes(root, 'evidence/value.json')).toString(), '{"value":1}\n');
});

for (const scope of ['external', 'internal']) {
  test(`rejects ${scope} ancestor symlink for reads and writes`, async (t) => {
    const root = await fixture(t);
    const target = scope === 'external' ? await fixture(t) : join(root, 'evidence');
    await symlink(target, join(root, 'redirect'), 'dir');
    const path = scope === 'external' ? 'redirect/evidence/value.json' : 'redirect/value.json';
    await assert.rejects(readSafeBytes(root, path), /ancestors/u);
    await assert.rejects(writeOwnedJson(root, 'redirect/new/deeper/report.json', output, owned), /ancestors/u);
    await assert.rejects(lstat(join(target, 'new')), { code: 'ENOENT' });
  });
}

test('rejects a symlink root', async (t) => {
  const root = await fixture(t);
  const outside = await fixture(t);
  await symlink(outside, join(root, 'alias'), 'dir');
  await assert.rejects(readSafeJson(join(root, 'alias'), 'evidence/value.json'), /real directory/u);
});

for (const dangling of [false, true]) {
  test(`rejects ${dangling ? 'dangling' : 'live'} final symlink`, async (t) => {
    const root = await fixture(t);
    await symlink(join(root, dangling ? 'absent' : 'evidence/value.json'), join(root, 'evidence/alias.json'));
    await assert.rejects(readSafeBytes(root, 'evidence/alias.json'), /regular file/u);
    await assert.rejects(writeOwnedJson(root, 'evidence/alias.json', output, owned));
    assert.equal((await lstat(join(root, 'evidence/alias.json'))).isSymbolicLink(), true);
  });
}

test('rejects hard-linked evidence and destinations', async (t) => {
  const root = await fixture(t);
  await link(join(root, 'evidence/value.json'), join(root, 'copy.json'));
  await assert.rejects(readSafeBytes(root, 'evidence/value.json'), /regular file/u);
  await assert.rejects(writeOwnedJson(root, 'evidence/value.json', output, owned), /regular file/u);
});

test('rejects directories and enforces the byte limit', async (t) => {
  const root = await fixture(t);
  await assert.rejects(readSafeBytes(root, 'evidence'), /regular file/u);
  await assert.rejects(readSafeBytes(root, 'evidence/value.json', 2), /byte limit/u);
  for (const limit of [0, -1, NaN, Infinity, 1.5, 8 * 1024 * 1024 + 1]) {
    await assert.rejects(readSafeBytes(root, 'evidence/value.json', limit), /byte limit/u);
  }
});

for (const bytes of [Buffer.from('{PRIVATE_SENTINEL'), Buffer.from([0x7b, 0x22, 0x78, 0x22, 0x3a, 0x22, 0xff, 0x22, 0x7d]), Buffer.from('\ufeff{}')]) {
  test(`rejects malformed JSON/UTF-8 ${bytes.toString('hex')}`, async (t) => {
    const root = await fixture(t);
    await writeFile(join(root, 'evidence/value.json'), bytes);
    await assert.rejects(readSafeJson(root, 'evidence/value.json'), (error) => {
      assert.equal(error.message, 'evidence file must contain valid UTF-8 JSON');
      assert.equal(error.message.includes('PRIVATE_SENTINEL'), false);
      return true;
    });
  });
}

test('writes and replaces only owned JSON without touching unrelated temporary files', async (t) => {
  const root = await fixture(t);
  const path = 'new/nested/report.json';
  await writeOwnedJson(root, path, output, owned);
  const tempSentinel = join(root, `${path}.tmp-${process.pid}`);
  await writeFile(tempSentinel, 'DO NOT REMOVE');
  await writeOwnedJson(root, path, '{"schema":"owned/v1","status":"failed"}\n', owned);
  assert.equal((await readSafeJson(root, path)).status, 'failed');
  assert.equal(await readFile(tempSentinel, 'utf8'), 'DO NOT REMOVE');
  assert.deepEqual((await readdir(join(root, 'new/nested'))).sort(), ['report.json', `report.json.tmp-${process.pid}`].sort());
});

test('never overwrites foreign or malformed evidence', async (t) => {
  const root = await fixture(t);
  await assert.rejects(writeOwnedJson(root, 'evidence/value.json', output, owned), /not owned/u);
  assert.equal(await readFile(join(root, 'evidence/value.json'), 'utf8'), '{"value":1}\n');
  await writeFile(join(root, 'evidence/value.json'), '{malformed');
  await assert.rejects(writeOwnedJson(root, 'evidence/value.json', output, owned), /valid UTF-8 JSON/u);
  assert.equal(await readFile(join(root, 'evidence/value.json'), 'utf8'), '{malformed');
});

async function checkout(t) {
  const root = await fixture(t);
  const git = async (...args) => (await execFile('git', ['-C', root, ...args], { encoding: 'utf8' })).stdout.trim();
  await git('init', '--quiet');
  await mkdir(join(root, 'src'));
  await writeFile(join(root, 'src/index.mjs'), 'export const value = 1;\n');
  await writeFile(join(root, '.gitignore'), 'src/ignored.mjs\nnode_modules/\n');
  await git('add', '--', 'src/index.mjs', '.gitignore');
  await git('-c', 'user.name=Evidence Test', '-c', 'user.email=evidence@example.invalid', 'commit', '--quiet', '-m', 'synthetic source fixture');
  return { root, git, head: await git('rev-parse', 'HEAD') };
}

test('accepts a clean exact pinned source checkout', async (t) => {
  const { root, head } = await checkout(t);
  await verifyValidatorSource(root, head);
});

for (const indexFlag of [null, '--assume-unchanged', '--skip-worktree']) {
  test(`rejects changed tracked validator bytes with ${indexFlag ?? 'normal index'}`, async (t) => {
    const { root, git, head } = await checkout(t);
    if (indexFlag) await git('update-index', indexFlag, '--', 'src/index.mjs');
    await writeFile(join(root, 'src/index.mjs'), 'export const value = 2;\n');
    assert.equal(await git('rev-parse', 'HEAD'), head);
    await assert.rejects(verifyValidatorSource(root, head), /tracked source differs/u);
  });
}

for (const path of ['src/extra.mjs', 'src/ignored.mjs']) {
  test(`rejects untracked validator code ${path}`, async (t) => {
    const { root, head } = await checkout(t);
    await writeFile(join(root, path), 'export const injected = true;\n');
    await assert.rejects(verifyValidatorSource(root, head), /untracked files/u);
  });
}

test('rejects the wrong revision and a nested fake validator root', async (t) => {
  const { root, head } = await checkout(t);
  await assert.rejects(verifyValidatorSource(root, '0'.repeat(40)), /pinned revision/u);
  await assert.rejects(verifyValidatorSource(root, 'main'), /revision is invalid/u);
  await assert.rejects(verifyValidatorSource(join(root, 'src'), head), /repository root/u);
});

test('rejects tracked source replaced by a symlink', async (t) => {
  const { root, head } = await checkout(t);
  await writeFile(join(root, 'target.mjs'), 'export const value = 1;\n');
  await rm(join(root, 'src/index.mjs'));
  await symlink(join(root, 'target.mjs'), join(root, 'src/index.mjs'));
  await assert.rejects(verifyValidatorSource(root, head), /regular file/u);
});

// These exercise the actual api-docs policy consumer, not only its IO helper.
// The validator toolchain is intentionally absent: real compiler/Contract IR
// integration remains in test-rpc-v1-projection-admission.mjs in hosted CI.
async function policyFixture(t) {
  const root = await fixture(t);
  const policy = {
    schema: 'ores.api-docs.rpc-v1-projection-admission-policy/v1',
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: '1'.repeat(40), manifestSchema: 'ores.typespec-json-schema-validator.projection-manifest/v1' },
    activation: { status: 'blocked', manifestPath: 'evidence/manifest.json', reportPath: 'evidence/report.json', blockers: [{ id: 'pending', reason: 'Exact product evidence is pending.' }] },
    inputs: { operationInventory: 'evidence/value.json', projectionMetadata: 'evidence/value.json', emitterConfiguration: 'policy.json' },
    outputs: [{ path: 'evidence/value.json', mediaType: 'application/json', projection: 'demo' }],
    projections: [{ id: 'demo', emitter: 'demo', declarationMode: 'all', representationDeltaIds: [], runtimeValidatorIds: [] }],
    toolchains: [{ id: 'demo', version: '1', root: 'repository', files: ['evidence/value.json'] }],
    approvedDeltas: null, runtimeValidators: null,
  };
  await writeFile(join(root, 'policy.json'), JSON.stringify(policy));
  return { root, policy };
}

test('actual policy audit stays blocked and rejects an ancestor-link input', async (t) => {
  const { auditProjectionAdmissionPolicy } = await import('./rpc-v1-projection-admission.mjs');
  const { root, policy } = await policyFixture(t);
  const run = () => auditProjectionAdmissionPolicy({ root, policyPath: 'policy.json' });
  const baseline = await run();
  assert.equal(baseline.status, 'passed');
  assert.equal(baseline.projectionAdmissible, false);
  const outside = await fixture(t);
  await symlink(outside, join(root, 'redirect'), 'dir');
  policy.inputs.operationInventory = 'redirect/evidence/value.json';
  await writeFile(join(root, 'policy.json'), JSON.stringify(policy));
  const rejected = await run();
  assert.equal(rejected.status, 'failed');
  assert.ok(rejected.findings.some((item) => item.ruleId === 'projection-policy-input-unreadable'));
});

test('actual policy audit refuses corrupt UTF-8 rather than accepting replacement text', async (t) => {
  const { auditProjectionAdmissionPolicy } = await import('./rpc-v1-projection-admission.mjs');
  const { root, policy } = await policyFixture(t);
  const bytes = Buffer.from(JSON.stringify(policy));
  bytes[bytes.indexOf(Buffer.from('Exact'))] = 0xff;
  await writeFile(join(root, 'policy.json'), bytes);
  const report = await auditProjectionAdmissionPolicy({ root, policyPath: 'policy.json' });
  assert.equal(report.status, 'failed');
  assert.equal(report.findings[0].ruleId, 'projection-policy-invalid');
  assert.equal(report.findings[0].message, 'evidence file must contain valid UTF-8 JSON');
});
