import 'dart:convert';
import 'tjsv_probe_shared.dart';

void main() {
  const encoded = String.fromEnvironment('CORPUS_BASE64');
  final input = utf8.decode(base64.decode(encoded));
  print(jsonEncode(runProbe(jsonDecode(input) as Map<String, dynamic>)));
}
