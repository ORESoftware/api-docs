// Fixed stdin/stdout conformance adapter; no production options or parser.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../lib/rpc_v1.dart';

const _limit = 16 * 1024 * 1024;

void _require(bool condition, String message) {
  if (!condition) throw StateError(message);
}

Map<String, Object?> _evaluate(Map<String, dynamic> row) {
  final name = row['name'] as String;
  final kind = row['kind'] as String;
  final encoded = row['encoded'] as String;
  Object value;
  try {
    value = switch (kind) {
      'call' => decodeRpcV1Call(encoded),
      'receipt' => decodeRpcV1Receipt(encoded),
      _ => throw StateError('unknown probe kind'),
    };
  } on RpcV1Exception {
    return {'name': name, 'kind': kind, 'accepted': false};
  }
  // Only decoder exceptions count as rejection. Encoder failures and crashes
  // propagate to a failing process, never a successful negative observation.
  final bytes = switch (value) {
    RpcV1Call() => encodeRpcV1Call(value),
    RpcV1Receipt() => encodeRpcV1Receipt(value),
    _ => throw StateError('unexpected decoded type'),
  };
  return {
    'name': name,
    'kind': kind,
    'accepted': true,
    'encoded': utf8.decode(bytes),
  };
}

Future<void> main(List<String> args) async {
  try {
    _require(args.isEmpty, 'probe accepts no arguments');
    final input = BytesBuilder(copy: false);
    await for (final chunk in stdin) {
      _require(input.length + chunk.length <= _limit, 'probe input exceeds limit');
      input.add(chunk);
    }
    final document = jsonDecode(utf8.decode(input.takeBytes()));
    _require(document is Map<String, dynamic>, 'probe must be an object');
    final request = document as Map<String, dynamic>;
    _require(
      request.length == 2 &&
          request['schema'] == 'ores.api-docs.rpc-probe/v1' &&
          request['cases'] is List,
      'invalid probe schema or fields',
    );
    final cases = request['cases'] as List;
    _require(cases.isNotEmpty && cases.length <= 4096, 'invalid probe coverage');
    final names = <String>{};
    final results = <Map<String, Object?>>[];
    for (final value in cases) {
      _require(value is Map<String, dynamic>, 'case must be an object');
      final row = value as Map<String, dynamic>;
      _require(
        row.length == 3 &&
            row['name'] is String &&
            row['kind'] is String &&
            row['encoded'] is String,
        'invalid probe case fields',
      );
      final name = row['name'] as String;
      _require(name.isNotEmpty && names.add(name), 'empty or duplicate probe name');
      results.add(_evaluate(row));
    }
    stdout.writeln(jsonEncode({
      'schema': 'ores.api-docs.rpc-probe-result/v1',
      'runtime': 'dart',
      'results': results,
    }));
    await stdout.flush();
  } catch (error) {
    stderr.writeln('probe execution failed: $error');
    exitCode = 3;
  }
}
