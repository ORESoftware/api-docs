import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const LOCK_PATH = 'contracts/tjsv-consumer.lock.json';
export const LOCK_SCHEMA = 'ores.tjsv-consumer-lock/v1';
export const EXPECTED_REPOSITORY = 'ORESoftware/typespec-json-schema-validator';
const ROOT = fileURLToPath(new URL('../', import.meta.url));
const SHA40 = /^[0-9a-f]{40}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const SAFE_PATH = /^(?!\/)(?!.*(?:^|\/)\.\.(?:\/|$))[A-Za-z0-9._/@+-]+(?:\/[A-Za-z0-9._/@+-]+)*$/;
const CONTROL_PATHS = new Set([
  LOCK_PATH,
  'scripts/check-tjsv-consumer-lock.mjs',
  'scripts/test-tjsv-consumer-lock.mjs',
]);

function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (object(value)) {
    return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

export function lockDigest(lock) {
  const copy = structuredClone(lock);
  delete copy.selfDigest;
  return createHash('sha256').update(canonicalJson(copy)).digest('hex');
}

function exactKeys(value, expected, where) {
  requireThat(object(value), `${where} must be an object`);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  requireThat(JSON.stringify(actual) === JSON.stringify(wanted), `${where} keys differ: ${actual.join(',')}`);
}

function uniqueStrings(values, where) {
  requireThat(Array.isArray(values) && values.length > 0, `${where} must be a non-empty array`);
  requireThat(values.every(value => typeof value === 'string' && value.length > 0), `${where} must contain non-empty strings`);
  requireThat(new Set(values).size === values.length, `${where} contains duplicates`);
}

function safeReference(path, where) {
  requireThat(typeof path === 'string' && SAFE_PATH.test(path), `${where} is not a safe repository-relative path: ${path}`);
}

export function validateLockFileInventory(paths) {
  const locks = [...paths].sort();
  requireThat(locks.length === 1 && locks[0] === LOCK_PATH,
    `expected exactly one canonical TJSV consumer lock, found: ${locks.join(',') || '<none>'}`);
  return true;
}

export function validateLock(lock) {
  exactKeys(lock, [
    'schema', 'repository', 'sourceRevisionBinding', 'compatibilityPolicy', 'profiles',
    'scanPolicy', 'selfDigestAlgorithm', 'selfDigest',
  ], 'lock');
  requireThat(lock.schema === LOCK_SCHEMA, `unsupported lock schema: ${lock.schema}`);
  requireThat(lock.repository === EXPECTED_REPOSITORY, 'lock points at the wrong TJSV repository');
  requireThat(lock.sourceRevisionBinding === 'runtime-git-head', 'source revision must be bound at execution time');
  requireThat(lock.selfDigestAlgorithm === 'sha256-sorted-json-v1', 'unsupported self-digest algorithm');
  requireThat(SHA256.test(lock.selfDigest), 'lock selfDigest must be lowercase SHA-256');
  requireThat(lockDigest(lock) === lock.selfDigest, 'lock selfDigest mismatch');

  exactKeys(lock.compatibilityPolicy, [
    'inferenceFromGitAncestryAllowed', 'policyIssue', 'upgradeAutomationIssue',
  ], 'compatibilityPolicy');
  requireThat(lock.compatibilityPolicy.inferenceFromGitAncestryAllowed === false,
    'TJSV compatibility must not be inferred from Git ancestry');
  for (const field of ['policyIssue', 'upgradeAutomationIssue']) {
    requireThat(typeof lock.compatibilityPolicy[field] === 'string' &&
      /^https:\/\/github\.com\/ORESoftware\/\.github\/issues\/[1-9][0-9]*$/.test(lock.compatibilityPolicy[field]),
    `invalid compatibilityPolicy.${field}`);
  }

  exactKeys(lock.scanPolicy, [
    'currentReferenceRoots', 'allowMutableRefs', 'allowShortShas',
    'allowUndeclaredCurrentReferences', 'allowWrongRepository',
  ], 'scanPolicy');
  uniqueStrings(lock.scanPolicy.currentReferenceRoots, 'scanPolicy.currentReferenceRoots');
  for (const root of lock.scanPolicy.currentReferenceRoots) safeReference(root, 'scan root');
  for (const field of ['allowMutableRefs', 'allowShortShas', 'allowUndeclaredCurrentReferences', 'allowWrongRepository']) {
    requireThat(lock.scanPolicy[field] === false, `${field} must remain fail-closed`);
  }

  requireThat(Array.isArray(lock.profiles) && lock.profiles.length > 0, 'profiles must be non-empty');
  const ids = new Set();
  const pinOwners = new Map();
  for (const [index, profile] of lock.profiles.entries()) {
    const where = `profiles[${index}]`;
    exactKeys(profile, ['id', 'revision', 'assuranceProfile', 'pinReferences', 'evidenceSchemaChecks'], where);
    requireThat(typeof profile.id === 'string' && /^[a-z][a-z0-9-]*$/.test(profile.id), `${where}.id invalid`);
    requireThat(!ids.has(profile.id), `duplicate profile id: ${profile.id}`);
    ids.add(profile.id);
    requireThat(SHA40.test(profile.revision), `${profile.id} revision must be immutable lowercase 40-char SHA`);
    requireThat(typeof profile.assuranceProfile === 'string' &&
      profile.assuranceProfile.trim() === profile.assuranceProfile && profile.assuranceProfile.length > 0,
    `${profile.id} assuranceProfile invalid`);
    uniqueStrings(profile.pinReferences, `${profile.id}.pinReferences`);
    for (const path of profile.pinReferences) {
      safeReference(path, `${profile.id} pin reference`);
      requireThat(!pinOwners.has(path), `${path} is assigned to multiple TJSV profiles`);
      pinOwners.set(path, profile);
    }
    requireThat(Array.isArray(profile.evidenceSchemaChecks), `${profile.id}.evidenceSchemaChecks must be an array`);
    const schemas = new Set();
    for (const [schemaIndex, check] of profile.evidenceSchemaChecks.entries()) {
      exactKeys(check, ['schema', 'references'], `${profile.id}.evidenceSchemaChecks[${schemaIndex}]`);
      requireThat(typeof check.schema === 'string' && /^[a-z0-9.-]+(?:\/[A-Za-z0-9._-]+)+$/.test(check.schema),
        `${profile.id} evidence schema invalid: ${check.schema}`);
      requireThat(!schemas.has(check.schema), `${profile.id} duplicate evidence schema: ${check.schema}`);
      schemas.add(check.schema);
      uniqueStrings(check.references, `${profile.id} ${check.schema} references`);
      for (const path of check.references) safeReference(path, `${profile.id} evidence schema reference`);
    }
  }
  return { pinOwners };
}

function workflowTjsvCheckouts(content) {
  const results = [];
  const repositoryPattern = /repository:\s*([^\s#]*typespec-json-schema-validator)\s*(?:#.*)?(?:\r?\n)([\s\S]{0,700}?)(?=\n\s*-\s+(?:name:|uses:|run:)|\n\s{0,6}[A-Za-z][A-Za-z0-9_-]*:|$)/g;
  for (const match of content.matchAll(repositoryPattern)) {
    const body = match[2];
    const ref = body.match(/(?:^|\n)\s*ref:\s*([^\s#]+)/)?.[1] ?? null;
    results.push({ repository: match[1], ref });
  }
  return results;
}

function currentLiteralCandidates(content) {
  const found = [];
  const constantPattern = /^\s*(?:export\s+)?const\s+TJSV_REVISION\s*=\s*['"]([^'"]+)['"]\s*;?\s*$/gm;
  for (const match of content.matchAll(constantPattern)) found.push(match[1]);
  const nearbyPattern = /typespec-json-schema-validator[\s\S]{0,260}?\b([0-9a-f]{7,40}|main|master|latest)\b/g;
  for (const match of content.matchAll(nearbyPattern)) found.push(match[1]);
  return [...new Set(found)];
}

export function auditFileMap(lock, fileMap) {
  const findings = [];
  let validated;
  try {
    validated = validateLock(lock);
  } catch (error) {
    return { status: 'failed', findings: [error instanceof Error ? error.message : String(error)] };
  }
  const { pinOwners } = validated;
  const knownRevisions = new Map(lock.profiles.map(profile => [profile.revision, profile]));

  for (const profile of lock.profiles) {
    for (const path of profile.pinReferences) {
      const content = fileMap[path];
      if (typeof content !== 'string') {
        findings.push(`declared pin reference is missing or non-text: ${path}`);
        continue;
      }
      if (!content.includes(profile.revision)) findings.push(`${path} does not contain locked ${profile.id} revision ${profile.revision}`);
      if (path.startsWith('.github/workflows/')) {
        const checkouts = workflowTjsvCheckouts(content);
        if (checkouts.length === 0) findings.push(`${path} declares a TJSV workflow pin but no TJSV checkout was parsed`);
        for (const checkout of checkouts) {
          if (checkout.repository !== EXPECTED_REPOSITORY) findings.push(`${path} uses wrong TJSV repository ${checkout.repository}`);
          if (checkout.ref !== profile.revision) findings.push(`${path} TJSV checkout ref ${checkout.ref ?? '<missing>'} differs from ${profile.revision}`);
        }
      }
    }
    for (const check of profile.evidenceSchemaChecks) {
      for (const path of check.references) {
        const content = fileMap[path];
        if (typeof content !== 'string') findings.push(`evidence schema reference is missing or non-text: ${path}`);
        else if (!content.includes(check.schema)) findings.push(`${path} no longer contains locked evidence schema ${check.schema}`);
      }
    }
  }

  for (const [path, content] of Object.entries(fileMap)) {
    if (CONTROL_PATHS.has(path) || typeof content !== 'string') continue;
    for (const [revision, profile] of knownRevisions) {
      if (content.includes(revision) && pinOwners.get(path)?.id !== profile.id) {
        findings.push(`${path} contains undeclared current TJSV revision ${revision} (${profile.id})`);
      }
    }
    for (const checkout of workflowTjsvCheckouts(content)) {
      if (checkout.repository !== EXPECTED_REPOSITORY) findings.push(`${path} uses wrong TJSV repository ${checkout.repository}`);
      if (checkout.ref === null) findings.push(`${path} TJSV checkout is missing ref`);
      else if (!SHA40.test(checkout.ref)) findings.push(`${path} TJSV checkout uses mutable or shortened ref ${checkout.ref}`);
      else if (pinOwners.get(path)?.revision !== checkout.ref) findings.push(`${path} TJSV checkout ${checkout.ref} is not declared by the consumer lock`);
    }
    for (const candidate of currentLiteralCandidates(content)) {
      if (!SHA40.test(candidate)) findings.push(`${path} contains mutable or shortened TJSV revision ${candidate}`);
      else if (pinOwners.get(path)?.revision !== candidate) findings.push(`${path} contains undeclared TJSV revision ${candidate}`);
    }
  }

  return { status: findings.length === 0 ? 'passed' : 'failed', findings: [...new Set(findings)].sort() };
}

function git(...args) {
  const env = { ...process.env, GIT_NO_REPLACE_OBJECTS: '1' };
  for (const key of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES']) delete env[key];
  return execFileSync('git', ['-C', ROOT, ...args], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], env, timeout: 30000, maxBuffer: 16 * 1024 * 1024,
  }).trim();
}

async function trackedTextMap(lock) {
  const roots = lock.scanPolicy.currentReferenceRoots;
  const paths = git('ls-files', '-z', '--', LOCK_PATH, ...roots).split('\0').filter(Boolean);
  const locks = git('ls-files', '-z', '--', '*tjsv-consumer.lock.json').split('\0').filter(Boolean);
  validateLockFileInventory(locks);
  const fileMap = {};
  for (const path of paths) {
    const bytes = await readFile(resolve(ROOT, path));
    if (bytes.length > 2 * 1024 * 1024 || bytes.includes(0)) continue;
    fileMap[path] = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  }
  requireThat(typeof fileMap[LOCK_PATH] === 'string', 'canonical TJSV consumer lock is not tracked text');
  return fileMap;
}

export async function main() {
  const lock = JSON.parse(await readFile(resolve(ROOT, LOCK_PATH), 'utf8'));
  const fileMap = await trackedTextMap(lock);
  const audit = auditFileMap(lock, fileMap);
  const report = {
    schema: 'ores.api-docs.tjsv-consumer-lock-report/v1',
    status: audit.status,
    sourceRevision: git('rev-parse', 'HEAD'),
    lockPath: LOCK_PATH,
    lockDigest: lock.selfDigest,
    validatorRepository: lock.repository,
    profiles: lock.profiles.map(profile => ({
      id: profile.id,
      revision: profile.revision,
      assuranceProfile: profile.assuranceProfile,
      pinReferenceCount: profile.pinReferences.length,
      evidenceSchemaCount: profile.evidenceSchemaChecks.length,
    })),
    compatibilityInferredFromGitAncestry: false,
    findings: audit.findings,
  };
  await mkdir(resolve(ROOT, 'tmp'), { recursive: true });
  await writeFile(resolve(ROOT, 'tmp/tjsv-consumer-lock-report.json'), `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600 });
  console.log(JSON.stringify(report));
  return audit.status === 'passed' ? 0 : 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().then(code => { process.exitCode = code; }).catch(error => {
    console.error(JSON.stringify({ status: 'failed', error: error instanceof Error ? error.message : String(error) }));
    process.exitCode = 3;
  });
}
