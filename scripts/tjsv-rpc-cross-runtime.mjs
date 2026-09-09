import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFile, writeFile, lstat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { main as runOracle, readCases, TJSV_REVISION } from './tjsv-rpc-admission.mjs';
import {
  REQUEST_SCHEMA, RESPONSE_SCHEMA, NATIVE_RUNTIMES, ORACLE_INPUTS, requireThat,
  makeProbeRequest, assessProbeResponse, invokeProbe, readProbeExecution, verifyOracleReceipt,
} from './tjsv-rpc-runtime-protocol.mjs';

const ROOT = fileURLToPath(new URL('../', import.meta.url));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const git = (...args) => execFileSync('git', ['-C', ROOT, ...args], { encoding: 'utf8' }).trim();
const SOURCE_PATHS = [
  'clients', 'rust', 'Cargo.toml', 'Cargo.lock',
  ...ORACLE_INPUTS,
  'scripts/tjsv-rpc-runtime-protocol.mjs', 'scripts/test-tjsv-rpc-entrypoint.mjs',
  'scripts/tjsv-rpc-cross-runtime.mjs', 'scripts/test_tjsv_rpc_runtime_protocol.mjs',
  '.github/workflows/tjsv-rpc-cross-runtime.yml', '.github/workflows/tjsv-rpc-admission.yml',
];
async function digestFile(path) {
  const absolute = resolve(ROOT, path);
  const stat = await lstat(absolute);
  requireThat(stat.isFile() && !stat.isSymbolicLink(), `not a regular evidence file: ${path}`);
  return hash(await readFile(absolute));
}
async function snapshotSources() {
  const paths = git('ls-files', '-z', '--', ...SOURCE_PATHS).split('\0').filter(Boolean).sort();
  requireThat(paths.length > 0, 'missing tracked sources');
  return Object.fromEntries(await Promise.all(paths.map(async path => [path, await digestFile(path)])));
}
function observeTypeScript(rows, client) {
  return {
    schema: RESPONSE_SCHEMA, runtime: 'typescript',
    results: rows.map(row => {
      let decoded;
      try { decoded = (row.kind === 'call' ? client.decodeCall : client.decodeReceipt)(row.encoded); }
      catch (error) {
        if (!(error instanceof client.RpcV1Error)) throw error;
        return { name: row.name, kind: row.kind, accepted: false };
      }
      // The real encoder runs outside the rejection handler.
      return { name: row.name, kind: row.kind, accepted: true, encoded: client.toNdjson(decoded) };
    }),
  };
}
function nativeResponse(runtime, rows) {
  return readProbeExecution(invokeProbe(resolve(ROOT, `tmp/tjsv-probes/${runtime}`), makeProbeRequest(rows)));
}
function protocolControls(row) {
  const valid = JSON.parse(makeProbeRequest([row]));
  const first = valid.cases[0];
  return [
    'null', '{',
    JSON.stringify({ ...valid, schema: 'wrong' }),
    JSON.stringify({ schema: REQUEST_SCHEMA, cases: [] }),
    JSON.stringify({ ...valid, expected: true }),
    JSON.stringify({ ...valid, cases: [{ ...first, kind: 'data' }] }),
    JSON.stringify({ ...valid, cases: [{ ...first, expected: true }] }),
    JSON.stringify({ ...valid, cases: [{ ...first, encoded: null }] }),
    JSON.stringify({ ...valid, cases: [first, first] }),
  ];
}

export async function main() {
  requireThat(process.argv.length === 2, 'fixed runtime entrypoint accepts no arguments');
  requireThat(git('status', '--porcelain', '--untracked-files=no') === '', 'source checkout is not clean');
  const revision = git('rev-parse', 'HEAD');
  const sourceDigests = await snapshotSources();
  const executableDigests = Object.fromEntries(await Promise.all(NATIVE_RUNTIMES.map(async runtime => [runtime, await digestFile(`tmp/tjsv-probes/${runtime}`)])));
  const oracleExit = await runOracle();
  if (oracleExit !== 0) return oracleExit;
  const oracleBytes = await readFile(resolve(ROOT, 'tmp/tjsv-rpc-admission.json'));
  const oracle = JSON.parse(oracleBytes);
  const rows = readCases(JSON.parse(await readFile(resolve(ROOT, 'examples/rpc-v1/conformance.json'))));
  verifyOracleReceipt(oracle, rows, revision, sourceDigests, TJSV_REVISION);

  const client = await import(pathToFileURL(resolve(ROOT, 'clients/typescript/src/rpc.js')).href);
  const observations = { typescript: observeTypeScript(rows, client) };
  for (const runtime of NATIVE_RUNTIMES) observations[runtime] = nativeResponse(runtime, rows);
  const runtimes = ['typescript', ...NATIVE_RUNTIMES];
  const direct = Object.fromEntries(runtimes.map(runtime => [runtime, assessProbeResponse(rows, observations[runtime], runtime)]));
  const findings = Object.entries(direct).flatMap(([runtime, result]) => result.findings.map(finding => ({ phase: 'direct', runtime, ...finding })));
  const cross = {};
  let producerEncodings = 0;
  let protocolRejections = 0;
  if (findings.length === 0) {
    // Decode every valid encoder output with every real runtime. The payloads
    // remain equal to the independently TJSV-validated JSON instances above.
    const crossed = runtimes.flatMap(producer => observations[producer].results.flatMap((result, index) => rows[index].expected ? [{
      name: `${producer}-${rows[index].name}`, kind: rows[index].kind,
      encoded: result.encoded, instance: rows[index].instance, expected: true,
    }] : []));
    producerEncodings = crossed.length;
    for (const runtime of runtimes) {
      const response = runtime === 'typescript' ? observeTypeScript(crossed, client) : nativeResponse(runtime, crossed);
      cross[runtime] = assessProbeResponse(crossed, response, runtime);
      findings.push(...cross[runtime].findings.map(finding => ({ phase: 'cross-encoding', runtime, ...finding })));
    }
    for (const runtime of NATIVE_RUNTIMES) {
      for (const request of protocolControls(rows[0])) {
        const execution = invokeProbe(resolve(ROOT, `tmp/tjsv-probes/${runtime}`), request);
        requireThat(!execution.error && !execution.signal && execution.status === 3 && execution.stdout === '', `${runtime} did not fail closed on malformed probe input`);
        protocolRejections += 1;
      }
    }
  }
  requireThat(git('rev-parse', 'HEAD') === revision && git('status', '--porcelain', '--untracked-files=no') === '', 'source revision or tracked files changed');
  const finalDigests = await snapshotSources();
  requireThat(JSON.stringify(finalDigests) === JSON.stringify(sourceDigests), 'source bytes changed during execution');
  for (const runtime of NATIVE_RUNTIMES) requireThat(await digestFile(`tmp/tjsv-probes/${runtime}`) === executableDigests[runtime], 'probe executable changed during execution');
  requireThat(hash(await readFile(resolve(ROOT, 'tmp/tjsv-rpc-admission.json'))) === hash(oracleBytes), 'oracle receipt changed during execution');
  const report = {
    schema: 'ores.api-docs.tjsv-cross-runtime/v1',
    status: findings.length ? 'stopped_for_evaluation' : 'passed',
    sourceRevision: revision, validatorRevision: TJSV_REVISION,
    oracleReceiptSha256: hash(oracleBytes), sourceDigests, executableDigests,
    coverage: {
      runtimes, fixtureCases: rows.length, directDecoderVerdicts: rows.length * runtimes.length,
      producerEncodings, crossDecoderVerdicts: producerEncodings * runtimes.length, protocolRejections,
      typeSpecParity: 'existing-independent-authority-gates-not-proven-by-this-runner',
      liveTransportIO: false, universalEquivalenceProven: false,
    },
    direct, cross, findings,
  };
  await writeFile(resolve(ROOT, 'tmp/tjsv-cross-runtime.json'), `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  console.log(JSON.stringify({ status: report.status, coverage: report.coverage, findings }));
  return findings.length ? 2 : 0;
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().then(code => { process.exitCode = code; }).catch(error => {
    console.error(JSON.stringify({ status: 'failed', error: error instanceof Error ? error.message : String(error) }));
    process.exitCode = 3;
  });
}
