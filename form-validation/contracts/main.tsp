import "@typespec/json-schema";
using TypeSpec.JsonSchema;

// Independently authored authority. Do not regenerate this from Schema A.
@id("ValidationCode")
enum ValidationCode {
  too_large, required, blank, min_chars, max_chars, min_lines, max_lines,
  email, phone_e164, number, integer, unsafe_integer, minimum, maximum,
  date, date_min, date_max, invalid_unicode,
}

@id("ValidationIssue")
model ValidationIssue {
  @minLength(1)
  @maxLength(128)
  // The final negative lookahead requires absolute end, including after LF.
  @pattern("^[A-Za-z0-9][A-Za-z0-9_.:-]*(?![\\s\\S])")
  field: string;
  code: ValidationCode;
}

@id("ValidationMessage")
model ValidationMessage {
  schema_version: "ores.form-validation/v1";
  @maxItems(128)
  issues: ValidationIssue[];
}
