import { spawnSync } from 'node:child_process';
import { isDeepStrictEqual } from 'node:util';

export const REQUEST_SCHEMA = 'ores.api-docs.rpc-probe/v1';
export const RESPONSE_SCHEMA = 'ores.api-docs.rpc-probe-result/v1';
export const MAX_PROTOCOL_BYTES = 16 * 1024 * 1024;
export const NATIVE_RUNTIMES = Object.freeze(['rust', 'go', 'dart']);
// The same closed manifest drives source snapshots and receipt verification.
// The Rust client oracle executes through the root workspace and therefore
// binds the complete tracked Rust client/core closure in addition to the
// independently authored contracts, helpers, tests, and exact workflow.
export const ORACLE_INPUTS = Object.freeze([
  '.github/workflows/tjsv-rpc-admission.yml',
  'Cargo.lock',
  'Cargo.toml',
  'clients/rust/Cargo.toml',
  'clients/rust/README.md',
  'clients/rust/examples/tjsv_admission.rs',
  'clients/rust/examples/tjsv_rpc_probe.rs',
  'clients/rust/src/lib.rs',
  'clients/rust/tests/client.rs',
  'clients/rust/tests/client_contract.rs',
  'clients/typescript/src/rpc.js',
  'examples/rpc-v1/conformance.json',
  'idl/typespec/v1.tsp',
  'json-schema/rpc-call.schema.json',
  'json-schema/rpc-receipt.schema.json',
  'runtime/v1-conformance.json',
  'rust/Cargo.toml',
  'rust/src/axum_router.rs',
  'rust/src/bin/authority_evidence.rs',
  'rust/src/binding.rs',
  'rust/src/call.rs',
  'rust/src/catalog.rs',
  'rust/src/discovery.rs',
  'rust/src/headers.rs',
  'rust/src/html.rs',
  'rust/src/infer.rs',
  'rust/src/lib.rs',
  'rust/src/map.rs',
  'rust/src/opto_sync.rs',
  'rust/src/paths.rs',
  'rust/src/project.rs',
  'rust/src/rpc_v1.rs',
  'rust/src/rpc_v1/decode.rs',
  'rust/src/rpc_v1/helpers.rs',
  'rust/src/rpc_v1/receipt.rs',
  'rust/src/rpc_v1/tests.rs',
  'rust/src/rpc_v1/types.rs',
  'rust/src/schema.rs',
  'rust/src/telemetry.rs',
  'rust/src/template.rs',
  'rust/tests/e2e_transports.rs',
  'rust/tests/queued_delete.rs',
  'rust/tests/rpc_v1_duplicate_envelopes.rs',
  'scripts/projection-evidence-io.mjs',
  'scripts/test-projection-evidence-io.mjs',
  'scripts/test-tjsv-rpc-entrypoint.mjs',
  'scripts/test_tjsv_rpc_admission.mjs',
  'scripts/test_tjsv_rust_admission.mjs',
  'scripts/tjsv-rpc-admission.mjs',
  'scripts/tjsv-rust-admission.mjs',
  'scripts/tjsv-source-integrity.mjs',
]);
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
export function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}
function exactKeys(value, keys, label) {
  requireThat(object(value) && isDeepStrictEqual(Object.keys(value).sort(), [...keys].sort()), `invalid ${label} fields`);
}

/** Expected verdicts and fixture groups are deliberately never sent to a decoder. */
export function makeProbeRequest(rows) {
  requireThat(Array.isArray(rows) && rows.length > 0 && rows.length <= 4096, 'invalid case count');
  const names = new Set();
  const cases = rows.map(row => {
    requireThat(object(row) && typeof row.name === 'string' && /^[a-z0-9][a-z0-9-]*$/.test(row.name), 'invalid case name');
    requireThat(!names.has(row.name), 'duplicate case name');
    names.add(row.name);
    requireThat(row.kind === 'call' || row.kind === 'receipt', 'invalid case kind');
    requireThat(typeof row.encoded === 'string', 'invalid case encoding');
    return { name: row.name, kind: row.kind, encoded: row.encoded };
  });
  const request = JSON.stringify({ schema: REQUEST_SCHEMA, cases });
  requireThat(Buffer.byteLength(request) <= MAX_PROTOCOL_BYTES, 'protocol input exceeds limit');
  return request;
}

/** A complete, closed, ordered response is required; missing evidence is never a pass. */
export function assessProbeResponse(rows, response, runtime) {
  makeProbeRequest(rows);
  exactKeys(response, ['schema', 'runtime', 'results'], 'response');
  requireThat(response.schema === RESPONSE_SCHEMA && response.runtime === runtime, 'response identity mismatch');
  requireThat(Array.isArray(response.results) && response.results.length === rows.length, 'response coverage mismatch');
  const findings = [];
  const results = response.results.map((result, index) => {
    requireThat(object(result) && typeof result.accepted === 'boolean', 'non-boolean acceptance');
    exactKeys(result, result.accepted ? ['name', 'kind', 'accepted', 'encoded'] : ['name', 'kind', 'accepted'], 'result');
    const row = rows[index];
    requireThat(result.name === row.name && result.kind === row.kind, 'case identity or ordering mismatch');
    requireThat(typeof row.expected === 'boolean', 'missing expected verdict');
    let preserved = null;
    if (result.accepted) {
      requireThat(typeof result.encoded === 'string' && Buffer.byteLength(result.encoded) <= MAX_PROTOCOL_BYTES, 'invalid result encoding');
      const decoded = JSON.parse(result.encoded);
      preserved = isDeepStrictEqual(decoded, row.instance);
    }
    const evidence = { name: row.name, kind: row.kind, expected: row.expected, accepted: result.accepted, preserved };
    if (result.accepted !== row.expected || preserved === false) findings.push(evidence);
    return evidence;
  });
  return { status: findings.length ? 'stopped_for_evaluation' : 'passed', results, findings };
}

/** No shell, inherited credentials, unlimited output, or crash-as-rejection fallback. */
export function invokeProbe(executable, request) {
  requireThat(typeof request === 'string' && Buffer.byteLength(request) <= MAX_PROTOCOL_BYTES, 'invalid probe input');
  return spawnSync(executable, [], {
    input: request, encoding: 'utf8', shell: false,
    timeout: 30000, killSignal: 'SIGKILL', maxBuffer: MAX_PROTOCOL_BYTES,
    env: { LANG: 'C.UTF-8', LC_ALL: 'C.UTF-8' },
  });
}
export function readProbeExecution(execution) {
  requireThat(object(execution) && !execution.error && !execution.signal && execution.status === 0, 'probe execution failed');
  requireThat(typeof execution.stdout === 'string' && execution.stdout.length > 0 && Buffer.byteLength(execution.stdout) <= MAX_PROTOCOL_BYTES, 'missing or oversized probe output');
  return JSON.parse(execution.stdout);
}

/** Bind the TJSV/TypeScript/Rust oracle receipt to the exact files being tested. */
export function verifyOracleReceipt(receipt, rows, revision, digests, validatorRevision) {
  makeProbeRequest(rows);
  requireThat(rows.every(row => typeof row.expected === 'boolean'), 'missing oracle expectations');
  requireThat(object(receipt) && receipt.schema === 'ores.api-docs.tjsv-rpc-admission/v1', 'wrong oracle receipt');
  requireThat(receipt.sourceRevision === revision && receipt.profile === 'ores-rpc-v1-call-receipt', 'stale oracle receipt');
  requireThat(receipt.status === 'passed' && Array.isArray(receipt.findings) && receipt.findings.length === 0, 'oracle did not pass');
  requireThat(receipt.validator?.repository === 'ORESoftware/typespec-json-schema-validator' && receipt.validator.revision === validatorRevision, 'wrong TJSV revision');
  requireThat(
    receipt.coverage?.scope === 'authored-json-schema-versus-typescript-and-rust-client-rpc-v1-fixtures'
      && isDeepStrictEqual(receipt.coverage.executedRuntimes, ['typescript', 'rust'])
      && receipt.coverage.rustPackage === 'ores-api-docs-client'
      && receipt.coverage.universalEquivalenceProven === false,
    'wrong oracle runtime coverage',
  );
  const required = ORACLE_INPUTS;
  exactKeys(receipt.sourceDigests, required, 'oracle digest');
  for (const path of required) {
    requireThat(typeof digests[path] === 'string' && /^[a-f0-9]{64}$/.test(digests[path]) && receipt.sourceDigests[path] === digests[path], `oracle input changed: ${path}`);
  }
  requireThat(Array.isArray(receipt.results) && receipt.results.length === rows.length, 'oracle coverage mismatch');
  for (const [index, row] of rows.entries()) {
    const result = receipt.results[index];
    exactKeys(result, ['name', 'kind', 'expected', 'tjsvAccepted', 'typescriptAccepted', 'rustAccepted'], 'oracle result');
    requireThat(
      result.name === row.name
        && result.kind === row.kind
        && result.expected === row.expected
        && result.tjsvAccepted === row.expected
        && result.typescriptAccepted === row.expected
        && result.rustAccepted === row.expected,
      'oracle verdict mismatch',
    );
  }
}
