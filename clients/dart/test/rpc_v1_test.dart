import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:ores_api_docs/rpc_v1.dart';
import 'package:test/test.dart';

void main() {
  final fixture = jsonDecode(
    File('../../examples/rpc-v1/conformance.json').readAsStringSync(),
  ) as Map<String, Object?>;

  test('fixture profile is v1 and distinct from RIDL v2', () {
    expect(fixture['profile'], 'ores-rpc-v1-call-receipt');
    expect(fixture['schemaVersion'], 1);
  });

  for (final value in fixture['valid']! as List<Object?>) {
    final testCase = Map<String, Object?>.from(value! as Map);
    final name = testCase['name']! as String;
    final kind = testCase['kind']! as String;
    final encoded = testCase['encoded']! as String;
    final prefix = testCase['tcp_prefix_hex']! as String;

    test('$name round-trips the exact shared bytes', () {
      final Object envelope = switch (kind) {
        'call' => decodeRpcV1Call(encoded),
        'receipt' => decodeRpcV1Receipt(encoded),
        _ => throw StateError('unknown fixture kind $kind'),
      };
      final bytes = switch (envelope) {
        RpcV1Call() => encodeRpcV1Call(envelope),
        RpcV1Receipt() => encodeRpcV1Receipt(envelope),
        _ => throw StateError('unknown envelope'),
      };
      expect(utf8.decode(bytes), encoded);
      expect(
        _hex(encodeRpcV1LengthPrefixed(envelope).sublist(0, 4)),
        prefix,
      );
    });
  }

  for (final value in fixture['invalid']! as List<Object?>) {
    final testCase = Map<String, Object?>.from(value! as Map);
    final name = testCase['name']! as String;
    final kind = testCase['kind']! as String;
    final encoded = testCase['encoded']! as String;
    test('$name fails closed', () {
      expect(
        () => kind == 'call'
            ? decodeRpcV1Call(encoded)
            : decodeRpcV1Receipt(encoded),
        throwsA(isA<RpcV1Exception>()),
      );
    });
  }

  test('absent and JSON-null bodies remain distinct', () {
    final absent = decodeRpcV1Receipt(
      '{"v":1,"op":"receipt","id":"c1","key":"healthz","ok":true}',
    );
    final presentNull = decodeRpcV1Receipt(
      '{"v":1,"op":"receipt","id":"c1","key":"healthz","ok":true,"body":null}',
    );
    expect(absent.body, isNull);
    expect(presentNull.body, isNotNull);
    expect(presentNull.body!.value, isNull);
  });

  test('correlation and transport mismatches fail closed', () {
    const call = RpcV1Call(
      id: 'c1',
      key: 'healthz',
      transport: RpcV1Transport.tcp,
    );
    final receipt = RpcV1Receipt.success(
      id: 'c2',
      key: 'healthz',
      transport: RpcV1Transport.tcp,
      status: 200,
    );
    expect(
      () => validateRpcV1ReceiptForCall(call, receipt),
      throwsA(isA<RpcV1Exception>()),
    );
  });

  test('NDJSON accepts one terminator and rejects multiple objects', () {
    final call = rpcV1CallFromNdjson(
      '{"v":1,"op":"call","id":"c1","key":"healthz"}\r\n',
    );
    expect(call.id, 'c1');
    expect(
      () => rpcV1CallFromNdjson(
        '{"v":1,"op":"call","id":"c1","key":"healthz"}\n{}\n',
      ),
      throwsA(isA<RpcV1Exception>()),
    );
  });

  test('length-prefix decoder is bounded and preserves partial tails', () {
    const call = RpcV1Call(id: 'c1', key: 'healthz');
    final first = encodeRpcV1LengthPrefixed(call);
    final second = encodeRpcV1LengthPrefixed(
      RpcV1Receipt.success(id: 'c1', key: 'healthz', status: 200),
    );
    final combined = Uint8List(first.length + 3)
      ..setRange(0, first.length, first)
      ..setRange(first.length, first.length + 3, second.sublist(0, 3));
    final split = splitRpcV1LengthPrefixed(combined);
    expect(split.frames, hasLength(1));
    expect(split.rest, hasLength(3));
    expect(
      () => splitRpcV1LengthPrefixed(
        Uint8List.fromList([0xff, 0xff, 0xff, 0xff]),
      ),
      throwsA(isA<RpcV1Exception>()),
    );
  });

  test('correlation identifiers are monotonic and bounded', () {
    final correlator = RpcV1Correlator('request-');
    expect([correlator.take(), correlator.take()], ['request-1', 'request-2']);
    expect(
      () => RpcV1Correlator(List.filled(128, 'x').join()),
      throwsA(isA<RpcV1Exception>()),
    );
  });
}

String _hex(List<int> bytes) =>
    bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
