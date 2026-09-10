import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import {
  EXPECTED_REPOSITORY,
  LOCK_PATH,
  auditFileMap,
  lockDigest,
  validateLock,
  validateLockFileInventory,
} from './check-tjsv-consumer-lock.mjs';

const realLock = JSON.parse(await readFile(new URL('../contracts/tjsv-consumer.lock.json', import.meta.url), 'utf8'));

function fixture() {
  const revision = 'a'.repeat(40);
  const schema = 'ores.example.receipt/v1';
  const workflow = '.github/workflows/example.yml';
  const runtime = 'scripts/runtime.mjs';
  const lock = {
    schema: 'ores.tjsv-consumer-lock/v1',
    repository: EXPECTED_REPOSITORY,
    sourceRevisionBinding: 'runtime-git-head',
    compatibilityPolicy: {
      inferenceFromGitAncestryAllowed: false,
      policyIssue: 'https://github.com/ORESoftware/.github/issues/75',
      upgradeAutomationIssue: 'https://github.com/ORESoftware/.github/issues/55',
    },
    profiles: [{
      id: 'example',
      revision,
      assuranceProfile: 'example-boundary',
      pinReferences: [workflow, runtime],
      evidenceSchemaChecks: [{ schema, references: [runtime] }],
    }],
    scanPolicy: {
      currentReferenceRoots: ['.github/workflows', 'scripts'],
      allowMutableRefs: false,
      allowShortShas: false,
      allowUndeclaredCurrentReferences: false,
      allowWrongRepository: false,
    },
    selfDigestAlgorithm: 'sha256-sorted-json-v1',
    selfDigest: '',
  };
  lock.selfDigest = lockDigest(lock);
  const fileMap = {
    [LOCK_PATH]: `${JSON.stringify(lock)}\n`,
    [workflow]: [
      'name: example',
      'jobs:',
      '  check:',
      '    steps:',
      '      - uses: actions/checkout@0000000000000000000000000000000000000000',
      '        with:',
      `          repository: ${EXPECTED_REPOSITORY}`,
      `          ref: ${revision}`,
      '',
    ].join('\n'),
    [runtime]: [
      `export const TJSV_REVISION = '${revision}';`,
      `export const RECEIPT_SCHEMA = '${schema}';`,
      '',
    ].join('\n'),
  };
  return { lock, fileMap, revision, schema, workflow, runtime };
}

function redigest(lock) {
  lock.selfDigest = lockDigest(lock);
  return lock;
}

function expectFinding(result, pattern) {
  assert.equal(result.status, 'failed');
  assert.ok(result.findings.some(finding => pattern.test(finding)), result.findings.join('\n'));
}

test('repository lock is structurally valid and self-digesting', () => {
  validateLock(realLock);
  assert.equal(lockDigest(realLock), realLock.selfDigest);
  assert.equal(realLock.compatibilityPolicy.inferenceFromGitAncestryAllowed, false);
  assert.equal(realLock.profiles.length, 3);
});

test('exactly one canonical consumer lock is required', () => {
  assert.equal(validateLockFileInventory([LOCK_PATH]), true);
  assert.throws(
    () => validateLockFileInventory([LOCK_PATH, 'docs/tjsv-consumer.lock.json']),
    /exactly one canonical TJSV consumer lock/,
  );
  assert.throws(() => validateLockFileInventory([]), /exactly one canonical TJSV consumer lock/);
});

test('a declared immutable workflow/runtime/schema profile passes', () => {
  const { lock, fileMap } = fixture();
  assert.deepEqual(auditFileMap(lock, fileMap), { status: 'passed', findings: [] });
});

test('self-digest tampering fails closed', () => {
  const { lock, fileMap } = fixture();
  lock.profiles[0].assuranceProfile = 'tampered';
  expectFinding(auditFileMap(lock, fileMap), /selfDigest mismatch/);
});

test('compatibility cannot be inferred from Git ancestry', () => {
  const { lock, fileMap } = fixture();
  lock.compatibilityPolicy.inferenceFromGitAncestryAllowed = true;
  redigest(lock);
  expectFinding(auditFileMap(lock, fileMap), /must not be inferred from Git ancestry/);
});

test('stale or unknown immutable workflow pin fails', () => {
  const { lock, fileMap, workflow, revision } = fixture();
  fileMap[workflow] = fileMap[workflow].replace(revision, 'b'.repeat(40));
  expectFinding(auditFileMap(lock, fileMap), /differs from|not declared/);
});

test('mutable workflow ref fails', () => {
  const { lock, fileMap, workflow, revision } = fixture();
  fileMap[workflow] = fileMap[workflow].replace(revision, 'main');
  expectFinding(auditFileMap(lock, fileMap), /mutable or shortened|differs from/);
});

test('shortened workflow SHA fails', () => {
  const { lock, fileMap, workflow, revision } = fixture();
  fileMap[workflow] = fileMap[workflow].replace(revision, revision.slice(0, 12));
  expectFinding(auditFileMap(lock, fileMap), /mutable or shortened|differs from/);
});

test('wrong TJSV repository fails even with the locked revision', () => {
  const { lock, fileMap, workflow } = fixture();
  const wrong = ['someone', 'typespec-json-schema-validator'].join('/');
  fileMap[workflow] = fileMap[workflow].replace(EXPECTED_REPOSITORY, wrong);
  expectFinding(auditFileMap(lock, fileMap), /wrong TJSV repository/);
});

test('runtime constant drift fails', () => {
  const { lock, fileMap, runtime, revision } = fixture();
  fileMap[runtime] = fileMap[runtime].replace(revision, 'c'.repeat(40));
  expectFinding(auditFileMap(lock, fileMap), /does not contain locked|undeclared TJSV revision/);
});

test('evidence schema drift fails independently of the validator pin', () => {
  const { lock, fileMap, runtime, schema } = fixture();
  fileMap[runtime] = fileMap[runtime].replace(schema, 'ores.example.receipt/v2');
  expectFinding(auditFileMap(lock, fileMap), /no longer contains locked evidence schema/);
});

test('an undeclared current reference cannot masquerade as historical evidence', () => {
  const { lock, fileMap, revision } = fixture();
  fileMap['docs/history.md'] = `old validator ${revision}\n`;
  expectFinding(auditFileMap(lock, fileMap), /undeclared current TJSV revision/);
});

test('a second profile cannot claim the same consumer path', () => {
  const { lock, fileMap, workflow } = fixture();
  lock.profiles.push({
    id: 'duplicate-owner',
    revision: 'd'.repeat(40),
    assuranceProfile: 'duplicate',
    pinReferences: [workflow],
    evidenceSchemaChecks: [],
  });
  redigest(lock);
  expectFinding(auditFileMap(lock, fileMap), /assigned to multiple TJSV profiles/);
});

test('lock repository identity itself is fail-closed', () => {
  const { lock, fileMap } = fixture();
  lock.repository = ['someone', 'typespec-json-schema-validator'].join('/');
  redigest(lock);
  expectFinding(auditFileMap(lock, fileMap), /wrong TJSV repository/);
});
