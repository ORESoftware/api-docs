import 'package:flutter/material.dart';
import 'package:ores_form_validation/ores_form_validation.dart' as forms;

/// Does not duplicate validation or truncate/normalize IME composition.
/// Visual line count is explicitly separate from the validator's logical lines.
class OresTextFormField extends StatelessWidget {
  const OresTextFormField({super.key, required this.controller,
    required this.validator, required this.label, this.localize,
    this.visualMinLines = 1, this.visualMaxLines = 1, this.onChanged,
    this.keyboardType, this.autovalidateMode = AutovalidateMode.onUserInteraction});

  final TextEditingController controller;
  final forms.FieldValidator validator;
  final String label;
  final String Function(String code)? localize;
  final int visualMinLines, visualMaxLines;
  final ValueChanged<String>? onChanged;
  final TextInputType? keyboardType;
  final AutovalidateMode autovalidateMode;

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<TextEditingValue>(
    valueListenable: controller,
    builder: (context, editing, child) => TextFormField(
      controller: controller,
      validator: validator.validator(localize: localize),
      autovalidateMode: autovalidateMode,
      minLines: visualMinLines,
      maxLines: visualMaxLines,
      keyboardType: keyboardType,
      onChanged: onChanged,
      // Flutter's built-in maxLength is grapheme-based. Do not install a
      // contradictory length limiter; use the portable scalar rule and counter.
      decoration: InputDecoration(labelText: label,
        counterText: validator.rules.maxChars == null
          ? '${editing.text.runes.length}'
          : '${editing.text.runes.length} / ${validator.rules.maxChars}'),
    ),
  );
}
