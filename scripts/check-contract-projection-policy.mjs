#!/usr/bin/env node

import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const policyPath = new URL('../governance/contract-projection-policy.json', import.meta.url);
const policy = JSON.parse(await readFile(policyPath, 'utf8'));

assert.equal(policy.schemaVersion, 1, 'unsupported projection policy schemaVersion');
assert.deepEqual(
  policy.authoredAuthorities.map(({ kind }) => kind).sort(),
  ['json-schema-2020-12', 'typespec'],
  'TypeSpec and authored JSON Schema Draft 2020-12 must be the two peer authorities',
);
assert.ok(
  policy.authoredAuthorities.every(({ role, independent }) => role === 'peer-authority' && independent === true),
  'both authored authorities must remain independent peer authorities',
);

const requiredEvidenceKinds = ['generated-json-schema', 'openapi', 'protobuf', 'grpc', 'wit', 'dafny', 'contract-ir'];
for (const kind of requiredEvidenceKinds) {
  assert.ok(policy.derivedEvidence.includes(kind), `missing derived evidence kind: ${kind}`);
}
assert.equal(new Set(policy.derivedEvidence).size, policy.derivedEvidence.length, 'derived evidence kinds must be unique');

const requiredStaticEvidenceKinds = ['contract-ir', 'protobuf', 'grpc', 'wit', 'dafny'];
assert.ok(policy.staticEvidence && typeof policy.staticEvidence === 'object', 'staticEvidence registry is required');
assert.deepEqual(Object.keys(policy.staticEvidence).sort(), [...requiredStaticEvidenceKinds].sort(), 'staticEvidence registry drift');
for (const kind of requiredStaticEvidenceKinds) {
  const paths = policy.staticEvidence[kind];
  assert.ok(Array.isArray(paths) && paths.length > 0, `${kind} static evidence paths are required`);
  assert.equal(new Set(paths).size, paths.length, `${kind} static evidence paths must be unique`);
  for (const relative of paths) {
    assert.equal(typeof relative, 'string', `${kind} evidence path must be a string`);
    assert.ok(relative.length > 0, `${kind} evidence path must not be empty`);
    assert.equal(path.isAbsolute(relative), false, `${kind} evidence path must be repository-relative`);
    assert.equal(relative.split('/').includes('..'), false, `${kind} evidence path must not traverse parents`);
    const info = await stat(path.join(root, relative));
    assert.ok(info.isFile(), `${kind} evidence path is not a file: ${relative}`);
  }
}

for (const relative of policy.staticEvidence.wit) {
  assert.ok(relative.startsWith('generated/'), 'WIT evidence must stay derived/generated');
}
for (const relative of policy.staticEvidence.dafny) {
  assert.ok(relative.startsWith('generated/'), 'Dafny evidence must stay derived/generated');
}

assert.equal(policy.invariants.generatedArtifactsMayNotReplaceAuthoredAuthority, true);
assert.equal(policy.invariants.peerAuthoritiesMustBeComparedSemantically, true);
assert.equal(policy.invariants.projectionDisagreementIsBlockingWhenProjectionIsRequired, true);
assert.equal(policy.invariants.missingRequiredEvidenceIsBlocking, true);
assert.equal(policy.invariants.zeroStepEvidenceIsNotAPass, true);

console.log('contract projection policy: ok');
