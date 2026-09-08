import 'dart:convert';
import 'dart:io';
import 'shared.dart';

void main() {
  final path = Platform.script.resolve('../../fixtures.json');
  runCorpus(jsonDecode(File.fromUri(path).readAsStringSync()) as List<dynamic>);
}
