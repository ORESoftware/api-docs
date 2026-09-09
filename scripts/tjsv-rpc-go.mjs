import { execFileSync } from 'node:child_process';
import { isDeepStrictEqual } from 'node:util';
import { fileURLToPath } from 'node:url';

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const requireThat = (condition, message) => { if (!condition) throw new Error(message); };
const exactKeys = (value, keys) => object(value) && isDeepStrictEqual(Object.keys(value).sort(), [...keys].sort());
const EXECUTABLE = fileURLToPath(new URL('../tmp/tjsv-rpc-go', import.meta.url));

/** Admit a complete, ordered native result set; never treat missing evidence as rejection. */
export function compareGoEvidence(rows, evidence) {
  requireThat(Array.isArray(rows) && rows.length > 0, 'empty Go comparison corpus');
  requireThat(exactKeys(evidence, ['schema', 'runtime', 'toolchain', 'results']), 'malformed Go evidence envelope');
  requireThat(evidence.schema === 'ores.api-docs.rpc-adapter/v1' && evidence.runtime === 'go', 'wrong Go evidence profile');
  requireThat(typeof evidence.toolchain === 'string' && /^go\d+\.\d+(?:\.\d+)?$/.test(evidence.toolchain), 'missing Go toolchain');
  requireThat(Array.isArray(evidence.results) && evidence.results.length === rows.length, 'incomplete Go results');
  const results = rows.map((row, index) => {
    const item = evidence.results[index];
    requireThat(object(item) && typeof item.accepted === 'boolean', 'malformed Go verdict');
    requireThat(exactKeys(item, ['name', 'kind', 'accepted', ...(item.accepted ? ['encoded'] : [])]), 'unknown or missing Go result fields');
    requireThat(item.name === row.name && item.kind === row.kind, 'Go result identity or order mismatch');
    if (item.accepted) {
      requireThat(typeof item.encoded === 'string', 'missing Go re-encoded value');
      requireThat(isDeepStrictEqual(JSON.parse(item.encoded), row.instance), `Go changed decoded value: ${row.name}`);
    }
    return { name: row.name, kind: row.kind, expected: row.expected, goAccepted: item.accepted };
  });
  const findings = results.filter(row => row.goAccepted !== row.expected);
  return { toolchain: evidence.toolchain, status: findings.length === 0 ? 'passed' : 'stopped_for_evaluation', results, findings };
}

/** Invoke a freshly built native executable. No shell, caller-supplied paths, or expected verdicts. */
export function runGoAdmission(rows) {
  const cases = rows.map(({ name, kind, encoded }) => ({ name, kind, encoded }));
  const output = execFileSync(EXECUTABLE, [], {
    input: JSON.stringify({ cases }), encoding: 'utf8', timeout: 30_000,
    maxBuffer: 32 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'],
  });
  return compareGoEvidence(rows, JSON.parse(output));
}
