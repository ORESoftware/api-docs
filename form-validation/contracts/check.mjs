import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { fieldCases, inspectObservation, requireThat, wireCases } from './corpus.mjs';

export const TJSV_REVISION = '6bb5b7c1ee41c8b43741e50a264c33a1165549c4';
const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const OUT = resolve(ROOT, 'tmp/tjsv-form');
const TJSV = resolve(ROOT, 'tmp/tjsv');
const TYPESPEC = resolve(ROOT, 'form-validation/contracts/main.tsp');
const AUTHORED = resolve(ROOT, 'form-validation/contracts/authored.schema.json');
const SOURCE_PATHS = [
  'form-validation/contracts/main.tsp', 'form-validation/contracts/authored.schema.json',
  'form-validation/contracts/check.mjs', 'form-validation/contracts/corpus.mjs', 'form-validation/contracts/check.test.mjs',
  'form-validation/wire-rust/Cargo.toml', 'form-validation/wire-rust/Cargo.lock',
  'form-validation/wire-rust/src/lib.rs', 'form-validation/wire-rust/examples/tjsv_probe.rs',
  'form-validation/rust/Cargo.toml', 'form-validation/rust/src/lib.rs', 'form-validation/fixtures.json',
  'form-validation/dart/pubspec.yaml', 'form-validation/dart/pubspec.lock',
  'form-validation/dart/lib/ores_form_validation.dart', 'form-validation/dart/lib/validation_message.dart',
  'form-validation/dart/test/shared.dart', 'form-validation/dart/test/tjsv_probe_shared.dart',
  'form-validation/dart/test/tjsv_probe_vm.dart', 'form-validation/dart/test/tjsv_probe_js.dart',
  '.github/workflows/tjsv-form-contract.yml',
];
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function command(executable, args, options = {}) {
  return execFileSync(executable, args, { cwd: ROOT, encoding: 'utf8', maxBuffer: 4 * 1024 * 1024,
    timeout: 180000, stdio: ['pipe', 'pipe', 'pipe'], ...options }).trim();
}
const git = (cwd, ...args) => command('git', ['-C', cwd, ...args]);
async function save(name, value) { await writeFile(resolve(OUT, name), `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' }); }
async function sourceDigests() {
  return Object.fromEntries(await Promise.all(SOURCE_PATHS.map(async path => [path, digest(await readFile(resolve(ROOT, path)))])));
}

export async function main() {
  requireThat(process.argv.length === 2, 'fixed CI entrypoint accepts no arguments');
  process.chdir(ROOT);
  requireThat(git(TJSV, 'rev-parse', 'HEAD') === TJSV_REVISION, 'wrong TJSV revision');
  requireThat(git(TJSV, 'status', '--porcelain', '--untracked-files=no') === '', 'modified TJSV checkout');
  await mkdir(OUT, { recursive: true });
  const before = await sourceDigests();
  const tjsv = await import(pathToFileURL(resolve(TJSV, 'src/index.mjs')).href);
  const runtime = await import(pathToFileURL(resolve(TJSV, 'src/runtime-conformance/index.mjs')).href);
  const fields = fieldCases(JSON.parse(await readFile(resolve(ROOT, 'form-validation/fixtures.json'), 'utf8')));
  const wire = wireCases(fields);
  for (const row of wire) {
    const dir = resolve(OUT, 'instances/ValidationMessage', row.expected ? 'valid' : 'invalid');
    await mkdir(dir, { recursive: true });
    await writeFile(resolve(dir, `${row.id}.json`), JSON.stringify(row.instance), { flag: 'wx' });
  }
  const parityReport = await tjsv.runCheck({
    typespec: TYPESPEC, authoredSchema: AUTHORED, outputDir: resolve(OUT, 'witness'),
    bundleId: 'form-validation.json', sealObjectSchemas: true, maxFindings: 1000,
    instances: resolve(OUT, 'instances'), probes: true, maxProbes: 64, formatAssertion: true,
  });
  await save('parity.json', parityReport);
  requireThat(parityReport.status === 'passed' && parityReport.zeroUnexplainedFindings === true, 'TypeSpec/authored JSON Schema disagreement');
  requireThat(parityReport.declarationMap.length === 3, 'incomplete declaration coverage');
  const generatedSchema = resolve(ROOT, parityReport.inputs.generatedJsonSchema.input);
  const contractIr = await tjsv.buildContractIr({ report: parityReport, typespec: TYPESPEC, generatedSchema, authoredSchema: AUTHORED });
  await save('contract-ir.json', contractIr);
  const current = { contractIr, parityReport, typespec: TYPESPEC, generatedSchema, authoredSchema: AUTHORED };
  const binding = await runtime.createRuntimeEvidenceBindingAgainstCurrentInputs(current);
  const corpusDigest = digest(tjsv.canonicalStringify({ wire, fields }));
  const expectedCases = wire.map(row => ({ id: row.id, declaration: 'ValidationMessage', expectation: row.expected ? 'accepted' : 'rejected' }));
  const payload = JSON.stringify({
    messages: wire.map(({ id, instance }) => ({ id, instance })),
    fields: fields.map(({ id, rules, value }) => ({ id, rules, value })),
  });
  const adapters = [];
  for (const [id, path] of [['schema-a', AUTHORED], ['schema-b', generatedSchema]]) {
    const schema = JSON.parse(await readFile(path, 'utf8'));
    const resolver = new tjsv.SchemaResolver();
    const base = resolver.addDocument(schema, path).base;
    const results = wire.map(row => {
      const verdict = tjsv.validateInstance({ schema: { $ref: 'ValidationMessage' }, instance: row.instance, resolver, base, formatAssertion: true });
      requireThat(typeof verdict.valid === 'boolean' && Array.isArray(verdict.errors), 'malformed TJSV verdict');
      requireThat(verdict.valid === (verdict.errors.length === 0), 'inconsistent TJSV verdict');
      return { caseId: row.id, declaration: 'ValidationMessage', verdict: verdict.valid ? 'accepted' : 'rejected' };
    });
    adapters.push({ id, language: 'json-schema', runtime: 'node', validator: 'tjsv', toolchain: process.version, status: 'passed', results });
  }
  // Real independently executed codecs, not expected results turned into receipts.
  const probes = [
    ['rust-native', 'rust', command('rustc', ['--version']), () => command(resolve(ROOT, 'form-validation/wire-rust/target/debug/examples/tjsv_probe'), [], { input: payload })],
    ['dart-vm', 'dart', command('dart', ['--version']), () => command('dart', ['run', 'form-validation/dart/test/tjsv_probe_vm.dart'], { input: payload })],
    ['dart-js', 'dart', `${command('dart', ['--version'])}; ${process.version}`, () => {
      command('dart', ['compile', 'js', `-DCORPUS_BASE64=${Buffer.from(payload).toString('base64')}`, 'form-validation/dart/test/tjsv_probe_js.dart', '-o', resolve(OUT, 'probe.js')]);
      return command('node', ['-e', 'global.self=global; require(process.argv[1])', resolve(OUT, 'probe.js')]);
    }],
  ];
  const executions = [];
  for (const [id, language, toolchain, execute] of probes) {
    const output = inspectObservation(wire, fields, JSON.parse(execute()));
    adapters.push({ id, language, runtime: id, validator: 'ores-form-validation-wire/v1', toolchain,
      status: 'passed', results: output.messages.map(row => ({ caseId: row.id, declaration: 'ValidationMessage', verdict: row.accepted ? 'accepted' : 'rejected' })) });
    executions.push({ id, observationDigest: digest(tjsv.canonicalStringify(output)), roundTrips: wire.length, fieldExecutions: fields.length });
  }
  const requiredAdapters = ['schema-a', 'schema-b', 'rust-native', 'dart-vm', 'dart-js'];
  const evidence = { schema: runtime.RUNTIME_EVIDENCE_SCHEMA, ...binding, corpusDigest, adapters };
  const admit = candidate => runtime.verifyRuntimeEvidenceAgainstCurrentInputs({ ...current, evidence: candidate,
    expectedCorpusDigest: corpusDigest, expectedCases, requiredAdapters, maxFindings: 1000 });
  const result = await admit(evidence);
  await save('runtime-evidence.json', evidence);
  await save('runtime-conformance.json', result);
  requireThat(result.status === 'passed' && result.zeroUnexplainedFindings === true, 'runtime contract disagreement');

  // Exercise the actual TJSV rejection paths; no replacement oracle or fake pass.
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
  const drifted = JSON.parse(await readFile(AUTHORED, 'utf8'));
  drifted.$defs.ValidationMessage.properties.issues.maxItems = 129;
  const driftPath = resolve(OUT, 'drifted.schema.json');
  await writeFile(driftPath, JSON.stringify(drifted), { flag: 'wx' });
  const drift = await tjsv.runCompare({ typespec: TYPESPEC, authoredSchema: driftPath, generatedSchema,
    maxFindings: 1000, probes: true, maxProbes: 64, formatAssertion: true });
  requireThat(drift.status === 'stopped_for_evaluation', 'TJSV accepted authored contract drift');
  negative.push({ name: 'authored-contract-drift', status: drift.status, findings: drift.counts.findings });
  await save('negative-controls.json', negative);
  const after = await sourceDigests();
  requireThat(tjsv.canonicalStringify(before) === tjsv.canonicalStringify(after), 'source changed during verification');
  requireThat(git(TJSV, 'rev-parse', 'HEAD') === TJSV_REVISION && git(TJSV, 'status', '--porcelain', '--untracked-files=no') === '', 'TJSV changed during verification');
  const receipt = {
    schema: 'ores.form-validation.tjsv-admission/v1', status: 'passed', sourceRevision: git(ROOT, 'rev-parse', 'HEAD'),
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: TJSV_REVISION },
    sourceDigests: before, corpusDigest, contractIrId: contractIr.irId, parityRunId: parityReport.runId,
    coverage: { declarations: 3, schemaLanes: 2, executedRuntimes: 3, wireCases: wire.length,
      fieldCasesPerRuntime: fields.length, negativeControls: negative.length, browserDomOrLiveServer: false,
      arbitraryProductContractsCovered: false, universalEquivalenceProven: false }, executions,
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
