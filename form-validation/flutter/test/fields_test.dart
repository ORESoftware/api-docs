import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ores_form_fields/ores_form_fields.dart';
import 'package:ores_form_validation/ores_form_validation.dart' as forms;

Widget fixture(GlobalKey<FormState> key, TextEditingController controller,
        forms.FieldValidator validator,
        {int visualLines = 1}) =>
    MaterialApp(
      home: Scaffold(
          body: Form(
              key: key,
              child: OresTextFormField(
                controller: controller,
                validator: validator,
                label: 'Input',
                visualMaxLines: visualLines,
              ))),
    );

void main() {
  testWidgets('Form.submit rejects invalid email and clears corrected errors',
      (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    final key = GlobalKey<FormState>();
    final validator = forms.FieldValidator(
        const forms.Rules(kind: forms.Kind.email, required: true));
    await tester.pumpWidget(fixture(key, controller, validator));
    expect(key.currentState!.validate(), isFalse);
    await tester.pump();
    expect(find.text('required'), findsOneWidget);
    await tester.enterText(find.byType(TextFormField), 'not-an-email');
    expect(key.currentState!.validate(), isFalse);
    await tester.pump();
    expect(find.text('email'), findsOneWidget);
    await tester.enterText(find.byType(TextFormField), 'person@example.com');
    expect(key.currentState!.validate(), isTrue);
    await tester.pump();
    expect(find.text('email'), findsNothing);
  });

  testWidgets('scalar counter agrees with server, not UTF16 or graphemes',
      (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    final key = GlobalKey<FormState>();
    final validator =
        forms.FieldValidator(const forms.Rules(required: true, maxChars: 1));
    await tester.pumpWidget(fixture(key, controller, validator));
    await tester.enterText(find.byType(TextFormField), '😀');
    expect(key.currentState!.validate(), isTrue);
    await tester.pump();
    expect(find.text('1 / 1'), findsOneWidget);
    await tester.enterText(find.byType(TextFormField), 'e\u0301');
    expect(key.currentState!.validate(), isFalse);
    await tester.pump();
    expect(find.text('2 / 1'), findsOneWidget);
    expect(find.text('max_chars'), findsOneWidget);
    expect(controller.text, 'e\u0301');
  });

  testWidgets('visual rows do not override the two-logical-line rule',
      (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    final key = GlobalKey<FormState>();
    final validator = forms.FieldValidator(const forms.Rules(maxLines: 2));
    await tester
        .pumpWidget(fixture(key, controller, validator, visualLines: 4));
    await tester.enterText(find.byType(TextFormField), 'one\ntwo');
    expect(key.currentState!.validate(), isTrue);
    await tester.enterText(find.byType(TextFormField), 'one\ntwo\nthree');
    expect(key.currentState!.validate(), isFalse);
    await tester.pump();
    expect(find.text('max_lines'), findsOneWidget);
  });
}
