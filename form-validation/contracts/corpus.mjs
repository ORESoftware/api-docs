import { isDeepStrictEqual } from 'node:util';

export const VERSION = 'ores.form-validation/v1';
export const CODES = Object.freeze(['too_large', 'required', 'blank', 'min_chars', 'max_chars', 'min_lines', 'max_lines', 'email', 'phone_e164', 'number', 'integer', 'unsafe_integer', 'minimum', 'maximum', 'date', 'date_min', 'date_max', 'invalid_unicode']);
export function requireThat(condition, message) { if (!condition) throw new Error(message); }
export function exactKeys(value, keys) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    && isDeepStrictEqual(Object.keys(value).sort(), [...keys].sort());
}
export function message(codes = [], field = 'input') {
  return { schema_version: VERSION, issues: codes.map(code => ({ field, code })) };
}

export function fieldCases(raw) {
  requireThat(Array.isArray(raw) && raw.length >= 85, 'missing field corpus');
  const seen = new Set();
  for (const row of raw) {
    requireThat(exactKeys(row, ['id', 'rules', 'value', 'errors']), 'invalid field fixture shape');
    requireThat(typeof row.id === 'string' && /^[a-z0-9][a-z0-9-]*$/.test(row.id) && !seen.has(row.id), 'invalid or duplicate field case ID');
    seen.add(row.id);
    requireThat(row.value === null || typeof row.value === 'string', 'invalid field fixture value');
    requireThat(row.rules !== null && typeof row.rules === 'object' && !Array.isArray(row.rules), 'invalid field fixture rules');
    requireThat(Array.isArray(row.errors) && row.errors.every(code => CODES.includes(code)), 'invalid field fixture error codes');
  }
  return raw;
}

export function wireCases(fields) {
  const rows = [];
  const add = (id, instance, expected) => rows.push({ id, instance, expected });
  add('empty', message(), true);
  for (const code of CODES) add(`code-${code.replaceAll('_', '-')}`, message([code]), true);
  add('field-boundary', message(['required'], 'a'.repeat(128)), true);
  add('issue-boundary', message(Array(128).fill('required')), true);
  add('field-punctuation', message(['email'], 'profile.email:primary-1'), true);
  add('ordered-codes', message(['blank', 'min_chars', 'max_lines', 'email']), true);
  const bad = [
    ['null', null], ['array', []], ['number', 42], ['string', VERSION],
    ['missing-version', { issues: [] }], ['missing-issues', { schema_version: VERSION }],
    ['wrong-version', { ...message(), schema_version: 'ores.form-validation/v2' }],
    ['numeric-version', { ...message(), schema_version: 1 }],
    ['extra-root', { ...message(), value: 'synthetic-private-sentinel' }],
    ['null-issues', { ...message(), issues: null }],
    ['object-issues', { ...message(), issues: {} }],
    ['null-issue', { ...message(), issues: [null] }],
    ['missing-field', { ...message(), issues: [{ code: 'required' }] }],
    ['missing-code', { ...message(), issues: [{ field: 'email' }] }],
    ['wrong-field-type', { ...message(), issues: [{ field: 1, code: 'required' }] }],
    ['wrong-code-type', { ...message(), issues: [{ field: 'email', code: 1 }] }],
    ['extra-issue', { ...message(), issues: [{ field: 'email', code: 'email', value: 'synthetic-private-sentinel' }] }],
    ['unknown-code', message(['custom'])], ['empty-code', message([''])],
    ['too-many-issues', message(Array(129).fill('required'))],
    ['field-too-long', message(['required'], 'a'.repeat(129))],
    ...['', ' ', 'email\n', 'email\r\n', 'a b', 'é', '😀', '<script>', '.email', '_email', 'email\u0000'].map((field, i) => [`bad-field-${i}`, message(['required'], field)]),
  ];
  for (const [id, instance] of bad) add(id, instance, false);
  for (const row of fields) add(`form-${row.id}`, message(row.errors), true);
  requireThat(new Set(rows.map(row => row.id)).size === rows.length, 'duplicate wire case ID');
  return rows;
}

/** Technical failures and malformed output never count as rejected input. */
export function inspectObservation(wire, fields, output) {
  requireThat(exactKeys(output, ['messages', 'fields']), 'malformed runtime output');
  requireThat(Array.isArray(output.messages) && output.messages.length === wire.length, 'missing or extra runtime cases');
  const expected = new Map(wire.map(row => [row.id, row]));
  const seen = new Set();
  for (const row of output.messages) {
    requireThat(exactKeys(row, ['id', 'accepted', 'value']) && typeof row.accepted === 'boolean', 'malformed runtime verdict');
    requireThat(expected.has(row.id) && !seen.has(row.id), 'unknown or duplicate runtime case');
    seen.add(row.id);
    requireThat(row.accepted ? isDeepStrictEqual(row.value, expected.get(row.id).instance) : row.value === null, 'runtime changed round-trip value or exposed rejected input');
  }
  requireThat(Array.isArray(output.fields) && output.fields.length === fields.length, 'missing or extra field executions');
  const byId = new Map(fields.map(row => [row.id, row]));
  seen.clear();
  for (const row of output.fields) {
    requireThat(exactKeys(row, ['id', 'message']) && byId.has(row.id) && !seen.has(row.id), 'invalid field execution');
    seen.add(row.id);
    requireThat(isDeepStrictEqual(row.message, message(byId.get(row.id).errors)), 'field error order or value-free message drift');
  }
  return output;
}
