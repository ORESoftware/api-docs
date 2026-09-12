import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
  LOCK_PATH,
  auditFileMap,
  lockDigest,
  validateLock,
  validateLockFileInventory,
} from './check-tjsv-consumer-lock.mjs';

const realLock = JSON.parse(readFileSync(new URL('../contracts/tjsv-consumer.lock.json', import.meta.url), 'utf8'));

function fixture() {
  const revision = '1111111111111111111111111111111111111111';
  const schema = 'ores.example.receipt/v1';
  const workflow = '.github/workflows/example.yml';
  const runtime = 'scripts/example.mjs';
  const lock = {
    schema: 'ores.tjsv-consumer-lock/v1',
    repository: 'ORESoftware/typespec-json-schema-validator',
    sourceRevisionBinding: 'runtime-git-head',
    compatibilityPolicy: {
      inferenceFromGitAncestryAllowed: false,
      policyIssue: 'https://github.com/ORESoftware/.github/issues/75',
      upgradeAutomationIssue: 'https://github.com/ORESoftware/.github/issues/55',
    },
    profiles: [
      {
        id: 'example',
        revision,
        assuranceProfile: 'example-assurance',
        pinReferences: [workflow, runtime],
        evidenceSchemaChecks: [
          { schema, references: [runtime] },
        ],
      },
    ],
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
    [LOCK_PATH]: JSON.stringify(lock),
    [workflow]: `steps:\n  - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n    with:\n      repository: ORESoftware/typespec-json-schema-validator\n      ref: ${revision}\n`,
    [runtime]: `const TJSV_REVISION = '${revision}';\nconst RECEIPT_SCHEMA = '${schema}';\n`,
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
  assert.equal(realLock.profiles.length, 4);
  assert.ok(realLock.profiles.some(profile => profile.id === 'request-surface-current'));
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

test('a regex assertion mentioning TJSV_REVISION is not a consumer pin', () => {
  const { lock, fileMap, runtime } = fixture();
  fileMap[runtime] += "const TJSV_PATTERN = /TJSV_REVISION\\s*=\\s*['\"]([0-9a-f]{40})/;\n";
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
  expectFinding(auditFileMap(lock, fileMap), /compatibility must not be inferred from Git ancestry/);
});

test('stale or unknown immutable workflow pin fails', () => {
  const { lock, fileMap, workflow } = fixture();
  fileMap[workflow] = fileMap[workflow].replace(/1{40}/g, '2'.repeat(40));
  expectFinding(auditFileMap(lock, fileMap), /does not contain locked|differs from|not declared/);
});

test('mutable workflow ref fails', () => {
  const { lock, fileMap, workflow } = fixture();
  fileMap[workflow] = fileMap[workflow].replace(/1{40}/g, 'main');
  expectFinding(auditFileMap(lock, fileMap), /mutable or shortened|does not contain locked|differs from/);
});

test('shortened workflow SHA fails', () => {
  const { lock, fileMap, workflow } = fixture();
  fileMap[workflow] = fileMap[workflow].replace(/1{40}/g, '1111111');
  expectFinding(auditFileMap(lock, fileMap), /mutable or shortened|does not contain locked|differs from/);
});

test('wrong TJSV repository fails even with the locked revision', () => {
  const { lock, fileMap, workflow } = fixture();
  fileMap[workflow] = fileMap[workflow].replace(
    'repository: ORESoftware/typespec-json-schema-validator',
    'repository: attacker/typespec-json-schema-validator',
  );
  expectFinding(auditFileMap(lock, fileMap), /wrong TJSV repository/);
});

test('runtime constant drift fails', () => {
  const { lock, fileMap, runtime } = fixture();
  fileMap[runtime] = fileMap[runtime].replace(/1{40}/g, '3'.repeat(40));
  expectFinding(auditFileMap(lock, fileMap), /does not contain locked|undeclared TJSV revision/);
});

test('evidence schema drift fails independently of the validator pin', () => {
  const { lock, fileMap, runtime, schema } = fixture();
  fileMap[runtime] = fileMap[runtime].replace(schema, 'ores.example.receipt/v2');
  expectFinding(auditFileMap(lock, fileMap), /no longer contains locked evidence schema/);
});

test('an undeclared current reference cannot masquerade as historical evidence', () => {
  const { lock, fileMap, revision } = fixture();
  fileMap['scripts/undeclared.mjs'] = `const TJSV_REVISION = '${revision}';\n`;
  expectFinding(auditFileMap(lock, fileMap), /undeclared current TJSV revision|undeclared TJSV revision/);
});

test('a second profile cannot claim the same consumer path', () => {
  const { lock, fileMap, revision, workflow } = fixture();
  lock.profiles.push({
    id: 'duplicate-owner',
    revision: '2'.repeat(40),
    assuranceProfile: 'duplicate',
    pinReferences: [workflow],
    evidenceSchemaChecks: [],
  });
  redigest(lock);
  expectFinding(auditFileMap(lock, fileMap), /assigned to multiple TJSV profiles/);
});

test('lock repository identity itself is fail-closed', () => {
  const { lock, fileMap } = fixture();
  lock.repository = 'attacker/typespec-json-schema-validator';
  redigest(lock);
  expectFinding(auditFileMap(lock, fileMap), /wrong TJSV repository/);
});
