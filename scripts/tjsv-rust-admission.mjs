import { execFileSync } from 'node:child_process';
import { isDeepStrictEqual } from 'node:util';

export const RUST_REPORT_SCHEMA = 'ores.api-docs.rust-rpc-admission/v1';
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const requireThat = (condition, message) => { if (!condition) throw new Error(message); };
const exactKeys = (value, keys) => object(value)
  && isDeepStrictEqual(Object.keys(value).sort(), [...keys].sort());

/** Run the real client; compilation, timeout, process and JSON failures propagate. */
export function runRustClient(root) {
  const stdout = execFileSync('cargo', [
    'run', '--quiet', '--locked', '--manifest-path', 'clients/rust/Cargo.toml',
    '--example', 'tjsv_admission',
  ], { cwd: root, encoding: 'utf8', timeout: 600000, maxBuffer: 16 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'] });
  return JSON.parse(stdout);
}

/** Rows come from readCases. Never trust a process exit code or a summary pass flag. */
export function compareRustResults(rows, schemaResult, rustReport) {
  requireThat(Array.isArray(rows) && rows.length > 0, 'missing admission rows');
  requireThat(object(schemaResult) && Array.isArray(schemaResult.results)
    && schemaResult.results.length === rows.length, 'incomplete schema/runtime results');
  requireThat(exactKeys(rustReport, ['schema', 'results'])
    && rustReport.schema === RUST_REPORT_SCHEMA, 'unsupported Rust report');
  requireThat(Array.isArray(rustReport.results) && rustReport.results.length === rows.length,
    'incomplete Rust results');
  const results = [];
  const findings = [];
  const seen = new Set();
  for (const [index, row] of rows.entries()) {
    requireThat(object(row) && typeof row.name === 'string' && !seen.has(row.name)
      && ['call', 'receipt'].includes(row.kind) && typeof row.expected === 'boolean', 'invalid admission row');
    seen.add(row.name);
    const schema = schemaResult.results[index];
    requireThat(object(schema) && schema.name === row.name && schema.kind === row.kind
      && schema.expected === row.expected && typeof schema.tjsvAccepted === 'boolean'
      && typeof schema.typescriptAccepted === 'boolean', 'schema result identity/verdict mismatch');
    const rust = rustReport.results[index];
    requireThat(object(rust) && typeof rust.accepted === 'boolean', 'malformed Rust verdict');
    requireThat(exactKeys(rust, rust.accepted ? ['name', 'kind', 'accepted', 'decoded'] : ['name', 'kind', 'accepted']),
      'unexpected Rust verdict fields');
    requireThat(rust.name === row.name && rust.kind === row.kind, 'Rust result identity/order mismatch');
    if (rust.accepted) {
      requireThat(isDeepStrictEqual(rust.decoded, row.instance), `Rust changed decoded value: ${row.name}`);
    }
    const result = {
      name: row.name, kind: row.kind, expected: row.expected,
      tjsvAccepted: schema.tjsvAccepted, typescriptAccepted: schema.typescriptAccepted,
      rustAccepted: rust.accepted,
    };
    results.push(result);
    // Recompute every finding; a supplied status/findings summary cannot erase drift.
    if ([result.tjsvAccepted, result.typescriptAccepted, result.rustAccepted]
      .some(accepted => accepted !== row.expected)) findings.push(result);
  }
  return { status: findings.length === 0 ? 'passed' : 'stopped_for_evaluation', results, findings };
}
