import assert from 'node:assert/strict';
import { execFile as execFileCallback } from 'node:child_process';
import { link, mkdir, mkdtemp, readFile, symlink, writeFile } from 'node:fs/promises';
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

function blockedPolicy(revision) {
  return {
    schema: POLICY_SCHEMA,
    validator: {
      repository: 'ORESoftware/typespec-json-schema-validator',
      revision,
      manifestSchema: 'ores.typespec-json-schema-validator.projection-manifest/v1',
    },
    activation: {
      status: 'blocked',
      manifestPath: 'generated/projection-manifest.json',
      reportPath: 'generated/projection-admission-report.json',
      blockers: [
        { id: 'contract-ir', reason: 'Exact current Contract IR evidence is not available.' },
      ],
    },
    inputs: {
      operationInventory: 'idl/operations.json',
      projectionMetadata: 'idl/lock.json',
      emitterConfiguration: 'idl/policy.json',
    },
    outputs: [
      { path: 'generated/output.txt', mediaType: 'text/plain', projection: 'demo' },
    ],
    projections: [
      {
        id: 'demo',
        emitter: 'demo-emitter',
        declarationMode: 'all',
        representationDeltaIds: [],
        runtimeValidatorIds: [],
      },
    ],
    toolchains: [
      { id: 'demo-generator', version: '1', root: 'repository', files: ['scripts/generator.mjs'] },
      {
        id: 'typespec-json-schema-validator',
        version: `git:${revision}`,
        root: 'validator',
        files: ['package.json', 'src/projection-admission/index.mjs'],
      },
    ],
    approvedDeltas: 'idl/approvals.json',
    runtimeValidators: 'runtime/validators.json',
  };
}

async function prepareBlockedRoot() {
  assert(validatorRoot, 'TSJSV_VALIDATOR_ROOT is required');
  const root = await mkdtemp(join(tmpdir(), 'api-docs-admission-blocked-'));
  const revision = await validatorHead();
  const policy = blockedPolicy(revision);
  await writeJson(join(root, 'idl/operations.json'), { operationId: 'demo.call' });
  await writeJson(join(root, 'idl/lock.json'), { fields: { id: 1 } });
  await writeFile(join(root, 'generated-output.tmp'), 'unused');
  await mkdir(join(root, 'generated'), { recursive: true });
  await writeFile(join(root, 'generated/output.txt'), 'generated output\n');
  await mkdir(join(root, 'scripts'), { recursive: true });
  await writeFile(join(root, 'scripts/generator.mjs'), 'export const version = 1;\n');
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
  await writeJson(join(root, 'idl/policy.json'), policy);
  return { root, revision, policy };
}

function enabledPolicy(revision) {
  const policy = blockedPolicy(revision);
  policy.activation.status = 'enabled';
  policy.activation.blockers = [];
  policy.approvedDeltas = null;
  policy.runtimeValidators = null;
  return policy;
}

async function prepareEnabledFixture() {
  assert(validatorRoot, 'TSJSV_VALIDATOR_ROOT is required');
  const root = await mkdtemp(join(tmpdir(), 'api-docs-admission-enabled-'));
  const revision = await validatorHead();
  const policy = enabledPolicy(revision);
  await mkdir(join(root, 'authorities/json-schema'), { recursive: true });
  await writeFile(join(root, 'authorities/main.tsp'), [
    'namespace Demo;',
    '',
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
    'x-typespec-name': 'Demo.Ping',
    type: 'object',
    required: ['id'],
    properties: {
      id: { type: 'string', minLength: 1 },
    },
    additionalProperties: false,
  });
  await writeJson(join(root, 'idl/operations.json'), { operationId: 'demo.call' });
  await writeJson(join(root, 'idl/lock.json'), { messages: { 'demo.Ping': { fields: { id: 1 } } } });
  await mkdir(join(root, 'generated'), { recursive: true });
  await writeFile(join(root, 'generated/output.txt'), 'generated output\n');
  await mkdir(join(root, 'scripts'), { recursive: true });
  await writeFile(join(root, 'scripts/generator.mjs'), 'export const version = 1;\n');
  await writeJson(join(root, 'idl/policy.json'), policy);

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
    revision,
    policy,
    generatedSchemaPath: relative(root, generatedSchema).replaceAll('\\', '/'),
  };
}

test('normalizes a closed blocked policy without ranking a source authority', async () => {
  const { policy } = await prepareBlockedRoot();
  const normalized = normalizeProjectionAdmissionPolicy(policy);
  assert.equal(normalized.activation.status, 'blocked');
  assert.deepEqual(normalized.activation.blockers.map((item) => item.id), ['contract-ir']);
  assert.equal(JSON.stringify(normalized).includes('precedence'), false);
});

test('rejects unknown policy properties', async () => {
  const { policy } = await prepareBlockedRoot();
  assert.throws(
    () => normalizeProjectionAdmissionPolicy({ ...policy, copiedGreenStatus: true }),
    /unsupported properties/u,
  );
});

test('rejects a floating validator revision', async () => {
  const { policy } = await prepareBlockedRoot();
  policy.validator.revision = 'main';
  assert.throws(() => normalizeProjectionAdmissionPolicy(policy), /40-character commit SHA/u);
});

test('rejects enabled activation that retains blockers', async () => {
  const { policy } = await prepareBlockedRoot();
  policy.activation.status = 'enabled';
  assert.throws(() => normalizeProjectionAdmissionPolicy(policy), /must not retain blockers/u);
});

test('rejects blocked activation without an explicit blocker', async () => {
  const { policy } = await prepareBlockedRoot();
  policy.activation.blockers = [];
  assert.throws(() => normalizeProjectionAdmissionPolicy(policy), /at least one blocker/u);
});

test('audits an explicitly blocked product policy without reporting projection admission', async () => {
  const fixture = await prepareBlockedRoot();
  const report = await auditProjectionAdmissionPolicy({
    root: fixture.root,
    policyPath: 'idl/policy.json',
    validatorRoot,
  });
  assert.equal(report.schema, POLICY_AUDIT_SCHEMA);
  assert.equal(report.status, 'passed');
  assert.equal(report.policyValid, true);
  assert.equal(report.projectionAdmissible, false);
  assert.equal(report.activationStatus, 'blocked');
});

test('blocked policy audit rejects a stale manifest', async () => {
  const fixture = await prepareBlockedRoot();
  await writeJson(join(fixture.root, fixture.policy.activation.manifestPath), {
    schema: 'ores.typespec-json-schema-validator.projection-manifest/v1',
  });
  const report = await auditProjectionAdmissionPolicy({
    root: fixture.root,
    policyPath: 'idl/policy.json',
    validatorRoot,
  });
  assert.equal(report.status, 'failed');
  assert.ok(report.findings.some((item) => item.ruleId === 'projection-policy-stale-green-artifact'));
});

test('blocked policy audit rejects a stale report', async () => {
  const fixture = await prepareBlockedRoot();
  await writeJson(join(fixture.root, fixture.policy.activation.reportPath), {
    schema: 'ores.typespec-json-schema-validator.projection-admission-report/v1',
  });
  const report = await auditProjectionAdmissionPolicy({
    root: fixture.root,
    policyPath: 'idl/policy.json',
    validatorRoot,
  });
  assert.equal(report.status, 'failed');
  assert.ok(report.findings.some((item) => item.ruleId === 'projection-policy-stale-green-artifact'));
});

test('policy audit rejects a symbolic-link output', async () => {
  const fixture = await prepareBlockedRoot();
  await writeFile(join(fixture.root, 'target.txt'), 'target');
  await writeFile(join(fixture.root, 'generated/output.txt'), 'replacement');
  await (await import('node:fs/promises')).rm(join(fixture.root, 'generated/output.txt'));
  await symlink(join(fixture.root, 'target.txt'), join(fixture.root, 'generated/output.txt'));
  const report = await auditProjectionAdmissionPolicy({
    root: fixture.root,
    policyPath: 'idl/policy.json',
    validatorRoot,
  });
  assert.ok(report.findings.some((item) => item.ruleId === 'projection-policy-output-unreadable'));
});

test('policy audit rejects a multiply linked input', async () => {
  const fixture = await prepareBlockedRoot();
  await link(join(fixture.root, 'idl/operations.json'), join(fixture.root, 'idl/operations-copy.json'));
  const report = await auditProjectionAdmissionPolicy({
    root: fixture.root,
    policyPath: 'idl/policy.json',
    validatorRoot,
  });
  assert.ok(report.findings.some((item) => item.ruleId === 'projection-policy-input-unreadable'));
});

test('policy audit rejects the wrong validator checkout', async () => {
  const fixture = await prepareBlockedRoot();
  fixture.policy.validator.revision = '0'.repeat(40);
  fixture.policy.toolchains[1].version = `git:${'0'.repeat(40)}`;
  await writeJson(join(fixture.root, 'idl/policy.json'), fixture.policy);
  const report = await auditProjectionAdmissionPolicy({
    root: fixture.root,
    policyPath: 'idl/policy.json',
    validatorRoot,
  });
  assert.ok(report.findings.some((item) => item.ruleId === 'projection-policy-validator-revision-mismatch'));
});

test('policy audit redacts malformed JSON contents', async () => {
  const fixture = await prepareBlockedRoot();
  await writeFile(join(fixture.root, 'idl/policy.json'), '{SENTINEL_PRIVATE_VALUE');
  const report = await auditProjectionAdmissionPolicy({
    root: fixture.root,
    policyPath: 'idl/policy.json',
    validatorRoot,
  });
  assert.equal(JSON.stringify(report).includes('SENTINEL_PRIVATE_VALUE'), false);
});

test('builds and verifies a real projection manifest from an exact Contract IR', async () => {
  const fixture = await prepareEnabledFixture();
  const result = await buildProjectionAdmission({
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  });
  assert.equal(result.contractVerification.admissible, true);
  assert.equal(result.manifest.status, 'passed');
  assert.equal(result.report.admissible, true);
  assert.deepEqual(result.manifest.declarations, ['Demo.Ping']);
  assert.deepEqual(result.manifest.projections[0].outputPaths, ['generated/output.txt']);
});

test('manifest binds operation inventory, projection metadata, and emitter configuration separately', async () => {
  const fixture = await prepareEnabledFixture();
  const result = await buildProjectionAdmission({
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  });
  const paths = Object.values(result.manifest.inputs).map((item) => item.path).sort();
  assert.deepEqual(paths, ['idl/lock.json', 'idl/operations.json', 'idl/policy.json']);
});

test('manifest binds exact validator and generator source closures', async () => {
  const fixture = await prepareEnabledFixture();
  const result = await buildProjectionAdmission({
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  });
  assert.deepEqual(result.manifest.toolchains.map((item) => item.id), [
    'demo-generator',
    'typespec-json-schema-validator',
  ]);
  for (const toolchain of result.manifest.toolchains) {
    assert.match(toolchain.artifactDigest, /^[a-f0-9]{64}$/u);
  }
});

test('enabled build rejects source drift after the Contract IR receipt', async () => {
  const fixture = await prepareEnabledFixture();
  await writeFile(join(fixture.root, 'authorities/main.tsp'), [
    'namespace Demo;',
    'model Ping { id: string; changed?: string; }',
    '',
  ].join('\n'));
  await assert.rejects(buildProjectionAdmission({
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  }), /does not match the exact current authority closure/u);
});

test('enabled build rejects a copied passed receipt with a different run id', async () => {
  const fixture = await prepareEnabledFixture();
  const receiptPath = join(fixture.root, 'evidence/parity-receipt.json');
  const receipt = JSON.parse(await readFile(receiptPath, 'utf8'));
  receipt.runId = '0'.repeat(64);
  await writeJson(receiptPath, receipt);
  await assert.rejects(buildProjectionAdmission({
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  }), /does not match the exact current authority closure/u);
});

test('enabled build re-hashes actual outputs rather than trusting policy metadata', async () => {
  const fixture = await prepareEnabledFixture();
  const first = await buildProjectionAdmission({
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  });
  await writeFile(join(fixture.root, 'generated/output.txt'), 'changed output\n');
  const second = await buildProjectionAdmission({
    root: fixture.root,
    validatorRoot,
    policyPath: 'idl/policy.json',
    contractIrPath: 'evidence/contract-ir.json',
    parityReceiptPath: 'evidence/parity-receipt.json',
    typespecPath: 'authorities/main.tsp',
    generatedSchemaPath: fixture.generatedSchemaPath,
    authoredSchemaPath: 'authorities/json-schema',
  });
  assert.notEqual(first.manifest.outputs[0].sha256, second.manifest.outputs[0].sha256);
  assert.notEqual(first.manifest.manifestId, second.manifest.manifestId);
});

test('CLI write and check round trip exact generated evidence', async () => {
  const fixture = await prepareEnabledFixture();
  const args = [
    scriptPath,
    '--mode=write',
    `--root=${fixture.root}`,
    `--validator-root=${validatorRoot}`,
    '--policy=idl/policy.json',
    '--contract-ir=evidence/contract-ir.json',
    '--parity-receipt=evidence/parity-receipt.json',
    '--typespec=authorities/main.tsp',
    `--generated-schema=${fixture.generatedSchemaPath}`,
    '--authored-schema=authorities/json-schema',
  ];
  await execFile(process.execPath, args, { encoding: 'utf8' });
  const checkArgs = args.map((item) => item === '--mode=write' ? '--mode=check' : item);
  await execFile(process.execPath, checkArgs, { encoding: 'utf8' });
  const manifest = JSON.parse(await readFile(join(fixture.root, fixture.policy.activation.manifestPath), 'utf8'));
  const report = JSON.parse(await readFile(join(fixture.root, fixture.policy.activation.reportPath), 'utf8'));
  assert.equal(manifest.schema, 'ores.typespec-json-schema-validator.projection-manifest/v1');
  assert.equal(report.admissible, true);
});

test('CLI check fails after output drift', async () => {
  const fixture = await prepareEnabledFixture();
  const common = [
    scriptPath,
    `--root=${fixture.root}`,
    `--validator-root=${validatorRoot}`,
    '--policy=idl/policy.json',
    '--contract-ir=evidence/contract-ir.json',
    '--parity-receipt=evidence/parity-receipt.json',
    '--typespec=authorities/main.tsp',
    `--generated-schema=${fixture.generatedSchemaPath}`,
    '--authored-schema=authorities/json-schema',
  ];
  await execFile(process.execPath, [common[0], '--mode=write', ...common.slice(1)], { encoding: 'utf8' });
  await writeFile(join(fixture.root, 'generated/output.txt'), 'drifted\n');
  await assert.rejects(
    execFile(process.execPath, [common[0], '--mode=check', ...common.slice(1)], { encoding: 'utf8' }),
    /stale/u,
  );
});

test('CLI audit-policy reports blocked status without claiming projection admission', async () => {
  const fixture = await prepareBlockedRoot();
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
