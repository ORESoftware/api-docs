import assert from 'node:assert/strict';
import { execFile as execFileCallback } from 'node:child_process';
import { link, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';
import test from 'node:test';
import {
  DELTA_APPROVAL_SCHEMA,
  POLICY_AUDIT_SCHEMA,
  POLICY_SCHEMA,
  RUNTIME_EVIDENCE_SCHEMA,
  auditProjectionAdmissionPolicy,
  buildProjectionAdmission,
  normalizeProjectionAdmissionPolicy,
} from './rpc-v1-projection-admission.mjs';

const execFile = promisify(execFileCallback);
const validatorRoot = resolve(process.env.TSJSV_VALIDATOR_ROOT ?? '');
const scriptPath = fileURLToPath(new URL('./rpc-v1-projection-admission.mjs', import.meta.url));

async function validatorHead() {
  const { stdout } = await execFile('git', ['-C', validatorRoot, 'rev-parse', 'HEAD'], { encoding: 'utf8' });
  return stdout.trim();
}

async function writeJson(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
}

function policyFor(revision, status = 'blocked') {
  return {
    schema: POLICY_SCHEMA,
    validator: {
      repository: 'ORESoftware/typespec-json-schema-validator',
      revision,
      manifestSchema: 'ores.typespec-json-schema-validator.projection-manifest/v1',
    },
    activation: {
      status,
      manifestPath: 'generated/projection-manifest.json',
      reportPath: 'generated/projection-admission-report.json',
      blockers: status === 'blocked'
        ? [{ id: 'contract-ir', reason: 'Exact current Contract IR evidence is not available.' }]
        : [],
    },
    inputs: {
      operationInventory: 'idl/operations.json',
      projectionMetadata: 'idl/lock.json',
      emitterConfiguration: 'idl/policy.json',
    },
    outputs: [{ path: 'generated/output.txt', mediaType: 'text/plain', projection: 'demo' }],
    projections: [{
      id: 'demo',
      emitter: 'demo-emitter',
      declarationMode: 'all',
      representationDeltaIds: [],
      runtimeValidatorIds: [],
    }],
    toolchains: [
      { id: 'demo-generator', version: '1', root: 'repository', files: ['scripts/generator.mjs'] },
      {
        id: 'typespec-json-schema-validator',
        version: `git:${revision}`,
        root: 'validator',
        files: ['package.json', 'src/projection-admission/index.mjs'],
      },
    ],
    approvedDeltas: status === 'blocked' ? 'idl/approvals.json' : null,
    runtimeValidators: status === 'blocked' ? 'runtime/validators.json' : null,
  };
}

async function commonFiles(root, policy) {
  await writeJson(join(root, 'idl/operations.json'), { operationId: 'demo.call' });
  await writeJson(join(root, 'idl/lock.json'), { messages: { Ping: { fields: { id: 1 } } } });
  await mkdir(join(root, 'generated'), { recursive: true });
  await writeFile(join(root, 'generated/output.txt'), 'generated output\n');
  await mkdir(join(root, 'scripts'), { recursive: true });
  await writeFile(join(root, 'scripts/generator.mjs'), 'export const version = 1;\n');
  await writeJson(join(root, 'idl/policy.json'), policy);
}

async function blockedFixture() {
  assert(validatorRoot, 'TSJSV_VALIDATOR_ROOT is required');
  const root = await mkdtemp(join(tmpdir(), 'api-docs-admission-blocked-'));
  const revision = await validatorHead();
  const policy = policyFor(revision, 'blocked');
  await commonFiles(root, policy);
  await writeJson(join(root, 'idl/approvals.json'), {
    schema: DELTA_APPROVAL_SCHEMA,
    status: 'blocked',
    approvals: [],
    blockers: ['approval-not-recorded'],
  });
  await writeJson(join(root, 'runtime/validators.json'), {
    schema: RUNTIME_EVIDENCE_SCHEMA,
    status: 'blocked',
    validators: [],
    blockers: ['ingress-egress-not-covered'],
  });
  return { root, revision, policy };
}

async function enabledFixture() {
  assert(validatorRoot, 'TSJSV_VALIDATOR_ROOT is required');
  const root = await mkdtemp(join(tmpdir(), 'api-docs-admission-enabled-'));
  const revision = await validatorHead();
  const policy = policyFor(revision, 'enabled');
  await commonFiles(root, policy);
  await mkdir(join(root, 'authorities/json-schema'), { recursive: true });
  await writeFile(join(root, 'authorities/main.tsp'), [
    'model Ping {',
    '  @minLength(1)',
    '  id: string;',
    '}',
    '',
  ].join('\n'));
  await writeJson(join(root, 'authorities/json-schema/Ping.schema.json'), {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    $id: 'https://example.invalid/Ping.schema.json',
    title: 'Ping',
    type: 'object',
    required: ['id'],
    properties: { id: { type: 'string', minLength: 1 } },
    unevaluatedProperties: false,
  });

  const validator = await import(pathToFileURL(join(validatorRoot, 'src/index.mjs')).href);
  const generatedDir = join(root, 'evidence/generated');
  const receipt = await validator.runCheck({
    typespec: join(root, 'authorities/main.tsp'),
    authoredSchema: join(root, 'authorities/json-schema'),
    outputDir: generatedDir,
    bundleId: 'typespec.generated.schema.json',
    maxFindings: 250,
    maxProbes: 64,
    probes: true,
    formatAssertion: false,
    instances: undefined,
    mapping: undefined,
    tspBin: join(validatorRoot, 'node_modules/.bin/tsp'),
    int64Strategy: 'string',
    sealObjectSchemas: true,
    polymorphicModelsStrategy: 'oneOf',
  });
  assert.equal(receipt.status, 'passed', JSON.stringify(receipt.findings));
  const generatedSchema = join(generatedDir, 'typespec.generated.schema.json');
  const contractIr = await validator.buildContractIr({
    report: receipt,
    typespec: join(root, 'authorities/main.tsp'),
    generatedSchema,
    authoredSchema: join(root, 'authorities/json-schema'),
  });
  await writeJson(join(root, 'evidence/parity-receipt.json'), receipt);
  await writeJson(join(root, 'evidence/contract-ir.json'), contractIr);
  return {
    root,
    policy,
    generatedSchemaPath: relative(root, generatedSchema).replaceAll('\\', '/'),
  };
}

function buildArgs(fixture) {
  return {
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  };
}

function cliArgs(fixture, mode) {
  return [
    scriptPath,
    `--mode=${mode}`,
    `--root=${fixture.root}`,
    `--validator-root=${validatorRoot}`,
    '--policy=idl/policy.json',
    '--contract-ir=evidence/contract-ir.json',
    '--parity-receipt=evidence/parity-receipt.json',
    '--typespec=authorities/main.tsp',
    `--generated-schema=${fixture.generatedSchemaPath}`,
    '--authored-schema=authorities/json-schema',
  ];
}

test('normalizes a closed blocked policy without source precedence', async () => {
  const { policy } = await blockedFixture();
  const normalized = normalizeProjectionAdmissionPolicy(policy);
  assert.equal(normalized.activation.status, 'blocked');
  assert.deepEqual(normalized.activation.blockers.map((item) => item.id), ['contract-ir']);
  assert.equal(JSON.stringify(normalized).includes('precedence'), false);
});

test('policy normalization rejects unknown properties and floating revisions', async () => {
  const { policy } = await blockedFixture();
  assert.throws(() => normalizeProjectionAdmissionPolicy({ ...policy, copiedGreenStatus: true }), /unsupported properties/u);
  policy.validator.revision = 'main';
  assert.throws(() => normalizeProjectionAdmissionPolicy(policy), /40-character commit SHA/u);
});

test('policy normalization enforces coherent blocker state', async () => {
  const { policy } = await blockedFixture();
  policy.activation.status = 'enabled';
  assert.throws(() => normalizeProjectionAdmissionPolicy(policy), /must not retain blockers/u);
  policy.activation.status = 'blocked';
  policy.activation.blockers = [];
  assert.throws(() => normalizeProjectionAdmissionPolicy(policy), /at least one blocker/u);
});

test('blocked policy audit is green without claiming projection admission', async () => {
  const fixture = await blockedFixture();
  const report = await auditProjectionAdmissionPolicy({ root: fixture.root, policyPath: 'idl/policy.json', validatorRoot });
  assert.equal(report.schema, POLICY_AUDIT_SCHEMA);
  assert.equal(report.status, 'passed');
  assert.equal(report.policyValid, true);
  assert.equal(report.projectionAdmissible, false);
  assert.equal(report.activationStatus, 'blocked');
});

test('blocked audit rejects stale manifest and report artifacts', async () => {
  const fixture = await blockedFixture();
  await writeJson(join(fixture.root, fixture.policy.activation.manifestPath), {
    schema: 'ores.typespec-json-schema-validator.projection-manifest/v1',
  });
  await writeJson(join(fixture.root, fixture.policy.activation.reportPath), {
    schema: 'ores.typespec-json-schema-validator.projection-admission-report/v1',
  });
  const report = await auditProjectionAdmissionPolicy({ root: fixture.root, policyPath: 'idl/policy.json', validatorRoot });
  assert.equal(report.status, 'failed');
  assert.equal(report.findings.filter((item) => item.ruleId === 'projection-policy-stale-green-artifact').length, 2);
});

test('blocked audit rejects symlink outputs and hard-linked inputs', async () => {
  const fixture = await blockedFixture();
  await writeFile(join(fixture.root, 'target.txt'), 'target');
  await rm(join(fixture.root, 'generated/output.txt'));
  await symlink(join(fixture.root, 'target.txt'), join(fixture.root, 'generated/output.txt'));
  await link(join(fixture.root, 'idl/operations.json'), join(fixture.root, 'idl/operations-copy.json'));
  const report = await auditProjectionAdmissionPolicy({ root: fixture.root, policyPath: 'idl/policy.json', validatorRoot });
  const ids = report.findings.map((item) => item.ruleId);
  assert.ok(ids.includes('projection-policy-output-unreadable'));
  assert.ok(ids.includes('projection-policy-input-unreadable'));
});

test('blocked audit rejects wrong validator revision and redacts malformed JSON', async () => {
  const fixture = await blockedFixture();
  fixture.policy.validator.revision = '0'.repeat(40);
  fixture.policy.toolchains[1].version = `git:${'0'.repeat(40)}`;
  await writeJson(join(fixture.root, 'idl/policy.json'), fixture.policy);
  let report = await auditProjectionAdmissionPolicy({ root: fixture.root, policyPath: 'idl/policy.json', validatorRoot });
  assert.ok(report.findings.some((item) => item.ruleId === 'projection-policy-validator-revision-mismatch'));
  await writeFile(join(fixture.root, 'idl/policy.json'), '{SENTINEL_PRIVATE_VALUE');
  report = await auditProjectionAdmissionPolicy({ root: fixture.root, policyPath: 'idl/policy.json', validatorRoot });
  assert.equal(JSON.stringify(report).includes('SENTINEL_PRIVATE_VALUE'), false);
});

test('builds a real exact-input Contract IR projection manifest', async () => {
  const fixture = await enabledFixture();
  const result = await buildProjectionAdmission(buildArgs(fixture));
  assert.equal(result.contractVerification.admissible, true);
  assert.equal(result.manifest.status, 'passed');
  assert.equal(result.report.admissible, true);
  assert.deepEqual(result.manifest.declarations, ['Ping']);
  assert.deepEqual(result.manifest.projections[0].outputPaths, ['generated/output.txt']);
});

test('manifest separates operation inventory, lock, and emitter configuration', async () => {
  const fixture = await enabledFixture();
  const result = await buildProjectionAdmission(buildArgs(fixture));
  assert.deepEqual(Object.values(result.manifest.inputs).map((item) => item.path).sort(), [
    'idl/lock.json',
    'idl/operations.json',
    'idl/policy.json',
  ]);
});

test('manifest binds exact validator and generator source closures', async () => {
  const fixture = await enabledFixture();
  const result = await buildProjectionAdmission(buildArgs(fixture));
  assert.deepEqual(result.manifest.toolchains.map((item) => item.id), [
    'demo-generator',
    'typespec-json-schema-validator',
  ]);
  for (const toolchain of result.manifest.toolchains) assert.match(toolchain.artifactDigest, /^[a-f0-9]{64}$/u);
});

test('enabled build rejects current TypeSpec drift', async () => {
  const fixture = await enabledFixture();
  await writeFile(join(fixture.root, 'authorities/main.tsp'), 'model Ping { id: string; changed?: string; }\n');
  await assert.rejects(buildProjectionAdmission(buildArgs(fixture)), /does not match the exact current authority closure/u);
});

test('enabled build rejects a copied passed receipt with another run id', async () => {
  const fixture = await enabledFixture();
  const receiptPath = join(fixture.root, 'evidence/parity-receipt.json');
  const receipt = JSON.parse(await readFile(receiptPath, 'utf8'));
  receipt.runId = '0'.repeat(64);
  await writeJson(receiptPath, receipt);
  await assert.rejects(buildProjectionAdmission(buildArgs(fixture)), /does not match the exact current authority closure/u);
});

test('enabled build re-hashes actual outputs', async () => {
  const fixture = await enabledFixture();
  const first = await buildProjectionAdmission(buildArgs(fixture));
  await writeFile(join(fixture.root, 'generated/output.txt'), 'changed output\n');
  const second = await buildProjectionAdmission(buildArgs(fixture));
  assert.notEqual(first.manifest.outputs[0].sha256, second.manifest.outputs[0].sha256);
  assert.notEqual(first.manifest.manifestId, second.manifest.manifestId);
});

test('CLI write/check round trip succeeds and later output drift fails', async () => {
  const fixture = await enabledFixture();
  await execFile(process.execPath, cliArgs(fixture, 'write'), { encoding: 'utf8' });
  await execFile(process.execPath, cliArgs(fixture, 'check'), { encoding: 'utf8' });
  const manifest = JSON.parse(await readFile(join(fixture.root, fixture.policy.activation.manifestPath), 'utf8'));
  const report = JSON.parse(await readFile(join(fixture.root, fixture.policy.activation.reportPath), 'utf8'));
  assert.equal(manifest.schema, 'ores.typespec-json-schema-validator.projection-manifest/v1');
  assert.equal(report.admissible, true);
  await writeFile(join(fixture.root, 'generated/output.txt'), 'drifted\n');
  await assert.rejects(execFile(process.execPath, cliArgs(fixture, 'check'), { encoding: 'utf8' }), /stale/u);
});

test('CLI audit-policy reports blocked status without a green projection claim', async () => {
  const fixture = await blockedFixture();
  const { stdout } = await execFile(process.execPath, [
    scriptPath,
    '--mode=audit-policy',
    `--root=${fixture.root}`,
    `--validator-root=${validatorRoot}`,
    '--policy=idl/policy.json',
  ], { encoding: 'utf8' });
  const report = JSON.parse(stdout);
  assert.equal(report.status, 'passed');
  assert.equal(report.activationStatus, 'blocked');
  assert.equal(report.projectionAdmissible, false);
});
