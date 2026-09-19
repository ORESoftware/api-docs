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
    'ores-trace-HA55l7mbjBwL3g7kcFatR',
    'ores-trace-kyWJwSSkCRw6JPP1fGBXa',
  );

  final found = await rpc.findUserById('dart-user', false, null);
  if (found.result.displayName != 'Dart User') throw StateError('find result mismatch');
  assertRpcTraceChain(
    found.traceIds,
    'ores-trace-tuxPrC6DrxG1JraioBRdE',
    'ores-trace-k9e5kg-cYX1JRJzeGhXQJ',
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
    'ores-trace-WXw41GYs3E6rSSfesc9RC',
    'ores-trace-1DJT7X2n4_bzNCwb3dhQy',
  );
}
