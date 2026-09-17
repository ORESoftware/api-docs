import '../lib/generated.dart';

void assertRpcTraceChain(
  List<String> traceIds,
  String expectedRpc,
  String expectedHandler,
) {
  if (traceIds.isEmpty || traceIds.first != expectedRpc) {
    throw StateError('missing rpc trace $expectedRpc');
  }
  if (!traceIds.contains(expectedHandler)) {
    throw StateError('missing handler trace $expectedHandler');
  }
  if (traceIds.any((value) => value.contains('proof-http'))) {
    throw StateError('RPC unexpectedly passed through ordinary HTTP adapter');
  }
}

Future<void> main() async {
  final rpc = ProofRpcClient('http://127.0.0.1:39091');

  final created = await rpc.createUser(
    'tenant-dart',
    const CreateUserRequest(id: 'dart-user', displayName: 'Dart User'),
  );
  if (created.result.id != 'dart-user') throw StateError('create result mismatch');
  assertRpcTraceChain(
    created.traceIds,
    'ores-trace-proof-rpc-create-M7qT2vB9nLs',
    'ores-trace-proof-handler-create-W8dYzQ8fJ2N',
  );

  final found = await rpc.findUserById('dart-user', false, null);
  if (found.result.displayName != 'Dart User') throw StateError('find result mismatch');
  assertRpcTraceChain(
    found.traceIds,
    'ores-trace-proof-rpc-find-C5mR8xK2vQz',
    'ores-trace-proof-handler-find-bJ7mQ2vA1Ks',
  );

  final updated = await rpc.updateUser(
    'dart-user',
    'dart-idempotency-1',
    const UpdateUserRequest(displayName: 'Dart Updated'),
  );
  if (updated.result.displayName != 'Dart Updated') {
    throw StateError('update result mismatch');
  }
  assertRpcTraceChain(
    updated.traceIds,
    'ores-trace-proof-rpc-update-P4nV7sJ3bWt',
    'ores-trace-proof-handler-update-pR8tV5xC3Lm',
  );
}
