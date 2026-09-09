import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { verifyValidatorSource } from '../../scripts/tjsv-source-integrity.mjs';
import { readSafeBytes, ensureEvidenceParents } from '../../scripts/projection-evidence-io.mjs';
import { PROFILES, RUNTIMES, readCases, compareEvidence, requireThat } from './evidence.mjs';
import { runTypeScript } from './typescript.mjs';

export const TJSV_REVISION = 'd60d0d79d83e075077382623ec9e23a401ab601f';
const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const PROFILE_ROOT = resolve(ROOT, 'form-validation/admission-profiles');
const sha256 = value => createHash('sha256').update(value).digest('hex');
const execute = (command, args, cwd = ROOT) => execFileSync(command, args, {
  cwd, encoding: 'utf8', timeout: 300000, maxBuffer: 8 * 1024 * 1024,
  stdio: ['ignore', 'pipe', 'pipe'],
});
const git = (cwd, ...args) => execute('git', ['-C', cwd, ...args]).trim();
const save = (path, data) => writeFile(path, `${JSON.stringify(data, null, 2)}\n`, { flag: 'wx' });

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
  const options = {
    typespec: resolve(PROFILE_ROOT, 'main.tsp'),
    authoredSchema: resolve(PROFILE_ROOT, 'authored.schema.json'),
    outputDir: resolve(work, 'generated'),
    instances, maxFindings: 1000, maxProbes: 64, probes: true,
    formatAssertion: true, sealObjectSchemas: true,
  };
  const parity = await tjsv.runCheck(options);
  await tjsv.writeReport(resolve(work, 'parity.json'), parity);
  requireThat(parity.status === 'passed' && parity.coverage.differentialInstanceValidation === true, 'TypeSpec/JSON Schema parity failed');
  requireThat(parity.differential.summary.comparedDeclarations === PROFILES.length && parity.differential.summary.refusals === 0, 'incomplete or refused differential lane');

  // Deliberately break only a disposable schema copy. Neither authored authority
  // is rewritten; both structural and behavioral drift must be detected.
  const drift = structuredClone(authored);
  drift.$defs.TextSubmission.properties.value.maxLength = 79;
  const driftPath = resolve(work, 'deliberate-drift.schema.json');
  await save(driftPath, drift);
  const negative = await tjsv.runCheck({ ...options, authoredSchema: driftPath, outputDir: resolve(work, 'negative-generated') });
  await tjsv.writeReport(resolve(work, 'negative-parity.json'), negative);
  requireThat(negative.status === 'stopped_for_evaluation' && negative.differential.summary.divergences > 0, 'deliberate schema drift was not detected');

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
    schema: 'ores.form-admission.receipt/v1', sourceRevision: revision,
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: TJSV_REVISION },
    corpusDigest: sha256(corpusBytes), sourceDigests: digests,
    parityRunId: parity.runId,
    negativeEvidence: { schemaRunId: negative.runId, schemaDivergences: negative.differential.summary.divergences, runtimeDrift: negativeRuntime.status },
    toolchains: { node: process.version, rust: execute('rustc', ['--version']).trim(), dart: execute('dart', ['--version']).trim(), typescript: typescript.toolchain },
    typescriptArtifactDigest: typescript.emittedDigest,
    runtimeOutputDigests: Object.fromEntries(RUNTIMES.map(name => [name, sha256(outputs[name])])),
    coverage: { profiles: PROFILES, fixtures: cases.length, runtimes: RUNTIMES, universalEquivalenceProven: false, scope: 'representative-json-value-admission-not-fleet-rollout' },
    ...result,
  };
  await save(resolve(work, 'receipt.json'), receipt);
  console.log(JSON.stringify({ status: receipt.status, fixtures: cases.length, profiles: PROFILES.length, runtimes: RUNTIMES, receipt: resolve(work, 'receipt.json') }));
  return receipt.status === 'passed' ? 0 : 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().then(code => { process.exitCode = code; }).catch(error => {
    // Do not log a child process command/argv or captured raw runtime output.
    console.error(JSON.stringify({ status: 'failed', error: error?.status !== undefined ? 'runtime/compiler process failed' : error.message }));
    process.exitCode = 3;
  });
}
