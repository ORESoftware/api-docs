import "@typespec/json-schema";
using TypeSpec.JsonSchema;

// Independently authored authority; JSON Schema is a peer, never a generated source.
namespace OresFormAdmission;

@id("TextSubmission")
model TextSubmission {
  @minLength(1)
  @maxLength(80)
  value: string;
}

@id("PhoneSubmission")
model PhoneSubmission {
  // Strict end-of-input: '$' alone can accept a trailing line terminator.
  @pattern("^\\+[1-9][0-9]{1,14}(?![\\s\\S])")
  value: string;
}

@id("IntegerSubmission")
model IntegerSubmission {
  // Form TEXT, not JSON Schema's numeric integer type. Preserve '-0'.
  @pattern("^(?:-?0|[1-9][0-9]?|1[0-4][0-9]|150)(?![\\s\\S])")
  value: string;
}
