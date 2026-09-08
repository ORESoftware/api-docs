import 'dart:convert';
import 'shared.dart';

// Base64 avoids dart2js command-line parsing of multiline source data.
// CI passes the exact checked-in synthetic corpus, without transformation.
void main() {
  const encoded = String.fromEnvironment('CORPUS_BASE64');
  runCorpus(jsonDecode(utf8.decode(base64.decode(encoded))) as List<dynamic>);
}
