part of 'rpc_v1.dart';

void _validateCommon({
  required String id,
  required String key,
  required RpcV1Transport? transport,
  required String? traceId,
  required String? spanId,
}) {
  _validateString(id, 'id', 128);
  if (!RegExp(r'^[A-Za-z][A-Za-z0-9_]*$').hasMatch(key)) {
    throw const RpcV1Exception('key must be a portable RPC identifier');
  }
  if (traceId != null) _validateString(traceId, 'traceId', 64);
  if (spanId != null) _validateString(spanId, 'spanId', 32);
  // Referencing the enum keeps this exhaustive when a transport is added.
  switch (transport) {
    case RpcV1Transport.http:
    case RpcV1Transport.tcp:
    case RpcV1Transport.websocket:
    case RpcV1Transport.nats:
    case null:
      break;
  }
}

void _validateString(String value, String name, int maxRunes) {
  if (value.isEmpty || value.runes.length > maxRunes || !_isUnicodeScalarString(value)) {
    throw RpcV1Exception('$name must be 1..$maxRunes Unicode scalar values');
  }
}

void _validateJson(Object? value, String name) {
  final seen = HashSet<Object>.identity();
  void visit(Object? current, String path) {
    if (current == null || current is bool || current is int) return;
    if (current is double) {
      if (!current.isFinite) {
        throw RpcV1Exception('$path must be finite JSON');
      }
      return;
    }
    if (current is String) {
      if (!_isUnicodeScalarString(current)) {
        throw RpcV1Exception('$path must contain Unicode scalar values');
      }
      return;
    }
    if (current is List) {
      if (!seen.add(current)) throw RpcV1Exception('$path must not be cyclic');
      for (var index = 0; index < current.length; index += 1) {
        visit(current[index], '$path[$index]');
      }
      seen.remove(current);
      return;
    }
    if (current is Map) {
      if (!seen.add(current)) throw RpcV1Exception('$path must not be cyclic');
      for (final entry in current.entries) {
        if (entry.key is! String ||
            !_isUnicodeScalarString(entry.key as String)) {
          throw RpcV1Exception('$path object keys must be Unicode strings');
        }
        visit(entry.value, '$path.${entry.key}');
      }
      seen.remove(current);
      return;
    }
    throw RpcV1Exception('$name must contain one JSON value');
  }

  visit(value, name);
}

Uint8List _encode(Map<String, Object?> value) {
  late final String text;
  try {
    text = jsonEncode(value);
  } on Object catch (error) {
    if (error is RpcV1Exception) rethrow;
    throw RpcV1Exception('frame is not JSON-encodable: $error');
  }
  final bytes = Uint8List.fromList(utf8.encode(text));
  if (bytes.length > rpcV1MaxFrameBytes) {
    throw RpcV1Exception(
      'frame is ${bytes.length} bytes, over the $rpcV1MaxFrameBytes limit',
    );
  }
  return bytes;
}

Map<String, Object?> _decodeObject(Object payload) {
  final text = _payloadText(payload);
  final Object? decoded;
  try {
    decoded = jsonDecode(text);
  } on FormatException catch (error) {
    throw RpcV1Exception('frame is not JSON: $error');
  }
  if (decoded is! Map) {
    throw const RpcV1Exception('frame must be a JSON object');
  }
  final result = <String, Object?>{};
  for (final entry in decoded.entries) {
    if (entry.key is! String) {
      throw const RpcV1Exception('frame member names must be strings');
    }
    result[entry.key as String] = entry.value;
  }
  return result;
}

String _payloadText(Object payload, {int extraBytes = 0}) {
  if (payload is String) {
    final size = utf8.encode(payload).length;
    if (size > rpcV1MaxFrameBytes + extraBytes) {
      throw RpcV1Exception(
        'frame is $size bytes, over the $rpcV1MaxFrameBytes limit',
      );
    }
    return payload;
  }
  if (payload is List<int>) {
    if (payload.length > rpcV1MaxFrameBytes + extraBytes) {
      throw RpcV1Exception(
        'frame is ${payload.length} bytes, over the $rpcV1MaxFrameBytes limit',
      );
    }
    if (payload.any((value) => value < 0 || value > 255)) {
      throw const RpcV1Exception('frame byte values must be from 0 to 255');
    }
    try {
      return utf8.decode(payload, allowMalformed: false);
    } on FormatException catch (error) {
      throw RpcV1Exception('frame is not UTF-8: $error');
    }
  }
  throw const RpcV1Exception('payload must be a String or byte list');
}

String _stripOneTerminator(String value) {
  var text = value;
  if (text.endsWith('\r\n')) {
    text = text.substring(0, text.length - 2);
  } else if (text.endsWith('\n')) {
    text = text.substring(0, text.length - 1);
  }
  if (text.isEmpty) throw const RpcV1Exception('NDJSON input is empty');
  if (text.contains('\n') || text.contains('\r')) {
    throw const RpcV1Exception(
      'NDJSON input must contain exactly one JSON object',
    );
  }
  return text;
}

void _rejectUnknown(
  Map<String, Object?> raw,
  Set<String> allowed,
  String envelope,
) {
  final unknown = raw.keys.where((name) => !allowed.contains(name)).toList()..sort();
  if (unknown.isNotEmpty) {
    throw RpcV1Exception(
      'unknown $envelope member(s): ${unknown.join(', ')}',
    );
  }
}

int _requiredInt(Map<String, Object?> raw, String name) {
  final value = raw[name];
  if (!raw.containsKey(name) || value is! int) {
    throw RpcV1Exception('$name has the wrong type');
  }
  return value;
}

String _requiredString(Map<String, Object?> raw, String name) {
  final value = raw[name];
  if (!raw.containsKey(name) || value is! String) {
    throw RpcV1Exception('$name has the wrong type');
  }
  return value;
}

String? _optionalString(Map<String, Object?> raw, String name) {
  if (!raw.containsKey(name)) return null;
  final value = raw[name];
  if (value is! String) throw RpcV1Exception('$name has the wrong type');
  return value;
}

Map<String, Object?>? _optionalObject(
  Map<String, Object?> raw,
  String name,
) {
  if (!raw.containsKey(name)) return null;
  final value = raw[name];
  if (value is! Map) throw RpcV1Exception('$name must be a JSON object');
  final result = <String, Object?>{};
  for (final entry in value.entries) {
    if (entry.key is! String) {
      throw RpcV1Exception('$name object keys must be strings');
    }
    result[entry.key as String] = entry.value;
  }
  return result;
}

bool _isUnicodeScalarString(String value) {
  final units = value.codeUnits;
  for (var index = 0; index < units.length; index += 1) {
    final unit = units[index];
    if (unit >= 0xd800 && unit <= 0xdbff) {
      if (index + 1 >= units.length) return false;
      final next = units[++index];
      if (next < 0xdc00 || next > 0xdfff) return false;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return false;
    }
  }
  return true;
}
