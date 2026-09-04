part of 'rpc_v1.dart';

const int rpcV1Version = 1;
const int rpcV1MaxFrameBytes = 8 * 1024 * 1024;
const int rpcV1LengthPrefixBytes = 4;
const int _maxSafeCorrelationId = 9007199254740991;

const Set<String> _callFields = {
  'v',
  'op',
  'id',
  'key',
  'transport',
  'path',
  'query',
  'headers',
  'body',
  'traceId',
  'spanId',
};
const Set<String> _receiptFields = {
  'v',
  'op',
  'id',
  'key',
  'transport',
  'ok',
  'status',
  'body',
  'error',
  'traceId',
  'spanId',
};

enum RpcV1Transport { http, tcp, websocket, nats }

extension RpcV1TransportWire on RpcV1Transport {
  String get wireName => name;
}

RpcV1Transport _parseTransport(String value) => switch (value) {
      'http' => RpcV1Transport.http,
      'tcp' => RpcV1Transport.tcp,
      'websocket' => RpcV1Transport.websocket,
      'nats' => RpcV1Transport.nats,
      _ => throw RpcV1Exception('unknown transport $value'),
    };

final class RpcV1Exception implements Exception {
  const RpcV1Exception(this.message);

  final String message;

  @override
  String toString() => 'RpcV1Exception: $message';
}

/// Wrapper used where JSON `null` must remain distinct from an absent member.
final class RpcV1Body {
  const RpcV1Body(this.value);

  final Object? value;
}

final class RpcV1Call {
  const RpcV1Call({
    required this.id,
    required this.key,
    this.transport,
    this.path,
    this.query,
    this.headers,
    this.body,
    this.traceId,
    this.spanId,
  });

  final String id;
  final String key;
  final RpcV1Transport? transport;
  final Map<String, Object?>? path;
  final Map<String, Object?>? query;
  final Map<String, Object?>? headers;
  final RpcV1Body? body;
  final String? traceId;
  final String? spanId;

  void validate() {
    _validateCommon(
      id: id,
      key: key,
      transport: transport,
      traceId: traceId,
      spanId: spanId,
    );
    if (path != null) _validateJson(path, 'path');
    if (query != null) _validateJson(query, 'query');
    if (headers != null) _validateHeaders(headers!);
    if (body != null) _validateJson(body!.value, 'body');
  }

  Map<String, Object?> toJson() {
    validate();
    final result = <String, Object?>{
      'v': rpcV1Version,
      'op': 'call',
      'id': id,
      'key': key,
    };
    if (transport != null) result['transport'] = transport!.wireName;
    if (path != null) result['path'] = path;
    if (query != null) result['query'] = query;
    if (headers != null) result['headers'] = headers;
    if (body != null) result['body'] = body!.value;
    if (traceId != null) result['traceId'] = traceId;
    if (spanId != null) result['spanId'] = spanId;
    return result;
  }

  factory RpcV1Call.fromJson(Map<String, Object?> raw) {
    _rejectUnknown(raw, _callFields, 'call');
    if (_requiredInt(raw, 'v') != rpcV1Version) {
      throw RpcV1Exception('unsupported RPC version ${raw['v']}');
    }
    if (_requiredString(raw, 'op') != 'call') {
      throw const RpcV1Exception('expected op call');
    }
    final transportName = _optionalString(raw, 'transport');
    final call = RpcV1Call(
      id: _requiredString(raw, 'id'),
      key: _requiredString(raw, 'key'),
      transport: transportName == null ? null : _parseTransport(transportName),
      path: _optionalObject(raw, 'path'),
      query: _optionalObject(raw, 'query'),
      headers: _optionalObject(raw, 'headers'),
      body: raw.containsKey('body') ? RpcV1Body(raw['body']) : null,
      traceId: _optionalString(raw, 'traceId'),
      spanId: _optionalString(raw, 'spanId'),
    );
    call.validate();
    return call;
  }
}


final RegExp _headerName = RegExp(r"^[!#$%&'*+.^_`|~0-9a-z-]+$");

void _validateHeaders(Map<String, Object?> headers) {
  for (final entry in headers.entries) {
    final name = entry.key;
    if (name.length > 128 || !_headerName.hasMatch(name)) {
      throw RpcV1Exception(
        'header name $name must be a canonical lowercase HTTP field name',
      );
    }
    _validateJson(entry.value, 'headers.$name');
  }
}

final class RpcV1Receipt {
  const RpcV1Receipt._({
    required this.id,
    required this.key,
    required this.ok,
    this.transport,
    this.status,
    this.body,
    this.error,
    this.traceId,
    this.spanId,
  });

  factory RpcV1Receipt.success({
    required String id,
    required String key,
    int? status,
    RpcV1Transport? transport,
    RpcV1Body? body,
    String? traceId,
    String? spanId,
  }) =>
      RpcV1Receipt._(
        id: id,
        key: key,
        ok: true,
        status: status,
        transport: transport,
        body: body,
        traceId: traceId,
        spanId: spanId,
      );

  factory RpcV1Receipt.failure({
    required String id,
    required String key,
    required Map<String, Object?> error,
    int? status,
    RpcV1Transport? transport,
    String? traceId,
    String? spanId,
  }) =>
      RpcV1Receipt._(
        id: id,
        key: key,
        ok: false,
        status: status,
        transport: transport,
        error: error,
        traceId: traceId,
        spanId: spanId,
      );

  final String id;
  final String key;
  final RpcV1Transport? transport;
  final bool ok;
  final int? status;
  final RpcV1Body? body;
  final Map<String, Object?>? error;
  final String? traceId;
  final String? spanId;

  void validate() {
    _validateCommon(
      id: id,
      key: key,
      transport: transport,
      traceId: traceId,
      spanId: spanId,
    );
    if (status != null && (status! < 100 || status! > 599)) {
      throw const RpcV1Exception('status must be an integer from 100 to 599');
    }
    if (body != null) _validateJson(body!.value, 'body');
    if (error != null) _validateJson(error, 'error');
    if (ok) {
      if (error != null) {
        throw const RpcV1Exception('a successful receipt must not carry error');
      }
      if (status != null && (status! < 200 || status! > 399)) {
        throw const RpcV1Exception(
          'a successful receipt status must be from 200 to 399',
        );
      }
    } else {
      if (body != null) {
        throw const RpcV1Exception('an error receipt must not carry body');
      }
      if (error == null) {
        throw const RpcV1Exception('an error receipt needs error');
      }
      if (status != null && (status! < 400 || status! > 599)) {
        throw const RpcV1Exception(
          'an error receipt status must be from 400 to 599',
        );
      }
    }
  }

  Map<String, Object?> toJson() {
    validate();
    final result = <String, Object?>{
      'v': rpcV1Version,
      'op': 'receipt',
      'id': id,
      'key': key,
    };
    if (transport != null) result['transport'] = transport!.wireName;
    result['ok'] = ok;
    if (status != null) result['status'] = status;
    if (body != null) result['body'] = body!.value;
    if (error != null) result['error'] = error;
    if (traceId != null) result['traceId'] = traceId;
    if (spanId != null) result['spanId'] = spanId;
    return result;
  }

  factory RpcV1Receipt.fromJson(Map<String, Object?> raw) {
    _rejectUnknown(raw, _receiptFields, 'receipt');
    if (_requiredInt(raw, 'v') != rpcV1Version) {
      throw RpcV1Exception('unsupported RPC version ${raw['v']}');
    }
    if (_requiredString(raw, 'op') != 'receipt') {
      throw const RpcV1Exception('expected op receipt');
    }
    final transportName = _optionalString(raw, 'transport');
    final ok = raw['ok'];
    if (ok is! bool) throw const RpcV1Exception('ok must be a boolean');
    final status = raw['status'];
    if (status != null && status is! int) {
      throw const RpcV1Exception('status must be an integer');
    }
    final receipt = RpcV1Receipt._(
      id: _requiredString(raw, 'id'),
      key: _requiredString(raw, 'key'),
      transport: transportName == null ? null : _parseTransport(transportName),
      ok: ok,
      status: status as int?,
      body: raw.containsKey('body') ? RpcV1Body(raw['body']) : null,
      error: _optionalObject(raw, 'error'),
      traceId: _optionalString(raw, 'traceId'),
      spanId: _optionalString(raw, 'spanId'),
    );
    receipt.validate();
    return receipt;
  }
}
