import { createHash } from 'node:crypto';

const sha256 = value => createHash('sha256').update(value).digest('hex');
const requireThat = (condition, message) => { if (!condition) throw new Error(message); };
const isPlainObject = value => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
};
const canonicalToken = value => typeof value === 'string'
  && value.length > 0
  && value.length <= 256
  && value === value.trim()
  && !/[\u0000-\u001f\u007f]/u.test(value);
const requireExactOwnKeys = (value, expected, label) => {
  requireThat(isPlainObject(value), `${label} must be a plain object`);
  requireThat(Object.getOwnPropertySymbols(value).length === 0, `${label} must not contain symbol keys`);
  const actual = Object.keys(value).sort();
  const required = [...expected].sort();
  requireThat(actual.length === required.length && actual.every((key, index) => key === required[index]), `${label} inventory mismatch`);
};

export const BOUNDARY_TARGETS = Object.freeze([
  Object.freeze({ language: 'rust', runtime: 'native', required: true, ingress: true, egress: true, evidence: 'runtime/rust-native.json' }),
  Object.freeze({ language: 'dart', runtime: 'vm', required: true, ingress: true, egress: true, evidence: 'runtime/dart-vm.json' }),
  Object.freeze({ language: 'dart', runtime: 'javascript-node', required: true, ingress: true, egress: true, evidence: 'runtime/dart-javascript.json' }),
  Object.freeze({ language: 'typescript', runtime: 'node-zod', required: true, ingress: true, egress: true, evidence: 'runtime/typescript-zod.json' }),
]);

export function buildBoundaryManifest(boundary) {
  requireThat(typeof boundary?.LANGUAGE_BOUNDARY_MANIFEST_SCHEMA === 'string', 'missing TJSV language boundary schema');
  return {
    schema: boundary.LANGUAGE_BOUNDARY_MANIFEST_SCHEMA,
    minimumDistinctLanguages: 3,
    authorities: {
      typeSpec: 'peer',
      jsonSchema: 'peer',
      generatedWitness: 'evidence_only',
    },
    targets: BOUNDARY_TARGETS.map(target => ({ ...target })),
  };
}

export function buildBoundaryEvidence({ boundary, sourceRevision, parityRunId, contractIrId, outputs, identities }) {
  requireThat(typeof boundary?.LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA === 'string', 'missing TJSV language boundary evidence schema');
  requireThat(typeof sourceRevision === 'string' && /^[a-f0-9]{40}$/u.test(sourceRevision), 'boundary source revision must be an immutable commit');
  requireThat(typeof parityRunId === 'string' && /^[a-f0-9]{64}$/u.test(parityRunId), 'boundary parity runId must be a SHA-256 digest');
  requireThat(typeof contractIrId === 'string' && /^[a-f0-9]{64}$/u.test(contractIrId), 'boundary Contract IR id must be a SHA-256 digest');

  const runtimeFor = Object.freeze({
    'rust-native': ['rust', 'native'],
    'dart-vm': ['dart', 'vm'],
    'dart-javascript': ['dart', 'javascript-node'],
    'typescript-zod': ['typescript', 'node-zod'],
  });
  const runtimeNames = Object.keys(runtimeFor);
  requireExactOwnKeys(outputs, runtimeNames, 'boundary outputs');
  requireExactOwnKeys(identities, runtimeNames, 'boundary identities');

  const pathFor = Object.fromEntries(BOUNDARY_TARGETS.map(target => {
    const entry = Object.entries(runtimeFor).find(([, value]) => value[0] === target.language && value[1] === target.runtime);
    requireThat(entry !== undefined, `missing runtime mapping for ${target.language}/${target.runtime}`);
    return [entry[0], target.evidence];
  }));

  const evidenceByPath = {};
  for (const runtime of runtimeNames) {
    const output = outputs[runtime];
    const identity = identities[runtime];
    requireThat(typeof output === 'string' && output.length > 0, `missing runtime output: ${runtime}`);
    requireExactOwnKeys(identity, ['toolchain', 'generator'], `runtime identity: ${runtime}`);
    requireExactOwnKeys(identity.toolchain, ['name', 'version'], `toolchain identity: ${runtime}`);
    requireExactOwnKeys(identity.generator, ['name', 'version'], `generator identity: ${runtime}`);
    requireThat(canonicalToken(identity.toolchain.name) && canonicalToken(identity.toolchain.version), `invalid toolchain identity: ${runtime}`);
    requireThat(canonicalToken(identity.generator.name) && canonicalToken(identity.generator.version), `invalid generator identity: ${runtime}`);
    const [language, executionRuntime] = runtimeFor[runtime];
    evidenceByPath[pathFor[runtime]] = {
      schema: boundary.LANGUAGE_BOUNDARY_EVIDENCE_SCHEMA,
      language,
      runtime: executionRuntime,
      status: 'passed',
      sourceRevision,
      artifactDigest: `sha256:${sha256(output)}`,
      receiptRunId: parityRunId,
      contractIrId,
      toolchain: { ...identity.toolchain },
      generator: { ...identity.generator },
      validation: { ingress: 'passed', egress: 'passed' },
    };
  }
  return evidenceByPath;
}
