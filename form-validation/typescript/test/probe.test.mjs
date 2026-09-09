import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { observe } from './probe.mjs';

const request = () => ({ schema: 'ores.form-admission.probes/v1', cases: [
  { id: 'valid', profile: 'TextSubmission', input: { value: 'name' } },
  { id: 'invalid', profile: 'TextSubmission', input: { value: '' } },
] });

test('real compiled Zod probe computes verdicts without expected answers', () => {
  const result = observe(request());
  assert.deepEqual(result.results.map(row => row.accepted), [true, false]);
  assert.deepEqual(result.results.map(row => row.preserved), [true, null]);
  assert.equal(JSON.stringify(result).includes('name'), false);
});

for (const [name, change] of [
  ['expectation leak', value => { value.cases[0].expected = true; }],
  ['schema leak', value => { value.authority = {}; }],
  ['missing case list', value => { delete value.cases; }],
  ['empty cases', value => { value.cases = []; }],
  ['unknown profile', value => { value.cases[0].profile = 'Unknown'; }],
  ['duplicate id', value => { value.cases[1].id = 'valid'; }],
  ['trailing newline id', value => { value.cases[0].id = 'bad\n'; }],
  ['missing input', value => { delete value.cases[0].input; }],
  ['wrong schema version', value => { value.schema = 'ores.form-admission.probes/v2'; }],
]) test(`probe refuses ${name}`, () => {
  const value = request();
  change(value);
  assert.throws(() => observe(value), /invalid probe request/);
});

test('real subprocess refuses malformed UTF-8, JSON, oversized input and arguments', () => {
  const program = new URL('./probe.mjs', import.meta.url);
  for (const [input, args] of [
    [Buffer.from([0xff]), []], ['not-json', []], [' '.repeat(1024 * 1024 + 1), []],
    [JSON.stringify(request()), ['--ignored-option']],
  ]) {
    assert.throws(() => execFileSync(process.execPath, [program.pathname, ...args], {
      input, timeout: 10000, stdio: ['pipe', 'pipe', 'pipe'],
    }), error => error.status === 3);
  }
});
