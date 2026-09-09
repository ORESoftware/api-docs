import 'dart:convert';
import 'dart:io';
import 'tjsv_probe_shared.dart';

Future<void> main(List<String> args) async {
  if (args.isNotEmpty) throw StateError('fixed probe accepts no arguments');
  final input = await stdin.transform(utf8.decoder).join();
  if (input.length > 1048576) throw StateError('probe input budget');
  print(jsonEncode(runProbe(jsonDecode(input) as Map<String, dynamic>)));
}
