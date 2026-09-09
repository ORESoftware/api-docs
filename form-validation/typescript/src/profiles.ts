import * as z from 'zod';

/**
 * Runtime adapters, not a third contract authority. The independent sources are
 * ../admission-profiles/main.tsp and authored.schema.json. TJSV checks both
 * sources and runs these adapters against the same Rust/Dart corpus.
 */
const scalarString = z.string().refine(value => {
  for (const scalar of value) {
    const point = scalar.codePointAt(0)!;
    if (point >= 0xd800 && point <= 0xdfff) return false;
  }
  return true;
}, 'invalid_unicode');

// Zod 4.5 measures string bounds in code points. Do not downgrade without
// rerunning the astral/combining fixtures. No trim, coercion or defaults.
const schemas = Object.freeze({
  TextSubmission: z.strictObject({ value: scalarString.min(1).max(80) }).readonly(),
  PhoneSubmission: z.strictObject({
    value: scalarString.regex(/^\+[1-9][0-9]{1,14}(?![\s\S])/),
  }).readonly(),
  IntegerSubmission: z.strictObject({
    value: scalarString.regex(/^(?:-?0|[1-9][0-9]?|1[0-4][0-9]|150)(?![\s\S])/),
  }).readonly(),
});

export type Profile = keyof typeof schemas;
export type Submission = z.infer<(typeof schemas)[Profile]>;
export type ParseResult =
  | Readonly<{ success: true; data: Submission }>
  | Readonly<{ success: false; code: 'invalid_submission' }>;

const rejected: ParseResult = Object.freeze({ success: false, code: 'invalid_submission' });

// Public input is a decoded JSON object, not a class instance, accessor or
// inherited-property bag. This guard avoids invoking ordinary getters.
function plainDataObject(input: unknown): input is Record<string, unknown> {
  if (input === null || typeof input !== 'object' || Array.isArray(input)) return false;
  const prototype: unknown = Object.getPrototypeOf(input);
  if (prototype !== Object.prototype && prototype !== null) return false;
  return Reflect.ownKeys(input).every(key => {
    if (typeof key !== 'string') return false;
    const descriptor = Object.getOwnPropertyDescriptor(input, key);
    return descriptor?.enumerable === true && Object.hasOwn(descriptor, 'value');
  });
}

/** Parse without retaining submitted values in errors or transforming success. */
export function parseProfile(profile: Profile, input: unknown): ParseResult {
  // An unknown developer-owned profile is a configuration error, not a
  // successful negative specimen or a dynamic remote schema.
  if (!Object.hasOwn(schemas, profile)) throw new Error('unsupported admission profile');
  if (!plainDataObject(input)) return rejected;
  const parsed = schemas[profile].safeParse(input);
  return parsed.success ? Object.freeze({ success: true, data: parsed.data }) : rejected;
}
