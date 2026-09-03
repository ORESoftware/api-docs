/// Transport-neutral RIDL frame codec for Flutter, browsers, and Dart servers.
///
/// HTTP does not use this envelope. WebSocket carries one encoded frame per
/// text message; TCP carries a four-byte big-endian length followed by the same
/// canonical UTF-8 JSON bytes.
import 'dart:convert';
import 'dart:typed_data';

const int frameVersion = 1;
const int maxFrameBytes = 8 * 1024 * 1024;
const int lengthPrefixBytes = 4;

const Set<String> _allowedFields = {
  'v',
  'id',
  't',
  'key',
  'method',
  'path',
  'query',
  'body',
  'code',
  'message',
  'meta',
};

enum FrameKind { call, data, end, error, cancel }

extension FrameKindWire on FrameKind {
  String get wireName => name;
}

FrameKind _parseFrameKind(String value) => switch (value) {
      'call' => FrameKind.call,
      'data' => FrameKind.data,
      'end' => FrameKind.end,
      'error' => FrameKind.error,
      'cancel' => FrameKind.cancel,
      _ => throw FrameException('unknown frame type $value'),
    };

final class FrameException implements Exception {
  const FrameException(this.message);

  final String message;

  @override
  String toString() => 'FrameException: $message';
}

final class QueryPair {
  const QueryPair(this.name, this.value);

  final String name;
  final String value;
}

/// Wrapper used where JSON `null` must remain distinct from an absent body.
final class PresentBody {
  const PresentBody(this.value);

  final Object? value;
}

final class Frame {
  Frame._({
    required this.version,
    required this.id,
    required this.kind,
    this.key,
    this.method,
    this.path,
    List<QueryPair> query = const [],
    this.body,
    required this.hasBody,
    this.code,
    this.message,
    Map<String, String> meta = const {},
  })  : query = List.unmodifiable(query),
        meta = Map.unmodifiable(meta);

  factory Frame.call({
    required String id,
    required String key,
    required String method,
    required String path,
    List<QueryPair> query = const [],
    PresentBody? body,
    Map<String, String> meta = const {},
  }) =>
      Frame._(
        version: frameVersion,
        id: id,
        kind: FrameKind.call,
        key: key,
        method: method,
        path: path,
        query: query,
        body: body?.value,
        hasBody: body != null,
        meta: meta,
      );

  factory Frame.data({required String id, required Object? body}) => Frame._(
        version: frameVersion,
        id: id,
        kind: FrameKind.data,
        body: body,
        hasBody: true,
      );

  factory Frame.end({required String id}) => Frame._(
        version: frameVersion,
        id: id,
        kind: FrameKind.end,
        hasBody: false,
      );

  factory Frame.cancel({required String id}) => Frame._(
        version: frameVersion,
        id: id,
        kind: FrameKind.cancel,
        hasBody: false,
      );

  factory Frame.error({
    required String id,
    required String code,
    String? message,
    PresentBody? body,
    Map<String, String> meta = const {},
  }) =>
      Frame._(
        version: frameVersion,
        id: id,
        kind: FrameKind.error,
        body: body?.value,
        hasBody: body != null,
        code: code,
        message: message,
        meta: meta,
      );

  final int version;
  final String id;
  final FrameKind kind;
  final String? key;
  final String? method;
  final String? path;
  final List<QueryPair> query;
  final Object? body;
  final bool hasBody;
  final String? code;
  final String? message;
  final Map<String, String> meta;

  Frame withMeta(String name, String value) {
    final next = Map<String, String>.of(meta)..[name] = value;
    return Frame._(
      version: version,
      id: id,
      kind: kind,
      key: key,
      method: method,
      path: path,
      query: query,
      body: body,
      hasBody: hasBody,
      code: code,
      message: message,
      meta: next,
    );
  }

  void validate() {
    if (version != frameVersion) {
      throw FrameException('unsupported frame version $version');
    }
    if (id.isEmpty || id.runes.length > 128 || !_isUnicodeScalarString(id)) {
      throw const FrameException('id must be 1..128 Unicode scalar values');
    }
    if (kind == FrameKind.call) {
      if (key == null || key!.isEmpty || !_isUnicodeScalarString(key!)) {
        throw const FrameException('a call frame needs an operation key');
      }
      if (method == null || method!.isEmpty || !_isUnicodeScalarString(method!)) {
        throw const FrameException('a call frame needs a method');
      }
      if (path == null || !path!.startsWith('/') || !_isUnicodeScalarString(path!)) {
        throw const FrameException('a call frame needs a path starting with /');
      }
    } else if (key != null || method != null || path != null || query.isNotEmpty) {
      throw FrameException('a ${kind.wireName} frame carries no addressing fields');
    }
    for (var index = 0; index < query.length; index += 1) {
      final pair = query[index];
      if (!_isUnicodeScalarString(pair.name) || !_isUnicodeScalarString(pair.value)) {
        throw FrameException('query[$index] must contain Unicode scalar strings');
      }
    }
    if (kind == FrameKind.data && !hasBody) {
      throw const FrameException('a data frame needs a body');
    }
    if (kind == FrameKind.error) {
      if (code == null || code!.isEmpty || !_isUnicodeScalarString(code!)) {
        throw const FrameException('an error frame needs a code');
      }
      if (message != null && !_isUnicodeScalarString(message!)) {
        throw const FrameException('an error message must contain Unicode scalars');
      }
    } else if (code != null || message != null) {
      throw FrameException('a ${kind.wireName} frame carries no code or message');
    }
    if (hasBody && !_isJsonValue(body)) {
      throw const FrameException('body must contain one JSON value');
    }
    for (final entry in meta.entries) {
      if (!_isUnicodeScalarString(entry.key) || !_isUnicodeScalarString(entry.value)) {
        throw FrameException('meta.${entry.key} must contain Unicode scalar strings');
      }
    }
  }

  Map<String, Object?> toCanonicalObject() {
    validate();
    final raw = <String, Object?>{
      'v': version,
      'id': id,
      't': kind.wireName,
    };
    if (kind == FrameKind.call) {
      raw['key'] = key;
      raw['method'] = method;
      raw['path'] = path;
      if (query.isNotEmpty) {
        raw['query'] = query.map((pair) => [pair.name, pair.value]).toList(growable: false);
      }
    }
    if (hasBody) raw['body'] = body;
    if (kind == FrameKind.error) {
      raw['code'] = code;
      if (message != null) raw['message'] = message;
    }
    if (meta.isNotEmpty) {
      final keys = meta.keys.toList()..sort();
      raw['meta'] = <String, String>{for (final key in keys) key: meta[key]!};
    }
    return raw;
  }
}

Uint8List encodeFrame(Frame frame) {
  late final String text;
  try {
    text = jsonEncode(frame.toCanonicalObject());
  } on Object catch (error) {
    if (error is FrameException) rethrow;
    throw FrameException('frame is not JSON-encodable: $error');
  }
  final bytes = Uint8List.fromList(utf8.encode(text));
  if (bytes.length > maxFrameBytes) {
    throw FrameException(
      'frame is ${bytes.length} bytes, over the $maxFrameBytes limit',
    );
  }
  return bytes;
}

Uint8List encodeTcp(Frame frame) {
  final payload = encodeFrame(frame);
  final output = Uint8List(lengthPrefixBytes + payload.length);
  ByteData.sublistView(output).setUint32(0, payload.length, Endian.big);
  output.setRange(lengthPrefixBytes, output.length, payload);
  return output;
}

Frame decodeFrame(Object payload) {
  final String text;
  if (payload is String) {
    final size = utf8.encode(payload).length;
    if (size > maxFrameBytes) {
      throw FrameException('frame is $size bytes, over the $maxFrameBytes limit');
    }
    text = payload;
  } else if (payload is List<int>) {
    if (payload.length > maxFrameBytes) {
      throw FrameException(
        'frame is ${payload.length} bytes, over the $maxFrameBytes limit',
      );
    }
    if (payload.any((value) => value < 0 || value > 255)) {
      throw const FrameException('frame byte values must be between 0 and 255');
    }
    try {
      text = utf8.decode(payload, allowMalformed: false);
    } on FormatException catch (error) {
      throw FrameException('frame is not UTF-8: $error');
    }
  } else {
    throw const FrameException('frame input must be a String or byte list');
  }

  final Object? decoded;
  try {
    decoded = jsonDecode(text);
  } on FormatException catch (error) {
    throw FrameException('frame is not JSON: $error');
  }
  if (decoded is! Map) {
    throw const FrameException('a frame must be a JSON object');
  }
  final raw = <String, Object?>{};
  for (final entry in decoded.entries) {
    if (entry.key is! String) {
      throw const FrameException('frame member names must be strings');
    }
    raw[entry.key as String] = entry.value;
  }
  final unknown = raw.keys.where((key) => !_allowedFields.contains(key)).toList()..sort();
  if (unknown.isNotEmpty) {
    throw FrameException('unknown frame member(s): ${unknown.join(', ')}');
  }

  final version = _requiredInt(raw, 'v');
  final id = _requiredString(raw, 'id');
  final kind = _parseFrameKind(_requiredString(raw, 't'));
  final key = _optionalString(raw, 'key');
  final method = _optionalString(raw, 'method');
  final path = _optionalString(raw, 'path');
  final query = <QueryPair>[];
  if (raw.containsKey('query')) {
    final value = raw['query'];
    if (value is! List) {
      throw const FrameException('query must be an array of [name, value] pairs');
    }
    for (final pair in value) {
      if (pair is! List || pair.length != 2 || pair[0] is! String || pair[1] is! String) {
        throw const FrameException(
          'each query entry must be a [name, value] pair of strings',
        );
      }
      query.add(QueryPair(pair[0] as String, pair[1] as String));
    }
  }

  final meta = <String, String>{};
  if (raw.containsKey('meta')) {
    final value = raw['meta'];
    if (value is! Map) {
      throw const FrameException('meta must be an object of strings');
    }
    for (final entry in value.entries) {
      if (entry.key is! String || entry.value is! String) {
        throw const FrameException('meta must be an object of strings');
      }
      meta[entry.key as String] = entry.value as String;
    }
  }

  final frame = Frame._(
    version: version,
    id: id,
    kind: kind,
    key: key,
    method: method,
    path: path,
    query: query,
    body: raw['body'],
    hasBody: raw.containsKey('body'),
    code: _optionalString(raw, 'code'),
    message: _optionalString(raw, 'message'),
    meta: meta,
  );
  frame.validate();
  return frame;
}

final class DecodedFrames {
  const DecodedFrames(this.frames, this.rest);

  final List<Frame> frames;
  final Uint8List rest;
}

DecodedFrames decodeStream(Uint8List buffer) {
  final frames = <Frame>[];
  final view = ByteData.sublistView(buffer);
  var offset = 0;
  while (buffer.length - offset >= lengthPrefixBytes) {
    final length = view.getUint32(offset, Endian.big);
    if (length > maxFrameBytes) {
      throw FrameException(
        'declared frame length $length is over the $maxFrameBytes limit',
      );
    }
    final start = offset + lengthPrefixBytes;
    if (buffer.length - start < length) break;
    frames.add(decodeFrame(Uint8List.sublistView(buffer, start, start + length)));
    offset = start + length;
  }
  return DecodedFrames(
    List.unmodifiable(frames),
    Uint8List.fromList(buffer.sublist(offset)),
  );
}

final class Correlator {
  Correlator([this.prefix = '']);

  final String prefix;
  int _next = 0;

  String take() {
    _next += 1;
    return '$prefix$_next';
  }
}

int _requiredInt(Map<String, Object?> raw, String name) {
  final value = raw[name];
  if (!raw.containsKey(name) || value is! int) {
    throw FrameException('$name has the wrong type');
  }
  return value;
}

String _requiredString(Map<String, Object?> raw, String name) {
  final value = raw[name];
  if (!raw.containsKey(name) || value is! String) {
    throw FrameException('$name has the wrong type');
  }
  return value;
}

String? _optionalString(Map<String, Object?> raw, String name) {
  if (!raw.containsKey(name)) return null;
  final value = raw[name];
  if (value is! String) throw FrameException('$name has the wrong type');
  return value;
}

bool _isJsonValue(Object? value) {
  if (value == null || value is bool || value is int) return true;
  if (value is double) return value.isFinite;
  if (value is String) return _isUnicodeScalarString(value);
  if (value is List) return value.every(_isJsonValue);
  if (value is Map) {
    return value.entries.every(
      (entry) =>
          entry.key is String &&
          _isUnicodeScalarString(entry.key as String) &&
          _isJsonValue(entry.value),
    );
  }
  return false;
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
