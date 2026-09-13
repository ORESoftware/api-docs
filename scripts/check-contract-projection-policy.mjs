#!/usr/bin/env node

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const path = new URL('../governance/contract-projection-policy.json', import.meta.url);
const policy = JSON.parse(await readFile(path, 'utf8'));

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
assert.equal(policy.invariants.generatedArtifactsMayNotReplaceAuthoredAuthority, true);
assert.equal(policy.invariants.peerAuthoritiesMustBeComparedSemantically, true);
assert.equal(policy.invariants.projectionDisagreementIsBlockingWhenProjectionIsRequired, true);
assert.equal(policy.invariants.missingRequiredEvidenceIsBlocking, true);
assert.equal(policy.invariants.zeroStepEvidenceIsNotAPass, true);

console.log('contract projection policy: ok');
