import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const TJSV_REVISION = '4473504c4c9d2831d825919f70c03994d8ce01d2';
export const PROFILE = 'ores-rpc-v1-call-receipt';
const ROOT = fileURLToPath(new URL('../', import.meta.url));
const INPUTS = Object.freeze([
  'examples/rpc-v1/conformance.json',
  'json-schema/rpc-call.schema.json',
  'json-schema/rpc-receipt.schema.json',
  'idl/typespec/v1.tsp',
  'runtime/v1-conformance.json',
  'clients/typescript/src/rpc.js',
  'scripts/tjsv-rpc-admission.mjs',
]);
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const requireThat = (condition, message) => { if (!condition) throw new Error(message); };

/** Validate the corpus before calling either oracle: invalid metadata is not a rejection. */
export function readCases(corpus) {
  requireThat(object(corpus), 'corpus must be an object');
  const fields = new Set(['schemaVersion', 'profile', 'maxFrameBytes', 'tcpLengthPrefixBytes', 'valid', 'invalid']);
  requireThat(Object.keys(corpus).every(key => fields.has(key)), 'unknown corpus field');
  requireThat(corpus.schemaVersion === 1 && corpus.profile === PROFILE, 'unsupported corpus profile');
  requireThat(corpus.maxFrameBytes === 8388608 && corpus.tcpLengthPrefixBytes === 4, 'unexpected framing contract');
  const seen = new Set();
  const rows = [];
  for (const group of ['valid', 'invalid']) {
    requireThat(Array.isArray(corpus[group]) && corpus[group].length > 0, `${group} corpus must not be empty`);
    const kinds = new Set();
    for (const entry of corpus[group]) {
      requireThat(object(entry), 'fixture must be an object');
      const allowed = new Set(['name', 'kind', 'encoded', ...(group === 'valid' ? ['tcp_prefix_hex'] : [])]);
      requireThat(Object.keys(entry).every(key => allowed.has(key)), 'unknown fixture field');
      requireThat(typeof entry.name === 'string' && /^[a-z0-9][a-z0-9-]*$/.test(entry.name), 'invalid fixture name');
      requireThat(!seen.has(entry.name), `duplicate fixture ${entry.name}`);
      seen.add(entry.name);
      requireThat(entry.kind === 'call' || entry.kind === 'receipt', `unknown fixture kind ${entry.kind}`);
      kinds.add(entry.kind);
      requireThat(typeof entry.encoded === 'string', `missing JSON for ${entry.name}`);
      const bytes = Buffer.byteLength(entry.encoded, 'utf8');
      requireThat(bytes > 0 && bytes <= corpus.maxFrameBytes, `fixture size outside profile: ${entry.name}`);
      // This corpus describes JSON-value admission, not malformed-wire parsing.
      const instance = JSON.parse(entry.encoded);
      if (group === 'valid') {
        requireThat(typeof entry.tcp_prefix_hex === 'string' && /^[0-9a-f]{8}$/.test(entry.tcp_prefix_hex), 'invalid TCP prefix');
        requireThat(Number.parseInt(entry.tcp_prefix_hex, 16) === bytes, `incorrect TCP prefix: ${entry.name}`);
      }
      rows.push({ name: entry.name, kind: entry.kind, encoded: entry.encoded, instance, expected: group === 'valid' });
    }
    requireThat(kinds.size === 2, `${group} corpus must cover call and receipt`);
  }
  return rows;
}

/** Compare real validator verdicts. Exceptions never masquerade as schema rejection. */
export function compareCorpus(corpus, validate, decode, isRuntimeRejection) {
  requireThat(typeof validate === 'function' && typeof isRuntimeRejection === 'function', 'missing validator adapter');
  requireThat(object(decode) && typeof decode.call === 'function' && typeof decode.receipt === 'function', 'missing runtime adapter');
  const results = [];
  const findings = [];
  for (const row of readCases(corpus)) {
    const verdict = validate(row.kind, row.instance);
    requireThat(object(verdict) && typeof verdict.valid === 'boolean' && Array.isArray(verdict.errors), 'malformed TJSV verdict');
    requireThat(verdict.valid === (verdict.errors.length === 0), 'inconsistent TJSV verdict');
    let runtimeAccepted = true;
    try {
      const decoded = decode[row.kind](row.encoded);
      requireThat(object(decoded) && isDeepStrictEqual(decoded, row.instance), `runtime changed decoded value: ${row.name}`);
    }
    catch (error) {
      if (!isRuntimeRejection(error)) throw error;
      runtimeAccepted = false;
    }
    const result = { name: row.name, kind: row.kind, expected: row.expected, tjsvAccepted: verdict.valid, typescriptAccepted: runtimeAccepted };
    results.push(result);
    if (verdict.valid !== row.expected || runtimeAccepted !== row.expected) findings.push(result);
  }
  return { status: findings.length === 0 ? 'passed' : 'stopped_for_evaluation', results, findings };
}

function git(cwd, ...args) {
  return execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

export async function main() {
  // This fixed CI entrypoint has no command-line options or independent flag parser.
  requireThat(process.argv.length === 2, 'this fixed admission entrypoint accepts no arguments');
  const validatorRoot = resolve(ROOT, 'tmp/tjsv');
  requireThat(git(validatorRoot, 'rev-parse', 'HEAD') === TJSV_REVISION, 'TJSV checkout does not match the reviewed pin');
  requireThat(git(validatorRoot, 'status', '--porcelain', '--untracked-files=no') === '', 'TJSV tracked files are modified');
  const snapshots = Object.fromEntries(await Promise.all(INPUTS.map(async path => [path, await readFile(resolve(ROOT, path))])));
  const sourceDigests = Object.fromEntries(INPUTS.map(path => [path, sha256(snapshots[path])]));
  const tjsv = await import(pathToFileURL(resolve(validatorRoot, 'src/index.mjs')).href);
  const runtime = await import(pathToFileURL(resolve(ROOT, 'clients/typescript/src/rpc.js')).href);
  const schemas = Object.fromEntries(['call', 'receipt'].map(kind => [kind, JSON.parse(snapshots[`json-schema/rpc-${kind}.schema.json`])]));
  const resolver = new tjsv.SchemaResolver();
  const bases = {};
  for (const [kind, schema] of Object.entries(schemas)) {
    requireThat(object(schema) && schema.$schema === 'https://json-schema.org/draft/2020-12/schema', `wrong schema dialect: ${kind}`);
    const findings = tjsv.validateJsonSchemaDocument(schema, `rpc-${kind}.schema.json`);
    requireThat(Array.isArray(findings) && findings.length === 0, `invalid authored schema: ${kind}`);
    bases[kind] = resolver.addDocument(schema, resolve(ROOT, `json-schema/rpc-${kind}.schema.json`)).base;
  }
  const result = compareCorpus(
    JSON.parse(snapshots['examples/rpc-v1/conformance.json']),
    (kind, instance) => tjsv.validateInstance({ schema: schemas[kind], instance, resolver, base: bases[kind], formatAssertion: true }),
    { call: runtime.decodeCall, receipt: runtime.decodeReceipt },
    error => error instanceof runtime.RpcV1Error,
  );
  for (const path of INPUTS) requireThat(sha256(await readFile(resolve(ROOT, path))) === sourceDigests[path], `source changed during admission: ${path}`);
  const report = {
    schema: 'ores.api-docs.tjsv-rpc-admission/v1',
    profile: PROFILE,
    sourceRevision: git(ROOT, 'rev-parse', 'HEAD'),
    validator: { repository: 'ORESoftware/typespec-json-schema-validator', revision: TJSV_REVISION },
    sourceDigests,
    coverage: {
      scope: 'authored-json-schema-versus-typescript-rpc-v1-fixtures',
      fixtures: result.results.length,
      otherRuntimeExecution: 'separate-four-language-conformance-workflow',
      typeSpecParity: 'separate-peer-authority-and-projection-gates',
      universalEquivalenceProven: false,
    },
    ...result,
  };
  const destination = resolve(ROOT, 'tmp/tjsv-rpc-admission.json');
  await mkdir(dirname(destination), { recursive: true });
  await writeFile(destination, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  console.log(JSON.stringify(report));
  return result.status === 'passed' ? 0 : 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().then(code => { process.exitCode = code; }).catch(error => {
    console.error(JSON.stringify({ status: 'failed', error: error instanceof Error ? error.message : String(error) }));
    process.exitCode = 3;
  });
}
