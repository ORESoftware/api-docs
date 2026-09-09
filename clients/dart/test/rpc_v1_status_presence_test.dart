import 'dart:convert';
import 'dart:typed_data';

import 'package:ores_api_docs/rpc_v1.dart';
import 'package:test/test.dart';

typedef ReceiptDecoder = RpcV1Receipt Function(Map<String, Object?>);

Map<String, Object?> _receipt(bool ok) => {
      'v': 1,
      'op': 'receipt',
      'id': 'status-presence',
      'key': 'healthz',
      'ok': ok,
      if (ok) 'body': null,
      if (!ok) 'error': <String, Object?>{'code': 'failed', 'detail': null},
    };

// Frame construction here deliberately bypasses the encoder: malformed input
// must reach the decoder rather than being rejected by a validating encoder.
RpcV1Receipt _lengthPrefixed(Map<String, Object?> raw) {
  final payload = utf8.encode(jsonEncode(raw));
  final packet = Uint8List(rpcV1LengthPrefixBytes + payload.length);
  ByteData.sublistView(packet).setUint32(0, payload.length, Endian.big);
  packet.setRange(rpcV1LengthPrefixBytes, packet.length, payload);
  final split = splitRpcV1LengthPrefixed(packet);
  expect(split.rest, isEmpty);
  expect(split.frames, hasLength(1));
  return decodeRpcV1Receipt(split.frames.single);
}

void main() {
  final decoders = <String, ReceiptDecoder>{
    'fromJson': RpcV1Receipt.fromJson,
    'JSON text': (raw) => decodeRpcV1Receipt(jsonEncode(raw)),
    'UTF-8 bytes': (raw) =>
        decodeRpcV1Receipt(Uint8List.fromList(utf8.encode(jsonEncode(raw)))),
    'NDJSON LF': (raw) => rpcV1ReceiptFromNdjson('${jsonEncode(raw)}\n'),
    'NDJSON CRLF': (raw) => rpcV1ReceiptFromNdjson('${jsonEncode(raw)}\r\n'),
    'length-prefixed payload': _lengthPrefixed,
  };

  for (final ok in [true, false]) {
    final state = ok ? 'success' : 'failure';
    final minimum = ok ? 200 : 400;
    final maximum = ok ? 399 : 599;
    final invalid = <String, Object?>{
      'explicit null': null,
      'string': '$minimum',
      'boolean': true,
      'fractional number': minimum + 0.5,
      'array': <Object?>[],
      'object': <String, Object?>{},
      'below state minimum': minimum - 1,
      'above state maximum': maximum + 1,
    };
    for (final decoder in decoders.entries) {
      for (final value in invalid.entries) {
        test('$state ${decoder.key} rejects ${value.key} status', () {
          final raw = _receipt(ok)..['status'] = value.value;
          expect(
            () => decoder.value(raw),
            throwsA(isA<RpcV1Exception>()),
          );
        });
      }

      test('$state ${decoder.key} preserves omitted status and payload null', () {
        final raw = _receipt(ok);
        final receipt = decoder.value(raw);
        expect(receipt.status, isNull);
        expect(receipt.toJson().containsKey('status'), isFalse);
        expect(receipt.toJson(), equals(raw));
      });

      for (final status in [minimum, maximum]) {
        test('$state ${decoder.key} preserves boundary status $status', () {
          final raw = _receipt(ok)..['status'] = status;
          final receipt = decoder.value(raw);
          expect(receipt.status, status);
          expect(receipt.toJson(), equals(raw));
        });
      }
    }
  }

  test('success factory null still means omitted status, not wire null', () {
    final receipt = RpcV1Receipt.success(
      id: 'status-presence',
      key: 'healthz',
      status: null,
      body: const RpcV1Body(null),
    );
    expect(receipt.toJson(), equals(_receipt(true)));
    expect(decodeRpcV1Receipt(encodeRpcV1Receipt(receipt)).status, isNull);
  });

  test('failure factory null still means omitted status, not wire null', () {
    final receipt = RpcV1Receipt.failure(
      id: 'status-presence',
      key: 'healthz',
      status: null,
      error: <String, Object?>{'code': 'failed', 'detail': null},
    );
    expect(receipt.toJson(), equals(_receipt(false)));
    expect(decodeRpcV1Receipt(encodeRpcV1Receipt(receipt)).status, isNull);
  });
}
