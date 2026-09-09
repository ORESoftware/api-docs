import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { readSafeBytes } from '../../scripts/projection-evidence-io.mjs';

/** No expected verdicts, source schemas or precomputed results reach the probe. */
export function probeRequest(cases) {
  return JSON.stringify({
    schema: 'ores.form-admission.probes/v1',
    cases: cases.map(({ id, profile, input }) => ({ id, profile, input })),
  });
}

export async function runTypeScript(root, cases) {
  const directory = resolve(root, 'form-validation/typescript');
  const environment = {};
  for (const key of ['PATH', 'HOME', 'TMPDIR', 'TMP', 'TEMP', 'SYSTEMROOT', 'WINDIR']) {
    if (process.env[key] !== undefined) environment[key] = process.env[key];
  }
  // In particular do not forward credentials or NODE_OPTIONS to the compiler
  // and candidate probe. No shell, runtime downloads or optional lane fallback.
  const run = (args, input) => execFileSync(process.execPath, args, {
    cwd: directory, env: environment, input, encoding: 'utf8',
    timeout: 120000, maxBuffer: 2 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'],
  });
  const json = async path => JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(await readSafeBytes(directory, path)));
  const manifest = await json('package.json');
  const zod = await json('node_modules/zod/package.json');
  const typescript = await json('node_modules/typescript/package.json');
  if (zod.version !== manifest.dependencies.zod || typescript.version !== manifest.devDependencies.typescript) {
    throw new Error('installed TypeScript/Zod versions differ from reviewed manifest');
  }
  const compiler = resolve(directory, 'node_modules/typescript/bin/tsc');
  run([compiler, '-p', 'tsconfig.json']);
  run([compiler, '-p', 'tsconfig.test.json']);
  const emitted = 'tmp/dist/profiles.js';
  const digest = bytes => createHash('sha256').update(bytes).digest('hex');
  const before = digest(await readSafeBytes(directory, emitted));
  const stdout = run(['test/probe.mjs'], probeRequest(cases));
  if (digest(await readSafeBytes(directory, emitted)) !== before) {
    throw new Error('compiled TypeScript profile changed during execution');
  }
  return {
    stdout,
    toolchain: { typescript: typescript.version, zod: zod.version },
    emittedDigest: before,
  };
}
