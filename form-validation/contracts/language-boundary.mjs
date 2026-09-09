import { requireThat } from './corpus.mjs';

const HEX_40 = /^[a-f0-9]{40}$/u;
const HEX_64 = /^[a-f0-9]{64}$/u;

export const BOUNDARY_TARGETS = Object.freeze([
  Object.freeze({
    id: 'rust-native',
    language: 'rust',
    runtime: 'native',
    evidence: 'runtime/rust-native.json',
  }),
  Object.freeze({
    id: 'dart-vm',
    language: 'dart',
    runtime: 'vm',
    evidence: 'runtime/dart-vm.json',
  }),
  Object.freeze({
    id: 'dart-js',
    language: 'dart',
    runtime: 'javascript-node',
    evidence: 'runtime/dart-js.json',
  }),
]);

const identity = (label, value) => {
  requireThat(value !== null && typeof value === 'object' && !Array.isArray(value), `missing ${label} identity`);
  requireThat(typeof value.name === 'string' && value.name.trim() === value.name && value.name.length > 0 && value.name.length <= 256, `invalid ${label} name`);
  requireThat(typeof value.version === 'string' && value.version.trim() === value.version && value.version.length > 0 && value.version.length <= 256, `invalid ${label} version`);
  return Object.freeze({ name: value.name, version: value.version });
};

export function buildBoundaryManifest(boundary) {
  requireThat(typeof boundary?.LANGUAGE_BOUNDARY_MANIFEST_SCHEMA === 'string', 'missing upstream boundary manifest schema');
  return Object.freeze({
    schema: boundary.LANGUAGE_BOUNDARY_MANIFEST_SCHEMA,
    minimumDistinctLanguages: 2,
    authorities: Object.freeze({
      typeSpec: 'peer',
      jsonSchema: 'peer',
      generatedWitness: 'evidence_only',
    }),
    targets: Object.freeze(BOUNDARY_TARGETS.map(target => Object.freeze({
      language: target.language,
      runtime: target.runtime,
      required: true,
      ingress: true,
      egress: true,
      evidence: target.evidence,
    }))),
  });
}

export function buildBoundaryEvidence({
  boundary,
  sourceRevision,
  parityRunId,
  contractIrId,
  observationDigests,
  identities,
}) {
  requireThat(typeof boundary?.LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA === 'string', 'missing upstream boundary evidence schema');
  requireThat(HEX_40.test(sourceRevision ?? ''), 'source revision must be an immutable full SHA');
  requireThat(HEX_64.test(parityRunId ?? ''), 'parity run id must be a lowercase SHA-256 digest');
  requireThat(HEX_64.test(contractIrId ?? ''), 'Contract IR id must be a lowercase SHA-256 digest');
  requireThat(observationDigests !== null && typeof observationDigests === 'object' && !Array.isArray(observationDigests), 'missing observation digests');
  requireThat(identities !== null && typeof identities === 'object' && !Array.isArray(identities), 'missing runtime identities');

  const evidence = {};
  for (const target of BOUNDARY_TARGETS) {
    const observationDigest = observationDigests[target.id];
    requireThat(HEX_64.test(observationDigest ?? ''), `missing or invalid observation digest for ${target.id}`);
    const runtimeIdentity = identities[target.id];
    requireThat(runtimeIdentity !== null && typeof runtimeIdentity === 'object' && !Array.isArray(runtimeIdentity), `missing identity for ${target.id}`);
    evidence[target.evidence] = Object.freeze({
      schema: boundary.LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA,
      language: target.language,
      runtime: target.runtime,
      status: 'passed',
      sourceRevision,
      artifactDigest: `sha256:${observationDigest}`,
      receiptRunId: parityRunId,
      contractIrId,
      toolchain: identity(`${target.id} toolchain`, runtimeIdentity.toolchain),
      generator: identity(`${target.id} generator`, runtimeIdentity.generator),
      validation: Object.freeze({ ingress: 'passed', egress: 'passed' }),
    });
  }
  return Object.freeze(evidence);
}
