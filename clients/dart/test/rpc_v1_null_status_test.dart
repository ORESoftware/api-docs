import 'dart:convert';
import 'dart:typed_data';

import 'package:ores_api_docs/rpc_v1.dart';
import 'package:test/test.dart';

void main() {
  for (final ok in [true, false]) {
    final raw = <String, Object?>{
      'v': 1,
      'op': 'receipt',
      'id': 'null-status',
      'key': 'healthz',
      'ok': ok,
      if (!ok) 'error': <String, Object?>{},
    };
    final withNull = <String, Object?>{...raw, 'status': null};
    final encoded = jsonEncode(withNull);

    test('ok=$ok rejects explicit null status in fromJson', () {
      expect(
        () => RpcV1Receipt.fromJson(withNull),
        throwsA(isA<RpcV1Exception>()),
      );
    });

    test('ok=$ok rejects explicit null status in JSON text', () {
      expect(
        () => decodeRpcV1Receipt(encoded),
        throwsA(isA<RpcV1Exception>()),
      );
    });

    test('ok=$ok rejects explicit null status in UTF-8 bytes', () {
      expect(
        () => decodeRpcV1Receipt(Uint8List.fromList(utf8.encode(encoded))),
        throwsA(isA<RpcV1Exception>()),
      );
    });

    test('ok=$ok rejects explicit null status in NDJSON', () {
      expect(
        () => rpcV1ReceiptFromNdjson('$encoded\r\n'),
        throwsA(isA<RpcV1Exception>()),
      );
    });

    test('ok=$ok rejects null status after splitting a length-prefixed frame', () {
      final bytes = utf8.encode(encoded);
      final framed = Uint8List(rpcV1LengthPrefixBytes + bytes.length);
      ByteData.sublistView(framed).setUint32(0, bytes.length, Endian.big);
      framed.setRange(rpcV1LengthPrefixBytes, framed.length, bytes);
      final split = splitRpcV1LengthPrefixed(framed);
      expect(split.frames, hasLength(1));
      expect(split.rest, isEmpty);
      expect(
        () => decodeRpcV1Receipt(split.frames.single),
        throwsA(isA<RpcV1Exception>()),
      );
    });

    test('ok=$ok preserves absent status through decode and encode', () {
      final receipt = decodeRpcV1Receipt(jsonEncode(raw));
      expect(receipt.ok, ok);
      expect(receipt.status, isNull);
      expect(receipt.toJson().containsKey('status'), isFalse);
      expect(jsonDecode(utf8.decode(encodeRpcV1Receipt(receipt))), raw);
    });

    test('ok=$ok preserves valid integer status through decode and encode', () {
      final withStatus = <String, Object?>{...raw, 'status': ok ? 200 : 500};
      final receipt = decodeRpcV1Receipt(jsonEncode(withStatus));
      expect(receipt.ok, ok);
      expect(receipt.status, withStatus['status']);
      expect(jsonDecode(utf8.decode(encodeRpcV1Receipt(receipt))), withStatus);
    });
  }
}
