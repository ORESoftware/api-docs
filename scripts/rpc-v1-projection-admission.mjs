#!/usr/bin/env node
import { execFile as execFileCallback } from 'node:child_process';
import { access, lstat, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, relative, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

export const POLICY_SCHEMA = 'ores.api-docs.rpc-v1-projection-admission-policy/v1';
export const POLICY_AUDIT_SCHEMA = 'ores.api-docs.rpc-v1-projection-admission-policy-audit/v1';
export const DELTA_APPROVAL_SCHEMA = 'ores.api-docs.projection-delta-approvals/v1';
export const RUNTIME_EVIDENCE_SCHEMA = 'ores.api-docs.projection-runtime-validator-evidence/v1';
const MANIFEST_SCHEMA = 'ores.typespec-json-schema-validator.projection-manifest/v1';
const REPORT_SCHEMA = 'ores.typespec-json-schema-validator.projection-admission-report/v1';
const MAX_JSON_BYTES = 8 * 1024 * 1024;
const HEX_40 = /^[a-f0-9]{40}$/u;
const IDENTIFIER = /^[a-z0-9](?:[a-z0-9._-]{0,126}[a-z0-9])?$/u;
const execFile = promisify(execFileCallback);

function isPlainObject(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (!isPlainObject(value)) return value;
  return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonicalize(value[key])]));
}

function stableJson(value) {
  return `${JSON.stringify(canonicalize(value), null, 2)}\n`;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function exactKeys(value, allowed, label) {
  assert(isPlainObject(value), `${label} must be an object`);
  const unknown = Object.keys(value).filter((key) => !allowed.has(key));
  assert(unknown.length === 0, `${label} contains unsupported properties`);
}

function validText(value, max = 2048) {
  return typeof value === 'string'
    && value.length >= 1
    && value.length <= max
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function validIdentifier(value) {
  return validText(value, 128) && IDENTIFIER.test(value);
}

function validRelativePath(value) {
  if (!validText(value, 512) || value.startsWith('/') || value.includes('\\')) return false;
  const segments = value.split('/');
  return !segments.some((segment) => segment === '' || segment === '.' || segment === '..');
}

function inside(root, candidate) {
  const rendered = relative(root, candidate);
  return rendered !== '' && rendered !== '..' && !rendered.startsWith(`..${sep}`);
}

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function readSafeBytes(rootPath, relativePath, maxBytes = MAX_JSON_BYTES) {
  assert(validRelativePath(relativePath), 'evidence path must be a normalized relative POSIX path');
  const root = resolve(rootPath);
  const path = resolve(root, relativePath);
  assert(inside(root, path), 'evidence path escapes the configured root');
  const stat = await lstat(path);
  assert(stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1, 'evidence path must be a singly linked regular file');
  assert(stat.size <= maxBytes, 'evidence file exceeds the configured byte limit');
  const bytes = await readFile(path);
  assert(bytes.length === stat.size, 'evidence file changed while it was read');
  return bytes;
}

async function readSafeJson(rootPath, relativePath) {
  const bytes = await readSafeBytes(rootPath, relativePath);
  try {
    return JSON.parse(bytes.toString('utf8'));
  } catch {
    throw new Error('evidence file must contain valid UTF-8 JSON');
  }
}

function normalizeStringArray(value, label, { identifiers = false, allowEmpty = true } = {}) {
  assert(Array.isArray(value), `${label} must be an array`);
  assert(allowEmpty || value.length > 0, `${label} must not be empty`);
  const seen = new Set();
  const result = [];
  for (const item of value) {
    assert(identifiers ? validIdentifier(item) : validRelativePath(item), `${label} contains an invalid entry`);
    assert(!seen.has(item), `${label} contains a duplicate entry`);
    seen.add(item);
    result.push(item);
  }
  return result.sort((left, right) => left.localeCompare(right));
}

export function normalizeProjectionAdmissionPolicy(value) {
  exactKeys(value, new Set([
    'schema', 'validator', 'activation', 'inputs', 'outputs', 'projections',
    'toolchains', 'approvedDeltas', 'runtimeValidators',
  ]), 'projection admission policy');
  assert(value.schema === POLICY_SCHEMA, `policy schema must be ${POLICY_SCHEMA}`);

  exactKeys(value.validator, new Set(['repository', 'revision', 'manifestSchema']), 'validator policy');
  assert(value.validator.repository === 'ORESoftware/typespec-json-schema-validator', 'validator repository is unsupported');
  assert(HEX_40.test(value.validator.revision), 'validator revision must be an exact 40-character commit SHA');
  assert(value.validator.manifestSchema === MANIFEST_SCHEMA, 'validator manifest schema is unsupported');

  exactKeys(value.activation, new Set(['status', 'manifestPath', 'reportPath', 'blockers']), 'activation policy');
  assert(['blocked', 'enabled'].includes(value.activation.status), 'activation status must be blocked or enabled');
  assert(validRelativePath(value.activation.manifestPath), 'activation manifestPath is invalid');
  assert(validRelativePath(value.activation.reportPath), 'activation reportPath is invalid');
  assert(Array.isArray(value.activation.blockers), 'activation blockers must be an array');
  const blockerIds = new Set();
  const blockers = value.activation.blockers.map((blocker) => {
    exactKeys(blocker, new Set(['id', 'reason']), 'activation blocker');
    assert(validIdentifier(blocker.id), 'activation blocker id is invalid');
    assert(validText(blocker.reason), 'activation blocker reason is invalid');
    assert(!blockerIds.has(blocker.id), 'activation blocker id is duplicated');
    blockerIds.add(blocker.id);
    return Object.freeze({ id: blocker.id, reason: blocker.reason });
  }).sort((left, right) => left.id.localeCompare(right.id));
  if (value.activation.status === 'blocked') {
    assert(blockers.length > 0, 'blocked activation must name at least one blocker');
  } else {
    assert(blockers.length === 0, 'enabled activation must not retain blockers');
  }

  exactKeys(value.inputs, new Set(['operationInventory', 'projectionMetadata', 'emitterConfiguration']), 'projection inputs');
  const inputs = {};
  for (const key of ['operationInventory', 'projectionMetadata', 'emitterConfiguration']) {
    assert(validRelativePath(value.inputs[key]), `projection input ${key} path is invalid`);
    inputs[key] = value.inputs[key];
  }

  assert(Array.isArray(value.outputs) && value.outputs.length > 0, 'projection outputs must be a non-empty array');
  const outputPaths = new Set();
  const outputs = value.outputs.map((output) => {
    exactKeys(output, new Set(['path', 'mediaType', 'projection']), 'projection output');
    assert(validRelativePath(output.path), 'projection output path is invalid');
    assert(validText(output.mediaType, 255) && /^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}\/[a-z0-9][a-z0-9!#$&^_.+-]{0,126}$/u.test(output.mediaType), 'projection output mediaType is invalid');
    assert(validIdentifier(output.projection), 'projection output owner is invalid');
    assert(!outputPaths.has(output.path), 'projection output path is duplicated');
    outputPaths.add(output.path);
    return Object.freeze({ path: output.path, mediaType: output.mediaType, projection: output.projection });
  }).sort((left, right) => left.path.localeCompare(right.path));

  assert(Array.isArray(value.projections) && value.projections.length > 0, 'projections must be a non-empty array');
  const projectionIds = new Set();
  const projections = value.projections.map((projection) => {
    exactKeys(projection, new Set([
      'id', 'emitter', 'declarationMode', 'representationDeltaIds', 'runtimeValidatorIds',
    ]), 'projection target');
    assert(validIdentifier(projection.id), 'projection id is invalid');
    assert(validIdentifier(projection.emitter), 'projection emitter is invalid');
    assert(projection.declarationMode === 'all', 'projection declarationMode must be all');
    assert(!projectionIds.has(projection.id), 'projection id is duplicated');
    projectionIds.add(projection.id);
    return Object.freeze({
      id: projection.id,
      emitter: projection.emitter,
      declarationMode: 'all',
      representationDeltaIds: Object.freeze(normalizeStringArray(
        projection.representationDeltaIds,
        'representationDeltaIds',
        { identifiers: true },
      )),
      runtimeValidatorIds: Object.freeze(normalizeStringArray(
        projection.runtimeValidatorIds,
        'runtimeValidatorIds',
        { identifiers: true },
      )),
    });
  }).sort((left, right) => left.id.localeCompare(right.id));
  for (const output of outputs) {
    assert(projectionIds.has(output.projection), 'projection output references an undeclared projection');
  }

  assert(Array.isArray(value.toolchains) && value.toolchains.length > 0, 'toolchains must be a non-empty array');
  const toolchainIds = new Set();
  const toolchains = value.toolchains.map((toolchain) => {
    exactKeys(toolchain, new Set(['id', 'version', 'root', 'files']), 'projection toolchain');
    assert(validIdentifier(toolchain.id), 'toolchain id is invalid');
    assert(validText(toolchain.version, 256), 'toolchain version is invalid');
    assert(['repository', 'validator'].includes(toolchain.root), 'toolchain root is invalid');
    assert(!toolchainIds.has(toolchain.id), 'toolchain id is duplicated');
    toolchainIds.add(toolchain.id);
    return Object.freeze({
      id: toolchain.id,
      version: toolchain.version,
      root: toolchain.root,
      files: Object.freeze(normalizeStringArray(toolchain.files, 'toolchain files', { allowEmpty: false })),
    });
  }).sort((left, right) => left.id.localeCompare(right.id));

  assert(value.approvedDeltas === null || validRelativePath(value.approvedDeltas), 'approvedDeltas path is invalid');
  assert(value.runtimeValidators === null || validRelativePath(value.runtimeValidators), 'runtimeValidators path is invalid');

  return Object.freeze({
    schema: POLICY_SCHEMA,
    validator: Object.freeze({ ...value.validator }),
    activation: Object.freeze({
      status: value.activation.status,
      manifestPath: value.activation.manifestPath,
      reportPath: value.activation.reportPath,
      blockers: Object.freeze(blockers),
    }),
    inputs: Object.freeze(inputs),
    outputs: Object.freeze(outputs),
    projections: Object.freeze(projections),
    toolchains: Object.freeze(toolchains),
    approvedDeltas: value.approvedDeltas,
    runtimeValidators: value.runtimeValidators,
  });
}

function policyFinding(ruleId, pointer, message) {
  return Object.freeze({ ruleId, pointer, message, severity: 'error' });
}

async function inspectOptionalEvidence(root, path, schema, status, findings, pointer) {
  if (path === null) return null;
  try {
    const value = await readSafeJson(root, path);
    if (!isPlainObject(value) || value.schema !== schema || value.status !== status) {
      findings.push(policyFinding('projection-policy-evidence-state-mismatch', pointer, 'evidence document schema or status does not match activation state'));
    }
    return value;
  } catch {
    findings.push(policyFinding('projection-policy-evidence-unreadable', pointer, 'evidence document is missing or unsafe'));
    return null;
  }
}

export async function auditProjectionAdmissionPolicy({ root = process.cwd(), policyPath, validatorRoot = null }) {
  const findings = [];
  let policy = null;
  try {
    policy = normalizeProjectionAdmissionPolicy(await readSafeJson(root, policyPath));
  } catch (error) {
    findings.push(policyFinding('projection-policy-invalid', '#', error.message));
  }
  if (policy) {
    for (const [key, path] of Object.entries(policy.inputs)) {
      try {
        await readSafeBytes(root, path);
      } catch {
        findings.push(policyFinding('projection-policy-input-unreadable', `#/inputs/${key}`, 'configured input is missing or unsafe'));
      }
    }
    for (let index = 0; index < policy.outputs.length; index += 1) {
      try {
        await readSafeBytes(root, policy.outputs[index].path);
      } catch {
        findings.push(policyFinding('projection-policy-output-unreadable', `#/outputs/${index}`, 'configured output is missing or unsafe'));
      }
    }
    const evidenceStatus = policy.activation.status === 'blocked' ? 'blocked' : 'passed';
    await inspectOptionalEvidence(root, policy.approvedDeltas, DELTA_APPROVAL_SCHEMA, evidenceStatus, findings, '#/approvedDeltas');
    await inspectOptionalEvidence(root, policy.runtimeValidators, RUNTIME_EVIDENCE_SCHEMA, evidenceStatus, findings, '#/runtimeValidators');
    for (let index = 0; index < policy.toolchains.length; index += 1) {
      const toolchain = policy.toolchains[index];
      const base = toolchain.root === 'validator' ? validatorRoot : root;
      if (!base) {
        findings.push(policyFinding('projection-policy-validator-root-missing', `#/toolchains/${index}`, 'validator-root is required to audit pinned validator source'));
        continue;
      }
      for (const path of toolchain.files) {
        try {
          await readSafeBytes(base, path);
        } catch {
          findings.push(policyFinding('projection-policy-toolchain-unreadable', `#/toolchains/${index}`, 'configured toolchain source is missing or unsafe'));
        }
      }
    }
    if (validatorRoot) {
      try {
        await loadValidator(validatorRoot, policy.validator.revision);
      } catch {
        findings.push(policyFinding('projection-policy-validator-revision-mismatch', '#/validator/revision', 'validator checkout does not match the pinned revision'));
      }
    }
    if (policy.activation.status === 'blocked') {
      for (const [key, path] of [
        ['manifestPath', policy.activation.manifestPath],
        ['reportPath', policy.activation.reportPath],
      ]) {
        if (await exists(resolve(root, path))) {
          findings.push(policyFinding('projection-policy-stale-green-artifact', `#/activation/${key}`, 'blocked activation must not retain a generated admission artifact'));
        }
      }
    }
  }
  const status = findings.length === 0 ? 'passed' : 'failed';
  return Object.freeze({
    schema: POLICY_AUDIT_SCHEMA,
    status,
    policyValid: status === 'passed',
    projectionAdmissible: false,
    activationStatus: policy?.activation.status ?? 'invalid',
    blockerIds: Object.freeze(policy?.activation.blockers.map((item) => item.id) ?? []),
    findings: Object.freeze(findings),
  });
}

async function loadValidator(validatorRoot, expectedRevision) {
  const root = resolve(validatorRoot);
  let head;
  try {
    ({ stdout: head } = await execFile('git', ['-C', root, 'rev-parse', 'HEAD'], { encoding: 'utf8' }));
  } catch {
    throw new Error('validator checkout must be a readable Git worktree');
  }
  assert(head.trim() === expectedRevision, 'validator checkout does not match the pinned revision');
  const packageJson = await readSafeJson(root, 'package.json');
  assert(packageJson.name === '@oresoftware/typespec-json-schema-validator', 'validator package identity is invalid');
  const [validator, projection] = await Promise.all([
    import(pathToFileURL(resolve(root, 'src/index.mjs')).href),
    import(pathToFileURL(resolve(root, 'src/projection-admission/index.mjs')).href),
  ]);
  assert(projection.PROJECTION_MANIFEST_SCHEMA === MANIFEST_SCHEMA, 'validator projection manifest schema mismatch');
  assert(HEX_40.test(expectedRevision), 'validator revision is invalid');
  return { root, validator, projection };
}

async function hashOne(projection, root, path, extra = {}) {
  const [descriptor] = await projection.hashProjectionFiles(root, [{ path, ...extra }], { maxFiles: 1 });
  return descriptor;
}

async function buildToolchains({ policy, root, validatorRoot, validator, projection }) {
  const result = [];
  for (const toolchain of policy.toolchains) {
    const base = toolchain.root === 'validator' ? validatorRoot : root;
    const files = await projection.hashProjectionFiles(base, toolchain.files.map((path) => ({ path })));
    const artifactDigest = validator.sha256(validator.canonicalStringify(files.map(({ path, sha256, size }) => ({ path, sha256, size }))));
    result.push({ id: toolchain.id, version: toolchain.version, artifactDigest });
  }
  return result.sort((left, right) => left.id.localeCompare(right.id));
}

function requireEvidenceDocument(value, schema, label) {
  exactKeys(value, new Set(['schema', 'status', label]), `${label} evidence`);
  assert(value.schema === schema, `${label} evidence schema is unsupported`);
  assert(value.status === 'passed', `${label} evidence must be passed`);
  assert(Array.isArray(value[label]), `${label} evidence entries must be an array`);
  return value[label];
}

export async function buildProjectionAdmission({
  root = process.cwd(),
  validatorRoot,
  policyPath,
  contractIrPath,
  parityReceiptPath,
  typespecPath,
  generatedSchemaPath,
  authoredSchemaPath,
}) {
  const policy = normalizeProjectionAdmissionPolicy(await readSafeJson(root, policyPath));
  assert(policy.activation.status === 'enabled', 'projection admission policy is blocked');
  const loaded = await loadValidator(validatorRoot, policy.validator.revision);
  const contractIr = await readSafeJson(root, contractIrPath);
  const parityReceipt = await readSafeJson(root, parityReceiptPath);
  const contractVerification = await loaded.validator.verifyContractIr({
    contractIr,
    report: parityReceipt,
    typespec: resolve(root, typespecPath),
    generatedSchema: resolve(root, generatedSchemaPath),
    authoredSchema: resolve(root, authoredSchemaPath),
  });
  assert(contractVerification.admissible === true, 'Contract IR does not match the exact current authority closure');

  const expectedSourceDigests = {
    typespec: contractIr.provenance?.typespec?.digest,
    generatedJsonSchema: contractIr.provenance?.generatedJsonSchema?.digest,
    authoredJsonSchema: contractIr.provenance?.authoredJsonSchema?.digest,
  };
  const expectedInputs = {
    operationInventory: await hashOne(loaded.projection, root, policy.inputs.operationInventory),
    projectionMetadata: await hashOne(loaded.projection, root, policy.inputs.projectionMetadata),
    emitterConfiguration: await hashOne(loaded.projection, root, policy.inputs.emitterConfiguration),
  };
  const outputs = await loaded.projection.hashProjectionFiles(root, policy.outputs);
  const toolchains = await buildToolchains({
    policy,
    root,
    validatorRoot: loaded.root,
    validator: loaded.validator,
    projection: loaded.projection,
  });
  const declarationIds = contractIr.declarations.map((item) => item.id).sort((left, right) => left.localeCompare(right));
  const projections = policy.projections.map((item) => ({
    id: item.id,
    emitter: item.emitter,
    declarationIds,
    outputPaths: outputs.filter((output) => output.projection === item.id).map((output) => output.path),
    representationDeltaIds: item.representationDeltaIds,
    runtimeValidatorIds: item.runtimeValidatorIds,
  }));
  const deltaDocument = policy.approvedDeltas === null
    ? { schema: DELTA_APPROVAL_SCHEMA, status: 'passed', approvals: [] }
    : await readSafeJson(root, policy.approvedDeltas);
  const runtimeDocument = policy.runtimeValidators === null
    ? { schema: RUNTIME_EVIDENCE_SCHEMA, status: 'passed', validators: [] }
    : await readSafeJson(root, policy.runtimeValidators);
  const representationDeltas = requireEvidenceDocument(deltaDocument, DELTA_APPROVAL_SCHEMA, 'approvals');
  const runtimeValidators = requireEvidenceDocument(runtimeDocument, RUNTIME_EVIDENCE_SCHEMA, 'validators');
  const approvedDeltas = representationDeltas.map((item) => ({
    id: item.id,
    projection: item.projection,
    declaration: item.declaration,
    sourceDigest: item.sourceDigest,
    approvalDigest: item.review?.approvalDigest,
    negativeFixtureDigest: item.negativeFixtureDigest,
  }));

  const manifest = loaded.projection.createProjectionManifest({
    contractIr,
    parityReceipt,
    expectedSourceDigests,
    inputs: expectedInputs,
    toolchains,
    projections,
    outputs,
    representationDeltas,
    runtimeValidators,
  });
  const report = loaded.projection.verifyProjectionManifest({
    manifest,
    contractIr,
    parityReceipt,
    expectedSourceDigests,
    expectedInputs,
    requiredToolchains: toolchains,
    actualOutputs: outputs,
    requiredProjections: policy.projections.map((item) => item.id),
    approvedDeltas,
    expectedRuntimeValidators: runtimeValidators,
  });
  assert(report.admissible === true, 'projection admission verification did not pass');
  return Object.freeze({ policy, manifest, report, contractVerification });
}

async function safeOwnedWrite(rootPath, relativePath, value, ownedSchemas) {
  assert(validRelativePath(relativePath), 'output path must be a normalized relative POSIX path');
  const root = resolve(rootPath);
  const path = resolve(root, relativePath);
  assert(inside(root, path), 'output path escapes the configured root');
  if (await exists(path)) {
    const stat = await lstat(path);
    assert(stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1, 'output path must be a singly linked regular file');
    let current;
    try {
      current = JSON.parse(await readFile(path, 'utf8'));
    } catch {
      throw new Error('refusing to replace malformed output at the configured destination');
    }
    assert(ownedSchemas.has(current?.schema), 'refusing to replace an output not owned by this admission tool');
  }
  await mkdir(dirname(path), { recursive: true });
  const temporary = `${path}.tmp-${process.pid}`;
  await rm(temporary, { force: true });
  await writeFile(temporary, stableJson(value), { flag: 'wx', mode: 0o600 });
  await rename(temporary, path);
}

async function checkOwnedJson(root, path, expected) {
  const actual = await readSafeJson(root, path);
  assert(stableJson(actual) === stableJson(expected), `${path} is stale; regenerate projection admission evidence`);
}

function parseArgs(argv) {
  const result = {};
  for (const argument of argv) {
    const match = /^--([a-z][a-z0-9-]*)=(.*)$/u.exec(argument);
    assert(match, `invalid argument form: ${argument}`);
    assert(!Object.hasOwn(result, match[1]), `duplicate argument: --${match[1]}`);
    result[match[1]] = match[2];
  }
  return result;
}

async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const mode = args.mode;
  const root = resolve(args.root ?? process.cwd());
  const policyPath = args.policy ?? 'idl/rpc-v1-projection-admission.policy.json';
  if (mode === 'audit-policy') {
    const audit = await auditProjectionAdmissionPolicy({ root, policyPath, validatorRoot: args['validator-root'] ?? null });
    process.stdout.write(stableJson(audit));
    return audit.status === 'passed' ? 0 : 1;
  }
  assert(['write', 'check'].includes(mode), 'mode must be audit-policy, write, or check');
  for (const name of [
    'validator-root', 'contract-ir', 'parity-receipt', 'typespec', 'generated-schema', 'authored-schema',
  ]) {
    assert(validText(args[name], 1024), `--${name} is required`);
  }
  const result = await buildProjectionAdmission({
    root,
    validatorRoot: args['validator-root'],
    policyPath,
    contractIrPath: args['contract-ir'],
    parityReceiptPath: args['parity-receipt'],
    typespecPath: args.typespec,
    generatedSchemaPath: args['generated-schema'],
    authoredSchemaPath: args['authored-schema'],
  });
  if (mode === 'write') {
    await safeOwnedWrite(root, result.policy.activation.manifestPath, result.manifest, new Set([MANIFEST_SCHEMA]));
    await safeOwnedWrite(root, result.policy.activation.reportPath, result.report, new Set([REPORT_SCHEMA]));
  } else {
    await checkOwnedJson(root, result.policy.activation.manifestPath, result.manifest);
    await checkOwnedJson(root, result.policy.activation.reportPath, result.report);
  }
  process.stdout.write(stableJson(result.report));
  return 0;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`rpc-v1 projection admission failed: ${error.message}\n`);
    process.exitCode = 1;
  });
}
