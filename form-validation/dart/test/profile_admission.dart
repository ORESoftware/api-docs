// Runs unchanged on the Dart VM and compiled JavaScript. No dart:io/FFI dependency.
import 'dart:convert';
import 'package:ores_form_validation/ores_form_validation.dart';

FieldValidator profileValidator(String profile) {
  final rules = switch (profile) {
    'TextSubmission' => const Rules(
        required: true, minChars: 1, maxChars: 80),
    'PhoneSubmission' => const Rules(kind: Kind.phoneE164, required: true),
    'IntegerSubmission' => const Rules(
        kind: Kind.integer, required: true, minimum: 0, maximum: 150),
    _ => throw StateError('unsupported admission profile'),
  };
  return FieldValidator(rules);
}

void main() {
  const encoded = String.fromEnvironment('PROFILE_CORPUS_BASE64');
  if (encoded.isEmpty) throw StateError('missing current corpus');
  final corpus = jsonDecode(utf8.decode(base64Decode(encoded)))
      as Map<String, dynamic>;
  if (corpus['schema'] != 'ores.form-admission.corpus/v1') {
    throw StateError('unsupported corpus');
  }
  final cases = corpus['cases'] as List<dynamic>;
  if (cases.isEmpty) throw StateError('empty corpus');
  final results = <Map<String, dynamic>>[];
  for (final item in cases) {
    final row = item as Map<String, dynamic>;
    final profile = row['profile'] as String;
    final validator = profileValidator(profile);
    final input = row['input'];
    var accepted = false;
    bool? preserved;
    if (input is Map<String, dynamic> &&
        input.length == 1 &&
        input.containsKey('value') &&
        input['value'] is String) {
      final value = input['value'] as String;
      accepted = validator.validate(value).isEmpty;
      if (accepted) preserved = value == input['value'];
    }
    if (accepted != row['expected']) {
      throw StateError('profile mismatch for ${row['id']}');
    }
    results.add({
      'id': row['id'],
      'profile': profile,
      'accepted': accepted,
      'preserved': preserved,
    });
  }
  print('ORES_FORM_ADMISSION=${jsonEncode({
        'schema': 'ores.form-admission.runtime/v1',
        'results': results,
      })}');
}
