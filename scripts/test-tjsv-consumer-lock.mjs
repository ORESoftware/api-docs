import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  LOCK_PATH,
  auditFileMap,
  lockDigest,
  validateLock,
  validateLockFileInventory,
} from './check-tjsv-consumer-lock.mjs';

const realLock = JSON.parse(readFileSync(new URL('../contracts/tjsv-consumer.lock.json', import.meta.url), 'utf8'));

function baseLock() {
  const lock = structuredClone(realLock);
  lock.profiles = [
    {
      id: 'fixture-profile',
      revision: '1111111111111111111111111111111111111111',
      assuranceProfile: 'fixture-assurance',
      pinReferences: ['.github/workflows/fixture.yml'],
      evidenceSchemaChecks: [
        {
          schema: 'fixture.report/v1',
          references: ['scripts/fixture.mjs'],
        },
      ],
    },
  ];
  lock.selfDigest = lockDigest(lock);
  return lock;
}

function fileMapFor(lock, workflow = null) {
  const profile = lock.profiles[0];
  return {
    [LOCK_PATH]: JSON.stringify(lock),
    '.github/workflows/fixture.yml': workflow ?? `repository: ORESoftware/typespec-json-schema-validator\n  ref: ${profile.revision}\n`,
    'scripts/fixture.mjs': `const schema = 'fixture.report/v1';\nconst TJSV_REVISION = '${profile.revision}';\n`,
  };
}

function expectFinding(result, pattern) {
  assert.equal(result.status, 'failed');
  assert.ok(result.findings.some(finding => pattern.test(finding)), result.findings.join('\n'));
}

test('repository lock is structurally valid and self-digesting', () => {
  validateLock(realLock);
  assert.equal(lockDigest(realLock), realLock.selfDigest);
  assert.equal(realLock.compatibilityPolicy.inferenceFromGitAncestryAllowed, false);
  assert.equal(realLock.profiles.length, 6);
  assert.ok(realLock.profiles.some(profile => profile.id === 'request-surface-current'));
  const pageManifest = realLock.profiles.find(
    profile => profile.id === 'web-page-manifest-peer-authority',
  );
  assert.ok(pageManifest);
  assert.equal(pageManifest.revision, '7cf36bbcbd9523caaf894ac9188bd29633b7ac9f');
  assert.deepEqual(pageManifest.pinReferences, ['.github/workflows/ores-web-page-manifest.yml']);
  const lambdaDeployment = realLock.profiles.find(
    profile => profile.id === 'lambda-deployment-peer-authority',
  );
  assert.ok(lambdaDeployment);
  assert.equal(lambdaDeployment.revision, '7cf36bbcbd9523caaf894ac9188bd29633b7ac9f');
  assert.deepEqual(lambdaDeployment.pinReferences, ['.github/workflows/lambda-deployment-docs.yml']);
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
  const lock = baseLock();
  assert.deepEqual(auditFileMap(lock, fileMapFor(lock)), { status: 'passed', findings: [] });
});

test('a direct composite-action pin is a first-class declared workflow consumer', () => {
  const lock = baseLock();
  const revision = lock.profiles[0].revision;
  const map = fileMapFor(lock, `- uses: ORESoftware/typespec-json-schema-validator@${revision}\n`);
  assert.deepEqual(auditFileMap(lock, map), { status: 'passed', findings: [] });
});

test('separate assurance profiles may share one reviewed immutable validator revision', () => {
  const lock = baseLock();
  lock.profiles.push({
    id: 'fixture-profile-two',
    revision: lock.profiles[0].revision,
    assuranceProfile: 'fixture-assurance-two',
    pinReferences: ['.github/workflows/fixture-two.yml'],
    evidenceSchemaChecks: [],
  });
  lock.selfDigest = lockDigest(lock);
  const map = fileMapFor(lock);
  map['.github/workflows/fixture-two.yml'] = `- uses: ORESoftware/typespec-json-schema-validator@${lock.profiles[0].revision}\n`;
  assert.deepEqual(auditFileMap(lock, map), { status: 'passed', findings: [] });
});

test('a regex assertion mentioning TJSV_REVISION is not a consumer pin', () => {
  const lock = baseLock();
  const map = fileMapFor(lock);
  map['scripts/nonconsumer.mjs'] = 'const pattern = /^TJSV_REVISION=[0-9a-f]{40}$/;\n';
  assert.deepEqual(auditFileMap(lock, map), { status: 'passed', findings: [] });
});

test('self-digest tampering fails closed', () => {
  const lock = baseLock();
  lock.selfDigest = '0'.repeat(64);
  expectFinding(auditFileMap(lock, fileMapFor(lock)), /selfDigest mismatch/);
});

test('compatibility cannot be inferred from Git ancestry', () => {
  const lock = baseLock();
  lock.compatibilityPolicy.inferenceFromGitAncestryAllowed = true;
  lock.selfDigest = lockDigest(lock);
  expectFinding(auditFileMap(lock, fileMapFor(lock)), /must not be inferred from Git ancestry/);
});

test('stale or unknown immutable workflow pin fails', () => {
  const lock = baseLock();
  const stale = '2'.repeat(40);
  expectFinding(
    auditFileMap(lock, fileMapFor(lock, `repository: ORESoftware/typespec-json-schema-validator\n  ref: ${stale}\n`)),
    /differs from|not declared/,
  );
});

test('mutable workflow ref fails', () => {
  const lock = baseLock();
  expectFinding(
    auditFileMap(lock, fileMapFor(lock, 'repository: ORESoftware/typespec-json-schema-validator\n  ref: main\n')),
    /mutable or shortened/,
  );
});

test('shortened workflow SHA fails', () => {
  const lock = baseLock();
  expectFinding(
    auditFileMap(lock, fileMapFor(lock, 'repository: ORESoftware/typespec-json-schema-validator\n  ref: 1111111\n')),
    /mutable or shortened/,
  );
});

test('wrong TJSV repository fails even with the locked revision', () => {
  const lock = baseLock();
  const revision = lock.profiles[0].revision;
  expectFinding(
    auditFileMap(lock, fileMapFor(lock, `repository: attacker/typespec-json-schema-validator\n  ref: ${revision}\n`)),
    /wrong TJSV repository/,
  );
});

test('runtime constant drift fails', () => {
  const lock = baseLock();
  const map = fileMapFor(lock);
  map['scripts/fixture.mjs'] = "const schema = 'fixture.report/v1';\nconst TJSV_REVISION = '2'.repeat(40);\n";
  expectFinding(auditFileMap(lock, map), /does not contain locked|undeclared/);
});

test('evidence schema drift fails independently of the validator pin', () => {
  const lock = baseLock();
  const map = fileMapFor(lock);
  map['scripts/fixture.mjs'] = `const schema = 'fixture.report/v2';\nconst TJSV_REVISION = '${lock.profiles[0].revision}';\n`;
  expectFinding(auditFileMap(lock, map), /no longer contains locked evidence schema/);
});

test('an undeclared current reference cannot masquerade as historical evidence', () => {
  const lock = baseLock();
  const map = fileMapFor(lock);
  map['docs/current.md'] = `current validator: ${lock.profiles[0].revision}\n`;
  expectFinding(auditFileMap(lock, map), /undeclared current TJSV revision/);
});

test('a second profile cannot claim the same consumer path', () => {
  const lock = baseLock();
  lock.profiles.push({
    id: 'duplicate-owner',
    revision: '2'.repeat(40),
    assuranceProfile: 'other',
    pinReferences: ['.github/workflows/fixture.yml'],
    evidenceSchemaChecks: [],
  });
  lock.selfDigest = lockDigest(lock);
  expectFinding(auditFileMap(lock, fileMapFor(lock)), /assigned to multiple TJSV profiles/);
});

test('lock repository identity itself is fail-closed', () => {
  const lock = baseLock();
  lock.repository = 'attacker/typespec-json-schema-validator';
  lock.selfDigest = lockDigest(lock);
  expectFinding(auditFileMap(lock, fileMapFor(lock)), /wrong TJSV repository/);
});
