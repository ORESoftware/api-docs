import '../lib/ores_form_validation.dart';

void check(bool value, String label) { if (!value) throw StateError(label); }

FieldValidator fromFixture(Map<String, dynamic> r) {
  const allowed = {'kind', 'required', 'non_blank', 'min_chars', 'max_chars', 'min_lines', 'max_lines', 'minimum', 'maximum', 'date_min', 'date_max'};
  check(r.keys.every(allowed.contains), 'unknown fixture rule');
  final kind = switch (r['kind']) {
    null || 'text' => Kind.text, 'email' => Kind.email, 'phone_e164' => Kind.phoneE164,
    'number' => Kind.number, 'integer' => Kind.integer, 'date' => Kind.date,
    _ => throw StateError('unknown fixture kind'),
  };
  return FieldValidator(Rules(kind: kind, required: r['required'] as bool? ?? false,
    nonBlank: r['non_blank'] as bool? ?? false,
    minChars: r['min_chars'] as int?, maxChars: r['max_chars'] as int?, minLines: r['min_lines'] as int?, maxLines: r['max_lines'] as int?,
    minimum: (r['minimum'] as num?)?.toDouble(), maximum: (r['maximum'] as num?)?.toDouble(),
    dateMin: r['date_min'] as String?, dateMax: r['date_max'] as String?));
}

void runCorpus(List<dynamic> cases) {
  check(cases.length >= 80, 'empty or truncated corpus');
  final ids = <String>{};
  for (final raw in cases) {
    final c = raw as Map<String, dynamic>;
    check(c.keys.every({'id', 'rules', 'value', 'errors'}.contains), 'unknown fixture key');
    final id = c['id'] as String;
    check(ids.add(id), 'duplicate fixture id');
    final actual = fromFixture(c['rules'] as Map<String, dynamic>).validate(c['value'] as String?);
    final expected = (c['errors'] as List).cast<String>();
    check(actual.length == expected.length && List.generate(actual.length, (i) => actual[i] == expected[i]).every((v) => v), 'fixture $id');
  }
  final invalid = <Rules>[
    const Rules(minChars: 3, maxChars: 2), const Rules(maxLines: 0), const Rules(minLines: 3, maxLines: 2),
    const Rules(minimum: 0), const Rules(kind: Kind.number, minimum: double.nan),
    const Rules(kind: Kind.number, maximum: double.infinity), const Rules(kind: Kind.number, minimum: 2, maximum: 1),
    const Rules(kind: Kind.integer, minimum: 0.5), const Rules(kind: Kind.integer, maximum: 9007199254740992),
    const Rules(dateMin: '2024-01-01'), const Rules(kind: Kind.date, dateMin: '2023-02-29'),
    const Rules(kind: Kind.date, dateMin: '2025-01-01', dateMax: '2024-01-01'), const Rules(minChars: maxInputBytes + 1),
    const Rules(minChars: -1),
  ];
  for (final rules in invalid) {
    var rejected = false;
    try { FieldValidator(rules); } on InvalidRules { rejected = true; }
    check(rejected, 'invalid rules accepted');
  }
  final text = FieldValidator(const Rules());
  check(text.validate('a' * maxInputBytes).isEmpty, 'inclusive byte cap');
  check(text.validate('a' * (maxInputBytes + 1)).single == 'too_large', 'byte cap');
  check(text.validate('😀' * (maxInputBytes ~/ 4 + 1)).single == 'too_large', 'unicode byte cap');
  check(text.validate(String.fromCharCode(0xd800)).single == 'invalid_unicode', 'unpaired surrogate');
  check(FieldValidator(const Rules(kind: Kind.number)).validate('9' * 400).single == 'number', 'numeric overflow');
  final required = FieldValidator(const Rules(required: true));
  final callback = required.validator(localize: (_) => '');
  check(callback('') == 'required', 'empty translation must not pass');
  check(callback('ok') == null, 'valid Flutter callback');
  final state = FieldState();
  state.edit(required, ''); check(state.visibleErrors.isEmpty, 'untouched errors hidden');
  state.blur(required, ''); check(state.visibleErrors.single == 'required', 'blur errors visible');
  state.edit(required, 'ok'); check(state.visibleErrors.isEmpty, 'stale errors cleared');
  check(!state.submit(required, null), 'submit revalidates');
  check(state.submit(required, 'ok'), 'corrected submission');
  print('${cases.length} shared fixtures plus configuration, boundary, callback and UI-state checks passed');
}
