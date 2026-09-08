import 'dart:convert';
import 'shared.dart';

// CI passes the exact same checked-in synthetic corpus into the JS compilation.
// No file-system, browser DOM, Flutter engine, network or sync dependency.
void main() {
  const corpus = String.fromEnvironment('CORPUS');
  runCorpus(jsonDecode(corpus) as List<dynamic>);
}
