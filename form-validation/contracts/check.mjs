import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { readSafeBytes } from '../../scripts/projection-evidence-io.mjs';
import { verifyValidatorSource } from '../../scripts/tjsv-source-integrity.mjs';
import { fieldCases, inspectObservation, requireThat, wireCases } from './corpus.mjs';
import { buildBoundaryEvidence, buildBoundaryManifest } from './language-boundary.mjs';

export const TJSV_REVISION = '4740f1367a7906813dcd420a77d0c9ede26943fb';
const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const OUT = resolve(ROOT, 'tmp/tjsv-form');
const TJSV = resolve(ROOT, 'tmp/tjsv');
const TYPESPEC = resolve(ROOT, 'form-validation/contracts/main.tsp');
const AUTHORED = resolve(ROOT, 'form-validation/contracts/authored.schema.json');
const SOURCE_PATHS = Object.freeze([
  '.github/workflows/tjsv-form-contract.yml',
  'form-validation/contracts/authored.schema.json',
  'form-validation/contracts/check.mjs',
  'form-validation/contracts/check.test.mjs',
  'form-validation/contracts/corpus.mjs',
  'form-validation/contracts/language-boundary.mjs',
  'form-validation/contracts/language-boundary.test.mjs',
  'form-validation/contracts/main.tsp',
  'form-validation/dart/lib/ores_form_validation.dart',
  'form-validation/dart/lib/validation_message.dart',
  'form-validation/dart/pubspec.lock',
  'form-validation/dart/pubspec.yaml',
  'form-validation/dart/test/shared.dart',
  'form-validation/dart/test/tjsv_probe_js.dart',
  'form-validation/dart/test/tjsv_probe_shared.dart',
  'form-validation/dart/test/tjsv_probe_vm.dart',
  'form-validation/fixtures.json',
  'form-validation/rust/Cargo.toml',
  'form-validation/rust/src/lib.rs',
  'form-validation/wire-rust/Cargo.lock',
  'form-validation/wire-rust/Cargo.toml',
  'form-validation/wire-rust/examples/tjsv_probe.rs',
  'form-validation/wire-rust/src/lib.rs',
  'scripts/projection-evidence-io.mjs',
  'scripts/tjsv-source-integrity.mjs',
]);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function command(executable, args, options = {}) {
  return execFileSync(executable, args, {
    cwd: ROOT,
    encoding: 'utf8',
    maxBuffer: 4 * 1024 * 1024,
    timeout: 180000,
    stdio: ['pipe', 'pipe', 'pipe'],
    ...options,
  }).trim();
}
function commandIdentity(executable, args, cwd = ROOT) {
  const child = spawnSync(executable, args, {
    cwd,
    encoding: 'utf8',
    maxBuffer: 1024 * 1024,
    timeout: 180000,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  requireThat(child.error === undefined && child.status === 0, `${executable} identity probe failed`);
  const value = `${child.stdout ?? ''}${child.stderr ?? ''}`.trim();
  requireThat(value.length > 0 && value.length <= 1024, `${executable} identity probe was empty or oversized`);
  return value;
}
const extractVersion = (name, value, expression) => {
  const match = expression.exec(value);
  requireThat(match !== null && typeof match[1] === 'string' && match[1].length > 0, `unable to parse ${name} version`);
  return match[1];
};
const git = (cwd, ...args) => command('git', ['-C', cwd, ...args]);
const repoRelative = absolutePath => {
  const path = relative(ROOT, absolutePath).split(sep).join('/');
  requireThat(path.length > 0 && path !== '..' && !path.startsWith('../'), 'schema path escapes repository root');
  return path;
};
async function save(name, value) {
  await writeFile(resolve(OUT, name), `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' });
}
async function candidateSnapshot(revision) {
  const output = git(ROOT, 'ls-tree', '-rz', '--full-tree', revision, '--', ...SOURCE_PATHS);
  const snapshots = {};
  for (const entry of output.split('\0').filter(Boolean)) {
    const match = /^(100644|100755) blob ([a-f0-9]{40})\t(.+)$/u.exec(entry);
    requireThat(match !== null, 'form contract source must be a regular tracked file');
    const [, , blobSha, path] = match;
    const bytes = await readSafeBytes(ROOT, path);
    const observedBlob = createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
    requireThat(observedBlob === blobSha, `form contract source differs from candidate commit: ${path}`);
    snapshots[path] = bytes;
  }
  const expected = [...SOURCE_PATHS].sort();
  const actual = Object.keys(snapshots).sort();
  requireThat(JSON.stringify(actual) === JSON.stringify(expected), 'form contract source inventory is incomplete');
  return snapshots;
}

export async function main() {
  requireThat(process.argv.length === 2, 'fixed CI entrypoint accepts no arguments');
  process.chdir(ROOT);
  await verifyValidatorSource(TJSV, TJSV_REVISION);
  const revision = git(ROOT, 'rev-parse', 'HEAD');
  requireThat(/^[a-f0-9]{40}$/u.test(revision), 'candidate source revision must be a full immutable SHA');
  const snapshots = await candidateSnapshot(revision);
  const sourceDigests = Object.fromEntries(
    Object.keys(snapshots).sort().map(path => [path, digest(snapshots[path])]),
  );

  await mkdir(OUT, { recursive: true });
  const tjsv = await import(pathToFileURL(resolve(TJSV, 'src/index.mjs')).href);
  const runtime = await import(pathToFileURL(resolve(TJSV, 'src/runtime-conformance/index.mjs')).href);
  const boundary = await import(pathToFileURL(resolve(TJSV, 'src/language-boundary-verification.mjs')).href);
  const decodeJson = bytes => JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes));
  const fields = fieldCases(decodeJson(snapshots['form-validation/fixtures.json']));
  const wire = wireCases(fields);
  for (const row of wire) {
    const dir = resolve(OUT, 'instances/ValidationMessage', row.expected ? 'valid' : 'invalid');
    await mkdir(dir, { recursive: true });
    await writeFile(resolve(dir, `${row.id}.json`), JSON.stringify(row.instance), { flag: 'wx' });
  }

  const parityReport = await tjsv.runCheck({
    typespec: TYPESPEC,
    authoredSchema: AUTHORED,
    outputDir: resolve(OUT, 'witness'),
    bundleId: 'form-validation.json',
    sealObjectSchemas: true,
    maxFindings: 1000,
    instances: resolve(OUT, 'instances'),
    probes: true,
    maxProbes: 64,
    formatAssertion: true,
  });
  await save('parity.json', parityReport);
  requireThat(parityReport.status === 'passed' && parityReport.zeroUnexplainedFindings === true, 'TypeSpec/authored JSON Schema disagreement');
  requireThat(Array.isArray(parityReport.findings) && parityReport.findings.length === 0, 'passing parity report did not retain explicit zero findings');
  requireThat(parityReport.coverage?.differentialInstanceValidation === true, 'differential instance validation did not execute');
  requireThat(parityReport.declarationMap.length === 3, 'incomplete declaration coverage');
  requireThat(parityReport.differential?.summary?.comparedDeclarations === 3, 'incomplete differential declaration coverage');
  requireThat(parityReport.differential.summary.probesEvaluated > 0, 'differential probes did not execute');
  requireThat(parityReport.differential.summary.divergences === 0 && parityReport.differential.summary.refusals === 0, 'differential parity diverged or refused evaluation');

  const generatedSchema = resolve(ROOT, parityReport.inputs.generatedJsonSchema.input);
  const contractIr = await tjsv.buildContractIr({
    report: parityReport,
    typespec: TYPESPEC,
    generatedSchema,
    authoredSchema: AUTHORED,
  });
  await save('contract-ir.json', contractIr);
  requireThat(contractIr.status === 'passed' && contractIr.admissible === true, 'Contract IR was not admissible');
  requireThat(Array.isArray(contractIr.declarations) && contractIr.declarations.length > 0, 'Contract IR declaration inventory is empty');
  requireThat(Array.isArray(contractIr.excludedDeclarations) && contractIr.excludedDeclarations.length === 0, 'Contract IR excludes declarations at a runtime boundary');
  requireThat(Array.isArray(contractIr.outOfScopeDeclarations) && contractIr.outOfScopeDeclarations.length === 0, 'Contract IR leaves declarations out of scope at a runtime boundary');
  if (Object.hasOwn(contractIr, 'complete')) requireThat(contractIr.complete === true, 'Contract IR is not complete');

  const current = {
    contractIr,
    parityReport,
    typespec: TYPESPEC,
    generatedSchema,
    authoredSchema: AUTHORED,
  };
  const binding = await runtime.createRuntimeEvidenceBindingAgainstCurrentInputs(current);
  const corpusDigest = digest(Buffer.from(tjsv.canonicalStringify({ wire, fields })));
  const expectedCases = wire.map(row => ({
    id: row.id,
    declaration: 'ValidationMessage',
    expectation: row.expected ? 'accepted' : 'rejected',
  }));
  const payload = JSON.stringify({
    messages: wire.map(({ id, instance }) => ({ id, instance })),
    fields: fields.map(({ id, rules, value }) => ({ id, rules, value })),
  });

  const rustIdentity = commandIdentity('rustc', ['--version']);
  const cargoIdentity = commandIdentity('cargo', ['--version']);
  const dartIdentity = commandIdentity('dart', ['--version']);
  const rustVersion = extractVersion('Rust', rustIdentity, /^rustc\s+(\S+)/u);
  const cargoVersion = extractVersion('Cargo', cargoIdentity, /^cargo\s+(\S+)/u);
  const dartVersion = extractVersion('Dart', dartIdentity, /Dart SDK version:\s+(\S+)/u);

  const adapters = [];
  for (const [id, path] of [['schema-a', AUTHORED], ['schema-b', generatedSchema]]) {
    const schema = decodeJson(await readSafeBytes(ROOT, repoRelative(path)));
    const resolver = new tjsv.SchemaResolver();
    const base = resolver.addDocument(schema, path).base;
    const results = wire.map(row => {
      const verdict = tjsv.validateInstance({
        schema: { $ref: 'ValidationMessage' },
        instance: row.instance,
        resolver,
        base,
        formatAssertion: true,
      });
      requireThat(typeof verdict.valid === 'boolean' && Array.isArray(verdict.errors), 'malformed TJSV verdict');
      requireThat(verdict.valid === (verdict.errors.length === 0), 'inconsistent TJSV verdict');
      return {
        caseId: row.id,
        declaration: 'ValidationMessage',
        verdict: verdict.valid ? 'accepted' : 'rejected',
      };
    });
    adapters.push({
      id,
      language: 'json-schema',
      runtime: 'node',
      validator: 'tjsv',
      toolchain: process.version,
      status: 'passed',
      results,
    });
  }

  const probes = [
    ['rust-native', 'rust', rustIdentity, () => command(resolve(ROOT, 'form-validation/wire-rust/target/debug/examples/tjsv_probe'), [], { input: payload })],
    ['dart-vm', 'dart', dartIdentity, () => command('dart', ['run', 'form-validation/dart/test/tjsv_probe_vm.dart'], { input: payload })],
    ['dart-js', 'dart', `${dartIdentity}; node ${process.versions.node}`, () => {
      command('dart', [
        'compile',
        'js',
        `-DCORPUS_BASE64=${Buffer.from(payload).toString('base64')}`,
        'form-validation/dart/test/tjsv_probe_js.dart',
        '-o',
        resolve(OUT, 'probe.js'),
      ]);
      return command('node', ['-e', 'global.self=global; require(process.argv[1])', resolve(OUT, 'probe.js')]);
    }],
  ];
  const executions = [];
  const observationDigests = {};
  for (const [id, language, toolchain, execute] of probes) {
    const output = inspectObservation(wire, fields, JSON.parse(execute()));
    const observationDigest = digest(Buffer.from(tjsv.canonicalStringify(output)));
    observationDigests[id] = observationDigest;
    adapters.push({
      id,
      language,
      runtime: id,
      validator: 'ores-form-validation-wire/v1',
      toolchain,
      status: 'passed',
      results: output.messages.map(row => ({
        caseId: row.id,
        declaration: 'ValidationMessage',
        verdict: row.accepted ? 'accepted' : 'rejected',
      })),
    });
    executions.push({
      id,
      observationDigest,
      roundTrips: wire.length,
      fieldExecutions: fields.length,
    });
  }

  const requiredAdapters = ['schema-a', 'schema-b', 'rust-native', 'dart-vm', 'dart-js'];
  const evidence = { schema: runtime.RUNTIME_EVIDENCE_SCHEMA, ...binding, corpusDigest, adapters };
  const admit = candidate => runtime.verifyRuntimeEvidenceAgainstCurrentInputs({
    ...current,
    evidence: candidate,
    expectedCorpusDigest: corpusDigest,
    expectedCases,
    requiredAdapters,
    maxFindings: 1000,
  });
  const result = await admit(evidence);
  await save('runtime-evidence.json', evidence);
  await save('runtime-conformance.json', result);
  requireThat(result.status === 'passed' && result.zeroUnexplainedFindings === true, 'runtime contract disagreement');

  const manifest = buildBoundaryManifest(boundary);
  const boundaryEvidence = buildBoundaryEvidence({
    boundary,
    sourceRevision: revision,
    parityRunId: parityReport.runId,
    contractIrId: contractIr.irId,
    observationDigests,
    identities: {
      'rust-native': {
        toolchain: { name: 'rustc', version: rustVersion },
        generator: { name: 'cargo', version: cargoVersion },
      },
      'dart-vm': {
        toolchain: { name: 'dart-vm', version: dartVersion },
        generator: { name: 'dart-run', version: dartVersion },
      },
      'dart-js': {
        toolchain: { name: 'node', version: process.versions.node },
        generator: { name: 'dart-compile-js', version: dartVersion },
      },
    },
  });
  const boundaryInput = {
    manifest,
    report: parityReport,
    contractIr,
    evidenceByPath: boundaryEvidence,
  };
  const boundaryVerification = boundary.verifyLanguageBoundaries(boundaryInput);
  await save('language-boundary-evidence.json', boundaryEvidence);
  await save('language-boundary-verification.json', boundaryVerification);
  requireThat(boundaryVerification.status === 'passed' && boundaryVerification.zeroUnexplainedFindings === true, `TJSV language-boundary verification stopped: ${boundaryVerification.findings.map(row => row.ruleId).join(',')}`);
  requireThat(boundaryVerification.counts.targets === 3 && boundaryVerification.counts.requiredTargets === 3 && boundaryVerification.counts.distinctRequiredLanguages === 2 && boundaryVerification.counts.admittedEvidence === 3 && boundaryVerification.counts.findings === 0, 'TJSV language-boundary coverage is incomplete');

  const boundaryNegative = [];
  for (const [name, expectedRule, mutate] of [
    ['missing-required-evidence', 'boundary-required-evidence-missing', value => { delete value.evidenceByPath['runtime/rust-native.json']; }],
    ['stale-parity-binding', 'boundary-evidence-receipt-mismatch', value => { value.evidenceByPath['runtime/dart-vm.json'].receiptRunId = '0'.repeat(64); }],
    ['generated-authority-promotion', 'boundary-authority-model-invalid', value => { value.manifest.authorities.generatedWitness = 'peer'; }],
    ['disabled-required-ingress', 'boundary-required-ingress-disabled', value => { value.manifest.targets[0].ingress = false; }],
  ]) {
    const candidate = structuredClone(boundaryInput);
    mutate(candidate);
    const rejected = boundary.verifyLanguageBoundaries(candidate);
    const ruleIds = rejected.findings.map(row => row.ruleId);
    requireThat(rejected.status === 'stopped_for_evaluation' && rejected.zeroUnexplainedFindings === false && ruleIds.includes(expectedRule), `TJSV language-boundary verifier accepted or misclassified ${name}`);
    boundaryNegative.push({ name, status: rejected.status, ruleIds });
  }
  await save('language-boundary-negative-controls.json', boundaryNegative);

  // Exercise the actual TJSV runtime-evidence rejection paths; no replacement oracle or fake pass.
  const negative = [];
  for (const [name, mutate] of [
    ['missing-runtime', value => value.adapters.pop()],
    ['missing-case', value => value.adapters[2].results.pop()],
    ['skipped-runtime', value => { value.adapters[2].status = 'skipped'; }],
    ['flipped-verdict', value => { value.adapters[2].results[0].verdict = 'rejected'; }],
    ['stale-corpus', value => { value.corpusDigest = '0'.repeat(64); }],
    ['stale-contract', value => { value.contractIrId = '0'.repeat(64); }],
    ['unrecognized-evidence', value => { value.approved = true; }],
  ]) {
    const candidate = structuredClone(evidence);
    mutate(candidate);
    const rejected = await admit(candidate);
    requireThat(rejected.status !== 'passed' && rejected.findingCount > 0, `TJSV accepted ${name}`);
    negative.push({ name, status: rejected.status, findings: rejected.findingCount });
  }

  const drifted = structuredClone(decodeJson(snapshots['form-validation/contracts/authored.schema.json']));
  drifted.$defs.ValidationMessage.properties.issues.maxItems = 129;
  const driftPath = resolve(OUT, 'drifted.schema.json');
  await writeFile(driftPath, JSON.stringify(drifted), { flag: 'wx' });
  const drift = await tjsv.runCompare({
    typespec: TYPESPEC,
    authoredSchema: driftPath,
    generatedSchema,
    maxFindings: 1000,
    probes: true,
    maxProbes: 64,
    formatAssertion: true,
  });
  requireThat(drift.status === 'stopped_for_evaluation', 'TJSV accepted authored contract drift');
  negative.push({ name: 'authored-contract-drift', status: drift.status, findings: drift.counts.findings });
  await save('negative-controls.json', negative);

  requireThat(git(ROOT, 'rev-parse', 'HEAD') === revision, 'candidate source revision changed during verification');
  await verifyValidatorSource(TJSV, TJSV_REVISION);
  for (const path of SOURCE_PATHS) {
    requireThat(digest(await readSafeBytes(ROOT, path)) === sourceDigests[path], `source changed during verification: ${path}`);
  }

  const receipt = {
    schema: 'ores.form-validation.tjsv-admission/v2',
    status: 'passed',
    sourceRevision: revision,
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: TJSV_REVISION },
    sourceDigests,
    corpusDigest,
    contractIrId: contractIr.irId,
    parityRunId: parityReport.runId,
    languageBoundaryVerificationId: boundaryVerification.verificationId,
    languageBoundary: {
      status: boundaryVerification.status,
      counts: boundaryVerification.counts,
      negativeControls: boundaryNegative.length,
    },
    toolchains: {
      node: process.version,
      rust: rustIdentity,
      cargo: cargoIdentity,
      dart: dartIdentity,
    },
    runtimeObservationDigests: observationDigests,
    coverage: {
      declarations: 3,
      schemaLanes: 2,
      executedRuntimes: 3,
      distinctLanguages: 2,
      boundaryTargets: 3,
      wireCases: wire.length,
      fieldCasesPerRuntime: fields.length,
      negativeControls: negative.length,
      boundaryNegativeControls: boundaryNegative.length,
      browserDomOrLiveServer: false,
      arbitraryProductContractsCovered: false,
      universalEquivalenceProven: false,
    },
    executions,
  };
  await save('admission.json', receipt);
  console.log(JSON.stringify(receipt));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch(error => {
    console.error(error instanceof Error ? error.message : 'form contract verification failed');
    process.exitCode = 3;
  });
}
