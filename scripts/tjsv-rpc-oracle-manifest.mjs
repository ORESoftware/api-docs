import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../', import.meta.url));

// Independently authored RPC contracts, runtime adapters, admission/verifier
// helpers, tests, and the exact workflow are always part of the oracle closure.
// Rust source is added from the tracked repository inventory below so a new
// Rust oracle input cannot be omitted from either producer or verifier evidence.
export const FIXED_INPUTS = Object.freeze([
  '.github/workflows/tjsv-rpc-admission.yml',
  'Cargo.lock',
  'Cargo.toml',
  'clients/typescript/src/rpc.js',
  'examples/rpc-v1/conformance.json',
  'idl/typespec/v1.tsp',
  'json-schema/rpc-call.schema.json',
  'json-schema/rpc-receipt.schema.json',
  'runtime/v1-conformance.json',
  'scripts/projection-evidence-io.mjs',
  'scripts/test-projection-evidence-io.mjs',
  'scripts/test-tjsv-rpc-entrypoint.mjs',
  'scripts/test_tjsv_rpc_admission.mjs',
  'scripts/test_tjsv_rpc_runtime_protocol.mjs',
  'scripts/test_tjsv_rust_admission.mjs',
  'scripts/tjsv-go-admission.mjs',
  'scripts/tjsv-rpc-admission.mjs',
  'scripts/tjsv-rpc-oracle-manifest.mjs',
  'scripts/tjsv-rpc-runtime-protocol.mjs',
  'scripts/tjsv-rust-admission.mjs',
  'scripts/tjsv-source-integrity.mjs',
]);

const REQUIRED_RUST_INPUTS = Object.freeze([
  'clients/rust/examples/tjsv_admission.rs',
  'rust/src/lib.rs',
]);

function trackedRustInputs(root) {
  const env = { ...process.env, GIT_NO_REPLACE_OBJECTS: '1' };
  for (const key of [
    'GIT_DIR',
    'GIT_WORK_TREE',
    'GIT_INDEX_FILE',
    'GIT_OBJECT_DIRECTORY',
    'GIT_ALTERNATE_OBJECT_DIRECTORIES',
  ]) delete env[key];
  const output = execFileSync('git', ['-C', root, 'ls-files', '-z', '--', 'rust', 'clients/rust'], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    env,
    timeout: 30000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const inputs = output.split('\0').filter(Boolean);
  if (!REQUIRED_RUST_INPUTS.every(path => inputs.includes(path))) {
    throw new Error('missing tracked Rust oracle/core');
  }
  return inputs;
}

/** Return one closed, sorted, immutable manifest for producer and verifier use. */
export function oracleInputPaths(root = ROOT) {
  return Object.freeze([...new Set([...FIXED_INPUTS, ...trackedRustInputs(root)])].sort());
}
