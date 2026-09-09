import { isDeepStrictEqual } from 'node:util';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { parseProfile } from '../tmp/dist/profiles.js';

const PROFILES = ['TextSubmission', 'PhoneSubmission', 'IntegerSubmission'];
const ownKeys = (value, fields) => value !== null && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).length === fields.length && fields.every(key => Object.hasOwn(value, key));
const requireThat = (condition) => { if (!condition) throw new Error('invalid probe request'); };

/** The probe receives no expectations, schemas or precomputed decisions. */
export function observe(request) {
  requireThat(ownKeys(request, ['schema', 'cases']) && request.schema === 'ores.form-admission.probes/v1');
  requireThat(Array.isArray(request.cases) && request.cases.length > 0 && request.cases.length <= 1000);
  const seen = new Set();
  const results = request.cases.map(row => {
    requireThat(ownKeys(row, ['id', 'profile', 'input']));
    requireThat(typeof row.id === 'string' && /^[a-z][a-z0-9-]{0,79}(?![\s\S])/.test(row.id));
    requireThat(!seen.has(row.id) && PROFILES.includes(row.profile));
    seen.add(row.id);
    const parsed = parseProfile(row.profile, row.input);
    return {
      id: row.id, profile: row.profile, accepted: parsed.success,
      preserved: parsed.success ? isDeepStrictEqual(parsed.data, row.input) : null,
    };
  });
  return { schema: 'ores.form-admission.runtime/v1', results };
}

async function main() {
  requireThat(process.argv.length === 2);
  const chunks = [];
  let size = 0;
  for await (const chunk of process.stdin) {
    size += chunk.length;
    requireThat(size <= 1024 * 1024);
    chunks.push(chunk);
  }
  const text = new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks));
  const response = observe(JSON.parse(text));
  console.log(`ORES_FORM_ADMISSION=${JSON.stringify(response)}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch(() => {
    console.error('TypeScript profile probe failed');
    process.exitCode = 3;
  });
}
