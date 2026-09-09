/// Value-free JSON-value contract shared with Rust and checked by TJSV.
/// It does not interpret schemas or replace field/domain validation.
library;

const validationMessageVersion = 'ores.form-validation/v1';
const validationCodes = <String>{
  'too_large',
  'required',
  'blank',
  'min_chars',
  'max_chars',
  'min_lines',
  'max_lines',
  'email',
  'phone_e164',
  'number',
  'integer',
  'unsafe_integer',
  'minimum',
  'maximum',
  'date',
  'date_min',
  'date_max',
  'invalid_unicode',
};

final class InvalidValidationMessage implements Exception {
  const InvalidValidationMessage();
  @override
  String toString() => 'invalid validation message';
}

bool _validField(String value) {
  if (value.isEmpty || value.length > 128) return false;
  bool alnum(int c) =>
      (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
  return alnum(value.codeUnitAt(0)) &&
      value.codeUnits
          .every((c) => alnum(c) || c == 95 || c == 46 || c == 58 || c == 45);
}

Map<Object?, Object?> _object(Object? value, Set<String> keys) {
  if (value is! Map ||
      value.length != keys.length ||
      !value.keys.every(keys.contains)) {
    throw const InvalidValidationMessage();
  }
  return value.cast<Object?, Object?>();
}

final class ValidationIssue {
  final String field, code;
  factory ValidationIssue(String field, String code) {
    if (!_validField(field) || !validationCodes.contains(code))
      throw const InvalidValidationMessage();
    return ValidationIssue._(field, code);
  }
  const ValidationIssue._(this.field, this.code);
  factory ValidationIssue.fromJson(Object? value) {
    final map = _object(value, {'field', 'code'});
    final field = map['field'], code = map['code'];
    if (field is! String || code is! String)
      throw const InvalidValidationMessage();
    return ValidationIssue(field, code);
  }
  Map<String, Object?> toJson() => {'field': field, 'code': code};
}

final class ValidationMessage {
  final List<ValidationIssue> issues;
  factory ValidationMessage(List<ValidationIssue> issues) {
    if (issues.length > 128) throw const InvalidValidationMessage();
    return ValidationMessage._(List.unmodifiable(issues));
  }
  const ValidationMessage._(this.issues);
  factory ValidationMessage.fromCodes(String field, List<String> codes) {
    if (!_validField(field) || codes.length > 128)
      throw const InvalidValidationMessage();
    return ValidationMessage(
        codes.map((code) => ValidationIssue(field, code)).toList());
  }
  factory ValidationMessage.fromJson(Object? value) {
    final map = _object(value, {'schema_version', 'issues'});
    final issues = map['issues'];
    if (map['schema_version'] != validationMessageVersion ||
        issues is! List ||
        issues.length > 128) {
      throw const InvalidValidationMessage();
    }
    return ValidationMessage(issues.map(ValidationIssue.fromJson).toList());
  }
  Map<String, Object?> toJson() => {
        'schema_version': validationMessageVersion,
        'issues': issues.map((issue) => issue.toJson()).toList(),
      };
}
