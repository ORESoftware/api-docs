import { parseProfile } from '../src/profiles.js';
import type { Submission, ParseResult, Profile } from '../src/profiles.js';

const profile: Profile = 'TextSubmission';
const result: ParseResult = parseProfile(profile, { value: 'name' });
if (result.success) {
  const name: string = result.data.value;
  // @ts-expect-error validated values are readonly
  result.data.value = name;
  // @ts-expect-error the parsed value is not a number
  const number: number = result.data.value;
} else {
  const code: 'invalid_submission' = result.code;
  // @ts-expect-error failures never expose submitted values
  result.data;
}
// @ts-expect-error only explicit reviewed profiles exist
parseProfile('ArbitraryRemoteSchema', {});
// @ts-expect-error inferred output requires a string
const wrong: Submission = { value: 42 };
// @ts-expect-error inferred output requires the value member
const missing: Submission = {};
