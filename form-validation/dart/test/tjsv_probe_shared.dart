import '../lib/validation_message.dart';
import 'shared.dart' show fromFixture;

// The transport supplies IDs and instances, never expected verdicts.
Map<String, Object?> runProbe(Map<String, dynamic> input) {
  final messages = <Map<String, Object?>>[];
  for (final raw in input['messages'] as List) {
    final row = raw as Map<String, dynamic>;
    try {
      final value = ValidationMessage.fromJson(row['instance']).toJson();
      messages.add({'id': row['id'], 'accepted': true, 'value': value});
    } on InvalidValidationMessage {
      messages.add({'id': row['id'], 'accepted': false, 'value': null});
    }
  }
  final fields = <Map<String, Object?>>[];
  for (final raw in input['fields'] as List) {
    final row = raw as Map<String, dynamic>;
    final codes = fromFixture(row['rules'] as Map<String, dynamic>)
        .validate(row['value'] as String?);
    fields.add({
      'id': row['id'],
      'message': ValidationMessage.fromCodes('input', codes).toJson()
    });
  }
  // These constructor checks also execute in VM and compiled JavaScript.
  final source = <ValidationIssue>[];
  final message = ValidationMessage(source);
  source.add(ValidationIssue('field', 'email'));
  if (message.issues.isNotEmpty) throw StateError('mutable message alias');
  var rejected = false;
  try {
    message.issues.add(ValidationIssue('field', 'email'));
  } on UnsupportedError {
    rejected = true;
  }
  if (!rejected) throw StateError('mutable message issues');
  return {'messages': messages, 'fields': fields};
