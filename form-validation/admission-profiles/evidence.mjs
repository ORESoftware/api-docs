/** Pure, fail-closed parsing/comparison. No schema or runtime implementation lives here. */
export const PROFILES = Object.freeze(['TextSubmission', 'PhoneSubmission', 'IntegerSubmission']);
export const RUNTIMES = Object.freeze(['rust-native', 'dart-vm', 'dart-javascript']);
export const CORPUS_SCHEMA = 'ores.form-admission.corpus/v1';
export const RESULT_SCHEMA = 'ores.form-admission.runtime/v1';
export const MARKER = 'ORES_FORM_ADMISSION=';
export const requireThat = (condition, message) => { if (!condition) throw new Error(message); };
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const keys = (value, expected) => object(value) && Object.keys(value).length === expected.length && expected.every(key => Object.hasOwn(value, key));

const jsonValue = value => value === null || typeof value === 'string' || typeof value === 'boolean' ||
  (typeof value === 'number' && Number.isFinite(value)) ||
  (Array.isArray(value) && value.every(jsonValue)) ||
  (object(value) && Object.getPrototypeOf(value) === Object.prototype && Object.values(value).every(jsonValue));

export function readCases(corpus) {
  requireThat(keys(corpus, ['schema', 'cases']) && corpus.schema === CORPUS_SCHEMA, 'invalid corpus envelope');
  requireThat(Array.isArray(corpus.cases) && corpus.cases.length > 0 && corpus.cases.length <= 1000, 'invalid corpus size');
  const seen = new Set();
  const coverage = new Set();
  const rows = corpus.cases.map(row => {
    requireThat(keys(row, ['id', 'profile', 'input', 'expected']), 'invalid case fields');
    requireThat(typeof row.id === 'string' && /^[a-z][a-z0-9-]{0,79}(?![\s\S])/.test(row.id), 'invalid case identifier');
    requireThat(!seen.has(row.id), 'duplicate case identifier');
    seen.add(row.id);
    requireThat(PROFILES.includes(row.profile), 'unknown profile');
    requireThat(typeof row.expected === 'boolean', 'expected verdict must be boolean');
    requireThat(Buffer.byteLength(JSON.stringify(row.input)) <= 100000, 'oversized fixture');
    // Refuse values JSON.stringify would coerce into a different specimen.
    requireThat(jsonValue(row.input), 'fixture must contain finite, plain JSON values');
    coverage.add(`${row.profile}:${row.expected}`);
    return Object.freeze({ ...row });
  });
  requireThat(coverage.size === PROFILES.length * 2, 'missing positive/negative profile coverage');
  return rows;
}

export function readRuntime(stdout, cases) {
  requireThat(typeof stdout === 'string', 'runtime stdout must be text');
  const lines = stdout.split(/\r?\n/u).filter(line => line.startsWith(MARKER));
  requireThat(lines.length === 1, 'expected exactly one fresh runtime envelope');
  const envelope = JSON.parse(lines[0].slice(MARKER.length));
  requireThat(keys(envelope, ['schema', 'results']) && envelope.schema === RESULT_SCHEMA, 'invalid runtime envelope');
  requireThat(Array.isArray(envelope.results) && envelope.results.length === cases.length, 'incomplete runtime coverage');
  const expected = new Map(cases.map(row => [row.id, row]));
  const actual = new Map();
  for (const row of envelope.results) {
    requireThat(keys(row, ['id', 'profile', 'accepted', 'preserved']), 'invalid runtime row');
    requireThat(expected.has(row.id) && !actual.has(row.id), 'unknown or duplicate runtime case');
    requireThat(row.profile === expected.get(row.id).profile, 'runtime profile mismatch');
    requireThat(typeof row.accepted === 'boolean', 'runtime verdict must be boolean');
    requireThat(row.preserved === (row.accepted ? true : null), 'runtime normalized input or malformed preservation verdict');
    actual.set(row.id, row);
  }
  return actual;
}

export function compareEvidence(corpus, validate, outputs) {
  const cases = readCases(corpus);
  requireThat(typeof validate === 'function', 'missing schema validator');
  requireThat(keys(outputs, RUNTIMES), 'missing or unexpected runtime');
  const runtimes = Object.fromEntries(RUNTIMES.map(name => [name, readRuntime(outputs[name], cases)]));
  const results = cases.map(row => {
    // Refusal, exception, malformed verdict or process failure is never a rejection.
    const verdict = validate(row.profile, row.input);
    requireThat(keys(verdict, ['valid', 'errors']) && typeof verdict.valid === 'boolean' && Array.isArray(verdict.errors), 'invalid schema verdict');
    requireThat(verdict.valid === (verdict.errors.length === 0), 'inconsistent schema verdict');
    return {
      id: row.id, profile: row.profile, expected: row.expected, schemaAccepted: verdict.valid,
      runtimes: Object.fromEntries(RUNTIMES.map(name => [name, runtimes[name].get(row.id).accepted])),
    };
  });
  const findings = results.filter(row => row.schemaAccepted !== row.expected || Object.values(row.runtimes).some(value => value !== row.expected));
  return { status: findings.length ? 'stopped_for_evaluation' : 'passed', results, findings };
}
