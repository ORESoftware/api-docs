import '../lib/generated.dart';

Future<void> main() async {
  final rpc = ProofRpcClient('http://127.0.0.1:39091');

  // This file is expected to fail `dart analyze`: the generated client surface
  // must reject payload/header argument types that differ from the backend spec.
  await rpc.createUser(
    'tenant',
    CreateUserRequest(id: 'bad', displayName: 42),
  );
  await rpc.updateUser(
    'bad',
    42,
    const UpdateUserRequest(displayName: 'Bad'),
  );
  await rpc.findUserById('bad', 'false', null);
}
