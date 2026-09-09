import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { verifyValidatorSource } from '../../scripts/tjsv-source-integrity.mjs';
import { readSafeBytes, ensureEvidenceParents } from '../../scripts/projection-evidence-io.mjs';
import { PROFILES, RUNTIMES, readCases, compareEvidence, requireThat } from './evidence.mjs';
import { buildBoundaryEvidence, buildBoundaryManifest } from './language-boundary.mjs';
import { runTypeScript } from './typescript.mjs';

export const TJSV_REVISION = '4740f1367a7906813dcd420a77d0c9ede26943fb';
const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const PROFILE_ROOT = resolve(ROOT, 'form-validation/admission-profiles');
const sha256 = value => createHash('sha256').update(value).digest('hex');
const execute = (command, args, cwd = ROOT) => execFileSync(command, args, {
  cwd, encoding: 'utf8', timeout: 300000, maxBuffer: 8 * 1024 * 1024,
  stdio: ['ignore', 'pipe', 'pipe'],
});
const commandIdentity = (command, args, cwd = ROOT) => {
  const child = spawnSync(command, args, {
    cwd, encoding: 'utf8', timeout: 300000, maxBuffer: 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  requireThat(child.error === undefined && child.status === 0, `${command} identity probe failed`);
  const identity = `${child.stdout ?? ''}${child.stderr ?? ''}`.trim();
  requireThat(identity.length > 0 && identity.length <= 1024, `${command} identity probe was empty or oversized`);
  return identity;
};
const git = (cwd, ...args) => execute('git', ['-C', cwd, ...args]).trim();
const save = (path, data) => writeFile(path, `${JSON.stringify(data, null, 2)}\n`, { flag: 'wx' });
const extractVersion = (name, identity, expression) => {
  const match = expression.exec(identity);
  requireThat(match !== null && typeof match[1] === 'string' && match[1].length > 0, `unable to parse ${name} version`);
  return match[1];
};

async function main() {
  // Fixed test entrypoint; no command-line options or independent flag parser.
  requireThat(process.argv.length === 2, 'this fixed test accepts no arguments');
  const tjsvRoot = resolve(ROOT, 'tmp/tjsv');
  await verifyValidatorSource(tjsvRoot, TJSV_REVISION);
  const scope = ['form-validation', '.github/workflows/form-profile-admission.yml', 'scripts/tjsv-source-integrity.mjs', 'scripts/projection-evidence-io.mjs'];
  requireThat(git(ROOT, 'status', '--porcelain', '--untracked-files=no', '--', ...scope) === '', 'modified admission sources');
  const revision = git(ROOT, 'rev-parse', 'HEAD');
  // Bind the bytes to HEAD's tree, not an index that can hide local edits.
  const tree = git(ROOT, 'ls-tree', '-rz', '--full-tree', revision, '--', ...scope);
  const snapshots = {};
  for (const entry of tree.split('\0').filter(Boolean)) {
    const match = /^(100644|100755) blob ([a-f0-9]{40})\t(.+)$/u.exec(entry);
    requireThat(match !== null, 'admission source must be a regular tracked file');
    const bytes = await readSafeBytes(ROOT, match[3]);
    requireThat(createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex') === match[2], 'source bytes differ from candidate commit');
    snapshots[match[3]] = bytes;
  }
  const paths = Object.keys(snapshots).sort();
  requireThat(paths.length > 0, 'empty candidate source inventory');
  const digests = Object.fromEntries(paths.map(path => [path, sha256(snapshots[path])]));
  const decodeJson = bytes => JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes));
  const corpusBytes = snapshots['form-validation/admission-profiles/corpus.json'];
  const corpus = decodeJson(corpusBytes);
  const cases = readCases(corpus);
  const authored = decodeJson(snapshots['form-validation/admission-profiles/authored.schema.json']);
  requireThat(Object.keys(authored.$defs).sort().join(',') === [...PROFILES].sort().join(','), 'schema/profile inventory mismatch');
  await ensureEvidenceParents(ROOT, 'tmp/form-profile-admission.json');
  const work = await mkdtemp(resolve(ROOT, 'tmp/form-profile-admission-'));
  const instances = resolve(work, 'instances');
  for (const row of cases) {
    const directory = resolve(instances, row.profile, row.expected ? 'valid' : 'invalid');
    await mkdir(directory, { recursive: true });
    await save(resolve(directory, `${row.id}.json`), row.input);
  }

  const tjsv = await import(pathToFileURL(resolve(tjsvRoot, 'src/index.mjs')).href);
  const boundary = await import(pathToFileURL(resolve(tjsvRoot, 'src/language-boundary-verification.mjs')).href);
  const options = {
    typespec: resolve(PROFILE_ROOT, 'main.tsp'),
    authoredSchema: resolve(PROFILE_ROOT, 'authored.schema.json'),
    outputDir: resolve(work, 'generated'),
    instances, maxFindings: 1000, maxProbes: 64, probes: true,
    formatAssertion: true, sealObjectSchemas: true,
  };
  const parity = await tjsv.runCheck(options);
  await tjsv.writeReport(resolve(work, 'parity.json'), parity);
  requireThat(parity.status === 'passed' && parity.zeroUnexplainedFindings === true, 'TypeSpec/JSON Schema parity failed');
  requireThat(Array.isArray(parity.findings) && parity.findings.length === 0, 'passing parity report did not retain explicit zero findings');
  requireThat(parity.coverage.differentialInstanceValidation === true, 'differential instance validation did not execute');
  requireThat(parity.differential.summary.comparedDeclarations === PROFILES.length && parity.differential.summary.probesEvaluated > 0 && parity.differential.summary.divergences === 0 && parity.differential.summary.refusals === 0, 'incomplete, divergent, or refused differential lane');

  const generatedSchema = resolve(ROOT, parity.inputs.generatedJsonSchema.input);
  const contractIr = await tjsv.buildContractIr({
    report: parity,
    typespec: options.typespec,
    generatedSchema,
    authoredSchema: options.authoredSchema,
  });
  await save(resolve(work, 'contract-ir.json'), contractIr);
  requireThat(contractIr.status === 'passed' && contractIr.admissible === true, 'Contract IR was not admissible');
  requireThat(Array.isArray(contractIr.declarations) && contractIr.declarations.length > 0, 'Contract IR declaration inventory is empty');
  requireThat(Array.isArray(contractIr.excludedDeclarations) && contractIr.excludedDeclarations.length === 0, 'Contract IR excluded declarations cannot cross runtime boundaries');
  requireThat(Array.isArray(contractIr.outOfScopeDeclarations) && contractIr.outOfScopeDeclarations.length === 0, 'Contract IR out-of-scope declarations cannot cross runtime boundaries');

  // Deliberately break only a disposable schema copy. Neither authored authority
  // is rewritten; both structural and behavioral drift must be detected.
  const drift = structuredClone(authored);
  drift.$defs.TextSubmission.properties.value.maxLength = 79;
  const driftPath = resolve(work, 'deliberate-drift.schema.json');
  await save(driftPath, drift);
  const negative = await tjsv.runCheck({ ...options, authoredSchema: driftPath, outputDir: resolve(work, 'negative-generated') });
  await tjsv.writeReport(resolve(work, 'negative-parity.json'), negative);
  requireThat(negative.status === 'stopped_for_evaluation' && negative.differential.summary.divergences > 0, 'TJSV accepted authored contract drift');

  const rustIdentity = commandIdentity('rustc', ['--version']);
  const dartIdentity = commandIdentity('dart', ['--version']);
  const rustVersion = extractVersion('Rust', rustIdentity, /^rustc\s+(\S+)/u);
  const dartVersion = extractVersion('Dart', dartIdentity, /Dart SDK version:\s+(\S+)/u);

  const outputs = {};
  outputs['rust-native'] = execute('cargo', [
    'test', '--manifest-path', 'form-validation/rust/Cargo.toml', '--locked',
    '--test', 'profile_admission', '--', '--nocapture',
  ]);
  const dartRoot = resolve(ROOT, 'form-validation/dart');
  const definition = `-DPROFILE_CORPUS_BASE64=${corpusBytes.toString('base64')}`;
  outputs['dart-vm'] = execute('dart', ['run', definition, 'test/profile_admission.dart'], dartRoot);
  const javascript = resolve(work, 'profile-admission.js');
  execute('dart', ['compile', 'js', definition, 'test/profile_admission.dart', '-o', javascript], dartRoot);
  outputs['dart-javascript'] = execute(process.execPath, ['-e', 'global.self=global; require(process.argv[1]);', javascript]);

  const typescript = await runTypeScript(ROOT, cases);
  outputs['typescript-zod'] = typescript.stdout;

  const resolver = new tjsv.SchemaResolver();
  const base = resolver.addDocument(authored, options.authoredSchema).base;
  const result = compareEvidence(corpus, (profile, input) => {
    const verdict = tjsv.validateInstance({ schema: authored.$defs[profile], instance: input, resolver, base, formatAssertion: true });
    return { valid: verdict.valid, errors: verdict.errors };
  }, outputs);
  requireThat(result.status === 'passed' && result.findings.length === 0, 'runtime contract disagreement');

  const manifest = buildBoundaryManifest(boundary);
  const identities = {
    'rust-native': {
      toolchain: { name: 'rustc', version: rustVersion },
      generator: { name: 'cargo-test-profile-admission', version: rustVersion },
    },
    'dart-vm': {
      toolchain: { name: 'dart-vm', version: dartVersion },
      generator: { name: 'dart-run-profile-admission', version: dartVersion },
    },
    'dart-javascript': {
      toolchain: { name: 'node', version: process.version },
      generator: { name: 'dart-compile-js-profile-admission', version: dartVersion },
    },
    'typescript-zod': {
      toolchain: { name: 'node', version: process.version },
      generator: {
        name: 'typescript-zod-profile-admission',
        version: `typescript-${typescript.toolchain.typescript}+zod-${typescript.toolchain.zod}`,
      },
    },
  };
  const boundaryEvidence = buildBoundaryEvidence({
    boundary,
    sourceRevision: revision,
    parityRunId: parity.runId,
    contractIrId: contractIr.irId,
    outputs,
    identities,
  });
  const boundaryInput = { manifest, report: parity, contractIr, evidenceByPath: boundaryEvidence };
  const boundaryVerification = boundary.verifyLanguageBoundaries(boundaryInput);
  await save(resolve(work, 'language-boundary-evidence.json'), boundaryEvidence);
  await save(resolve(work, 'language-boundary-verification.json'), boundaryVerification);
  requireThat(boundaryVerification.status === 'passed' && boundaryVerification.zeroUnexplainedFindings === true, `TJSV language-boundary verification stopped: ${boundaryVerification.findings.map(row => row.ruleId).join(',')}`);
  requireThat(boundaryVerification.counts.targets === 4 && boundaryVerification.counts.requiredTargets === 4 && boundaryVerification.counts.distinctRequiredLanguages === 3 && boundaryVerification.counts.admittedEvidence === 4 && boundaryVerification.counts.findings === 0, 'TJSV language-boundary coverage is incomplete');

  // Exercise the real upstream boundary verifier, not a local look-alike.
  const boundaryNegativeControls = [];
  for (const [name, expectedRule, mutate] of [
    ['missing-required-evidence', 'boundary-required-evidence-missing', value => { delete value.evidenceByPath['runtime/rust-native.json']; }],
    ['stale-parity-binding', 'boundary-evidence-receipt-mismatch', value => { value.evidenceByPath['runtime/dart-vm.json'].receiptRunId = '0'.repeat(64); }],
    ['generated-authority-promotion', 'boundary-authority-model-invalid', value => { value.manifest.authorities.generatedWitness = 'peer'; }],
    ['disabled-required-egress', 'boundary-required-egress-disabled', value => { value.manifest.targets[0].egress = false; }],
  ]) {
    const candidate = structuredClone(boundaryInput);
    mutate(candidate);
    const rejected = boundary.verifyLanguageBoundaries(candidate);
    const ruleIds = rejected.findings.map(row => row.ruleId);
    requireThat(rejected.status === 'stopped_for_evaluation' && rejected.zeroUnexplainedFindings === false && ruleIds.includes(expectedRule), `TJSV language-boundary verifier accepted or misclassified ${name}`);
    boundaryNegativeControls.push({ name, status: rejected.status, ruleIds });
  }
  await save(resolve(work, 'language-boundary-negative-controls.json'), boundaryNegativeControls);

  // A fabricated accepting result for an invalid case must stop evaluation too.
  const invalid = cases.find(row => !row.expected);
  const mutated = structuredClone(outputs);
  const lines = mutated['dart-vm'].split('\n');
  const index = lines.findIndex(line => line.startsWith('ORES_FORM_ADMISSION='));
  const envelope = JSON.parse(lines[index].slice('ORES_FORM_ADMISSION='.length));
  const badRow = envelope.results.find(row => row.id === invalid.id);
  badRow.accepted = true;
  badRow.preserved = true;
  lines[index] = `ORES_FORM_ADMISSION=${JSON.stringify(envelope)}`;
  mutated['dart-vm'] = lines.join('\n');
  const negativeRuntime = compareEvidence(corpus, (profile, input) => {
    const verdict = tjsv.validateInstance({ schema: authored.$defs[profile], instance: input, resolver, base, formatAssertion: true });
    return { valid: verdict.valid, errors: verdict.errors };
  }, mutated);
  requireThat(negativeRuntime.status === 'stopped_for_evaluation', 'deliberate runtime drift was not detected');

  requireThat(git(ROOT, 'rev-parse', 'HEAD') === revision, 'source revision changed during admission');
  await verifyValidatorSource(tjsvRoot, TJSV_REVISION);
  for (const path of paths) requireThat(sha256(await readSafeBytes(ROOT, path)) === digests[path], 'admission source changed during execution');
  const receipt = {
    schema: 'ores.form-admission.receipt/v2', sourceRevision: revision,
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: TJSV_REVISION },
    corpusDigest: sha256(corpusBytes), sourceDigests: digests,
    parityRunId: parity.runId,
    contractIrId: contractIr.irId,
    languageBoundaryVerificationId: boundaryVerification.verificationId,
    languageBoundary: {
      status: boundaryVerification.status,
      counts: boundaryVerification.counts,
      negativeControls: boundaryNegativeControls.length,
    },
    negativeEvidence: { schemaRunId: negative.runId, schemaDivergences: negative.differential.summary.divergences, runtimeDrift: negativeRuntime.status },
    toolchains: { node: process.version, rust: rustIdentity, dart: dartIdentity, typescript: typescript.toolchain },
    typescriptArtifactDigest: typescript.emittedDigest,
    runtimeOutputDigests: Object.fromEntries(RUNTIMES.map(name => [name, sha256(outputs[name])])),
    coverage: { profiles: PROFILES, fixtures: cases.length, runtimes: RUNTIMES, distinctLanguages: 3, boundaryTargets: 4, universalEquivalenceProven: false, scope: 'representative-json-value-admission-not-fleet-rollout' },
    ...result,
  };
  await save(resolve(work, 'receipt.json'), receipt);
  console.log(JSON.stringify({ status: receipt.status, fixtures: cases.length, profiles: PROFILES.length, runtimes: RUNTIMES, boundaryVerificationId: receipt.languageBoundaryVerificationId, receipt: resolve(work, 'receipt.json') }));
  return receipt.status === 'passed' ? 0 : 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().then(code => { process.exitCode = code; }).catch(error => {
    // Do not log a child process command/argv or captured raw runtime output.
    console.error(JSON.stringify({ status: 'failed', error: error?.status !== undefined ? 'runtime/compiler process failed' : error.message }));
    process.exitCode = 3;
  });
}
