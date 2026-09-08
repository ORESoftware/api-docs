library;

import 'dart:convert';

const maxInputBytes = 65536;
const maxSafeInteger = 9007199254740991;

enum Kind { text, email, phoneE164, number, integer, date }

/// Local developer-owned configuration, not a remotely executable schema.
final class Rules {
  final Kind kind;
  final bool required, nonBlank;
  final int? minChars, maxChars, minLines, maxLines;
  final double? minimum, maximum;
  final String? dateMin, dateMax;
  const Rules(
      {this.kind = Kind.text,
      this.required = false,
      this.nonBlank = false,
      this.minChars,
      this.maxChars,
      this.minLines,
      this.maxLines,
      this.minimum,
      this.maximum,
      this.dateMin,
      this.dateMax});
}

final class InvalidRules implements Exception {
  const InvalidRules();
  @override
  String toString() => 'invalid form validation rules';
}

/// Immutable validator. Errors are stable localization keys, never field values.
final class FieldValidator {
  final Rules rules;
  final DateTime? _dateMin, _dateMax;
  factory FieldValidator(Rules rules) {
    bool ordered(int? a, int? b) => a == null || b == null || a <= b;
    if (!ordered(rules.minChars, rules.maxChars) ||
        !ordered(rules.minLines, rules.maxLines) ||
        [rules.minChars, rules.maxChars]
            .nonNulls
            .any((n) => n < 0 || n > maxInputBytes) ||
        [rules.minLines, rules.maxLines]
            .nonNulls
            .any((n) => n < 1 || n > maxInputBytes + 1)) {
      throw const InvalidRules();
    }
    final numeric = rules.kind == Kind.number || rules.kind == Kind.integer;
    if ((!numeric && (rules.minimum != null || rules.maximum != null)) ||
        [rules.minimum, rules.maximum].nonNulls.any((n) => !n.isFinite) ||
        (rules.minimum != null &&
            rules.maximum != null &&
            rules.minimum! > rules.maximum!)) {
      throw const InvalidRules();
    }
    if (rules.kind == Kind.integer &&
        [rules.minimum, rules.maximum]
            .nonNulls
            .any((n) => n % 1 != 0 || n.abs() > maxSafeInteger)) {
      throw const InvalidRules();
    }
    if (rules.kind != Kind.date &&
        (rules.dateMin != null || rules.dateMax != null)) {
      throw const InvalidRules();
    }
    final low = rules.dateMin == null ? null : _date(rules.dateMin!);
    final high = rules.dateMax == null ? null : _date(rules.dateMax!);
    if ((rules.dateMin != null && low == null) ||
        (rules.dateMax != null && high == null) ||
        (low != null && high != null && low.isAfter(high))) {
      throw const InvalidRules();
    }
    return FieldValidator._(rules, low, high);
  }
  const FieldValidator._(this.rules, this._dateMin, this._dateMax);

  List<String> validate(String? value) {
    if (value == null || value.isEmpty)
      return rules.required ? const ['required'] : const [];
    // Dart can contain lone UTF-16 surrogates; Rust strings cannot. Reject rather
    // than silently replacing malformed text while measuring UTF-8 or syncing it.
    if (!_wellFormed(value)) return const ['invalid_unicode'];
    if (value.length > maxInputBytes ||
        utf8.encode(value).length > maxInputBytes) return const ['too_large'];
    final errors = <String>[];
    if (rules.nonBlank && value.runes.every(_whiteSpace)) errors.add('blank');
    final chars = value.runes.length;
    if (rules.minChars != null && chars < rules.minChars!)
      errors.add('min_chars');
    if (rules.maxChars != null && chars > rules.maxChars!)
      errors.add('max_chars');
    final lines = logicalLines(value);
    if (rules.minLines != null && lines < rules.minLines!)
      errors.add('min_lines');
    if (rules.maxLines != null && lines > rules.maxLines!)
      errors.add('max_lines');
    switch (rules.kind) {
      case Kind.text:
        break;
      case Kind.email:
        if (!_email(value)) errors.add('email');
      case Kind.phoneE164:
        if (!_full(_phonePattern, value)) errors.add('phone_e164');
      case Kind.number:
      case Kind.integer:
        final integer = rules.kind == Kind.integer;
        final parsed = _full(integer ? _integerPattern : _numberPattern, value)
            ? double.tryParse(value)
            : null;
        if (parsed == null || !parsed.isFinite) {
          errors.add(integer ? 'integer' : 'number');
        } else if (integer && parsed.abs() > maxSafeInteger) {
          errors.add('unsafe_integer');
        } else {
          if (rules.minimum != null && parsed < rules.minimum!)
            errors.add('minimum');
          if (rules.maximum != null && parsed > rules.maximum!)
            errors.add('maximum');
        }
      case Kind.date:
        final parsed = _date(value);
        if (parsed == null) {
          errors.add('date');
        } else {
          if (_dateMin != null && parsed.isBefore(_dateMin))
            errors.add('date_min');
          if (_dateMax != null && parsed.isAfter(_dateMax))
            errors.add('date_max');
        }
    }
    return List.unmodifiable(errors);
  }

  /// Assign directly to TextFormField.validator or FormField<String>.validator.
  /// Localization must produce a non-empty message; invalid data cannot become
  /// valid because a translation is missing.
  String? Function(String?) validator(
          {String Function(String code)? localize}) =>
      (value) {
        final errors = validate(value);
        if (errors.isEmpty) return null;
        final code = errors.first;
        final message = localize?.call(code);
        return message == null || message.isEmpty ? code : message;
      };
}

bool _whiteSpace(int c) =>
    (c >= 9 && c <= 13) ||
    c == 32 ||
    c == 133 ||
    c == 160 ||
    c == 5760 ||
    (c >= 8192 && c <= 8202) ||
    c == 8232 ||
    c == 8233 ||
    c == 8239 ||
    c == 8287 ||
    c == 12288;

int logicalLines(String value) {
  var lines = 1;
  var previousCr = false;
  for (final c in value.runes) {
    if ((c == 13 || c == 10 || c == 133 || c == 8232 || c == 8233) &&
        !(c == 10 && previousCr)) lines++;
    previousCr = c == 13;
  }
  return lines;
}

bool _wellFormed(String value) {
  for (var i = 0; i < value.length; i++) {
    final c = value.codeUnitAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      if (++i == value.length) return false;
      final low = value.codeUnitAt(i);
      if (low < 0xdc00 || low > 0xdfff) return false;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      return false;
    }
  }
  return true;
}

bool _full(RegExp pattern, String value) {
  final match = pattern.firstMatch(value);
  return match != null && match.start == 0 && match.end == value.length;
}

final _phonePattern = RegExp(r'\+[1-9][0-9]{1,14}');
final _integerPattern = RegExp(r'-?(0|[1-9][0-9]*)');
final _numberPattern = RegExp(r'-?(0|[1-9][0-9]*)(\.[0-9]+)?');
// WHATWG email grammar, with the same explicit ASCII/mailbox profile as Rust.
final _emailPattern = RegExp(
    r"[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*");
bool _email(String value) {
  if (value.length > 254 || value.codeUnits.any((c) => c > 127)) return false;
  final at = value.indexOf('@');
  if (at <= 0 || at > 64) return false;
  final local = value.substring(0, at), domain = value.substring(at + 1);
  return !local.startsWith('.') &&
      !local.endsWith('.') &&
      !local.contains('..') &&
      domain.contains('.') &&
      _full(_emailPattern, value);
}

final _datePattern = RegExp(r'[0-9]{4}-[0-9]{2}-[0-9]{2}');
DateTime? _date(String value) {
  if (!_full(_datePattern, value)) return null;
  final year = int.parse(value.substring(0, 4));
  final month = int.parse(value.substring(5, 7));
  final day = int.parse(value.substring(8, 10));
  if (year == 0 || month < 1 || month > 12 || day < 1 || day > 31) return null;
  final date = DateTime.utc(year, month, day);
  return date.year == year && date.month == month && date.day == day
      ? date
      : null;
}

/// Value-free UI lifecycle shared with the Rust FieldState adapter.
final class FieldState {
  bool _touched = false, _submitted = false;
  List<String> _errors = const [];
  void edit(FieldValidator validator, String? value) {
    _errors = validator.validate(value);
  }

  void blur(FieldValidator validator, String? value) {
    _touched = true;
    edit(validator, value);
  }

  bool submit(FieldValidator validator, String? value) {
    _submitted = true;
    edit(validator, value);
    return _errors.isEmpty;
  }

  List<String> get visibleErrors => _touched || _submitted ? _errors : const [];
}
