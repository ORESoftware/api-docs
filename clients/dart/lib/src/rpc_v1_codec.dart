part of 'rpc_v1.dart';

Uint8List encodeRpcV1Call(RpcV1Call call) => _encode(call.toJson());

Uint8List encodeRpcV1Receipt(RpcV1Receipt receipt) => _encode(receipt.toJson());

RpcV1Call decodeRpcV1Call(Object payload) =>
    RpcV1Call.fromJson(_decodeObject(payload));

RpcV1Receipt decodeRpcV1Receipt(Object payload) =>
    RpcV1Receipt.fromJson(_decodeObject(payload));

String rpcV1CallToNdjson(RpcV1Call call) =>
    '${utf8.decode(encodeRpcV1Call(call))}\n';

String rpcV1ReceiptToNdjson(RpcV1Receipt receipt) =>
    '${utf8.decode(encodeRpcV1Receipt(receipt))}\n';

RpcV1Call rpcV1CallFromNdjson(Object payload) => decodeRpcV1Call(
      _stripOneTerminator(_payloadText(payload, extraBytes: 2)),
    );

RpcV1Receipt rpcV1ReceiptFromNdjson(Object payload) => decodeRpcV1Receipt(
      _stripOneTerminator(_payloadText(payload, extraBytes: 2)),
    );

Uint8List encodeRpcV1LengthPrefixed(Object envelope) {
  final payload = switch (envelope) {
    RpcV1Call() => encodeRpcV1Call(envelope),
    RpcV1Receipt() => encodeRpcV1Receipt(envelope),
    _ => throw const RpcV1Exception('unsupported v1 envelope type'),
  };
  final output = Uint8List(rpcV1LengthPrefixBytes + payload.length);
  ByteData.sublistView(output).setUint32(0, payload.length, Endian.big);
  output.setRange(rpcV1LengthPrefixBytes, output.length, payload);
  return output;
}

final class RpcV1SplitFrames {
  const RpcV1SplitFrames(this.frames, this.rest);

  final List<Uint8List> frames;
  final Uint8List rest;
}

RpcV1SplitFrames splitRpcV1LengthPrefixed(Uint8List buffer) {
  final frames = <Uint8List>[];
  final view = ByteData.sublistView(buffer);
  var offset = 0;
  while (buffer.length - offset >= rpcV1LengthPrefixBytes) {
    final length = view.getUint32(offset, Endian.big);
    if (length > rpcV1MaxFrameBytes) {
      throw RpcV1Exception(
        'declared frame length $length is over the $rpcV1MaxFrameBytes limit',
      );
    }
    final start = offset + rpcV1LengthPrefixBytes;
    if (buffer.length - start < length) break;
    frames.add(Uint8List.fromList(buffer.sublist(start, start + length)));
    offset = start + length;
  }
  return RpcV1SplitFrames(
    List.unmodifiable(frames),
    Uint8List.fromList(buffer.sublist(offset)),
  );
}

RpcV1Receipt validateRpcV1ReceiptForCall(
  RpcV1Call call,
  RpcV1Receipt receipt,
) {
  call.validate();
  receipt.validate();
  if (receipt.id != call.id) {
    throw const RpcV1Exception('receipt id does not match call id');
  }
  if (receipt.key != call.key) {
    throw const RpcV1Exception('receipt key does not match call key');
  }
  if (call.transport != null &&
      receipt.transport != null &&
      receipt.transport != call.transport) {
    throw const RpcV1Exception(
        'receipt transport does not match call transport');
  }
  return receipt;
}

final class RpcV1Correlator {
  RpcV1Correlator([this.prefix = '']) {
    if (!_isUnicodeScalarString(prefix) || prefix.runes.length >= 128) {
      throw const RpcV1Exception(
        'correlation prefix must contain fewer than 128 Unicode scalars',
      );
    }
  }

  final String prefix;
  int _next = 0;

  String take() {
    if (_next >= _maxSafeCorrelationId) {
      throw const RpcV1Exception('correlation id counter exhausted');
    }
    _next += 1;
    final value = '$prefix$_next';
    _validateString(value, 'correlation id', 128);
    return value;
  }
}
