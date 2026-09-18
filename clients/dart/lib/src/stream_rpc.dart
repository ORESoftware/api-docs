enum RpcStreamCarrier { websocket, tcp }

enum RpcStreamFrameKind { data, end, error, cancel, call }

final class RpcStreamFrame {
  const RpcStreamFrame._({
    required this.id,
    required this.kind,
    this.body,
    required this.hasBody,
    this.code,
    this.message,
  });

  factory RpcStreamFrame.data({
    required String id,
    required Object? body,
  }) =>
      RpcStreamFrame._(
        id: id,
        kind: RpcStreamFrameKind.data,
        body: body,
        hasBody: true,
      );

  factory RpcStreamFrame.end({required String id}) => RpcStreamFrame._(
        id: id,
        kind: RpcStreamFrameKind.end,
        hasBody: false,
      );

  factory RpcStreamFrame.error({
    required String id,
    required String code,
    String? message,
  }) =>
      RpcStreamFrame._(
        id: id,
        kind: RpcStreamFrameKind.error,
        hasBody: false,
        code: code,
        message: message,
      );

  factory RpcStreamFrame.cancel({required String id}) => RpcStreamFrame._(
        id: id,
        kind: RpcStreamFrameKind.cancel,
        hasBody: false,
      );

  factory RpcStreamFrame.call({required String id}) => RpcStreamFrame._(
        id: id,
        kind: RpcStreamFrameKind.call,
        hasBody: false,
      );

  final String id;
  final RpcStreamFrameKind kind;
  final Object? body;
  final bool hasBody;
  final String? code;
  final String? message;
}

final class RpcStreamCall {
  const RpcStreamCall({
    required this.id,
    required this.key,
    required this.method,
    required this.path,
    this.query = const [],
    this.body,
  });

  final String id;
  final String key;
  final String method;
  final String path;
  final List<(String, String)> query;
  final Object? body;
}

abstract interface class RpcStreamSession {
  Stream<RpcStreamFrame> get incoming;
  Future<void> cancel();
}

abstract interface class FramedRpcStream {
  RpcStreamCarrier get carrier;
  Future<RpcStreamSession> open(RpcStreamCall call);
}

final class RpcStreamException implements Exception {
  const RpcStreamException(this.message, {this.code});

  final String message;
  final String? code;

  @override
  String toString() => code == null
      ? 'RpcStreamException: $message'
      : 'RpcStreamException($code): $message';
}

final class RpcStreamContext {
  const RpcStreamContext({
    required this.id,
    required this.key,
    required this.carrier,
    required this.ended,
    required this.cancelled,
    this.error,
  });

  final String id;
  final String key;
  final RpcStreamCarrier carrier;
  final bool ended;
  final bool cancelled;
  final Object? error;
}

final class RpcStreamRequest {
  const RpcStreamRequest({
    required this.method,
    required this.path,
    this.query = const [],
    this.body,
  });

  final String method;
  final String path;
  final List<(String, String)> query;
  final Object? body;
}

final class RpcStreamClient<T> {
  RpcStreamClient._({
    required RpcStreamSession session,
    required String id,
    required String key,
    required RpcStreamCarrier carrier,
    required T Function(Object?) decoder,
  })  : _session = session,
        _id = id,
        _key = key,
        _carrier = carrier,
        _decoder = decoder;

  final RpcStreamSession _session;
  final String _id;
  final String _key;
  final RpcStreamCarrier _carrier;
  final T Function(Object?) _decoder;

  bool _ended = false;
  bool _cancelled = false;
  Object? _error;

  RpcStreamContext get context => RpcStreamContext(
        id: _id,
        key: _key,
        carrier: _carrier,
        ended: _ended,
        cancelled: _cancelled,
        error: _error,
      );

  Future<void> cancel() async {
    if (_ended || _cancelled) return;
    try {
      await _session.cancel();
      _cancelled = true;
    } on Object catch (error) {
      final wrapped = RpcStreamException(
        '$_key: stream cancellation failed: $error',
      );
      _error = wrapped;
      throw wrapped;
    }
  }

  Stream<T> get values async* {
    var sawTerminal = false;
    try {
      await for (final frame in _session.incoming) {
        if (frame.id != _id) {
          throw RpcStreamException(
            'frame for correlation id ${frame.id} arrived on the stream for $_id',
          );
        }

        switch (frame.kind) {
          case RpcStreamFrameKind.data:
            if (!frame.hasBody) {
              throw const RpcStreamException(
                'a stream data frame arrived without a body',
              );
            }
            try {
              yield _decoder(frame.body);
            } on Object catch (error) {
              throw RpcStreamException(
                '$_key: stream data failed typed decoding: $error',
              );
            }
          case RpcStreamFrameKind.end:
            _ended = true;
            sawTerminal = true;
            return;
          case RpcStreamFrameKind.error:
            final remote = RpcStreamException(
              frame.message ?? 'remote ${frame.code ?? "unknown"}',
              code: frame.code,
            );
            _error = remote;
            sawTerminal = true;
            throw remote;
          case RpcStreamFrameKind.cancel:
            _cancelled = true;
            sawTerminal = true;
            return;
          case RpcStreamFrameKind.call:
            throw const RpcStreamException(
              'a call frame cannot arrive inside its response stream',
            );
        }
      }
      if (!sawTerminal && !_ended && !_cancelled) {
        throw const RpcStreamException(
          'stream transport ended without an end or cancel frame',
        );
      }
    } on Object catch (error) {
      _error = error;
      rethrow;
    }
  }
}

final class RpcStreamCallBuilder<T> {
  RpcStreamCallBuilder._({
    required OresRpcStreamClient owner,
    required String key,
    required RpcStreamRequest request,
    required T Function(Object?) decoder,
  })  : _owner = owner,
        _key = key,
        _request = RpcStreamRequest(
          method: request.method,
          path: request.path,
          query: List.of(request.query),
          body: _cloneBody(request.body),
        ),
        _decoder = decoder;

  final OresRpcStreamClient _owner;
  final String _key;
  RpcStreamRequest _request;
  final T Function(Object?) _decoder;
  bool _opened = false;

  RpcStreamCallBuilder<T> addQueryField(String name, Object? value) {
    _request = RpcStreamRequest(
      method: _request.method,
      path: _request.path,
      query: [..._request.query, (name, '$value')],
      body: _request.body,
    );
    return this;
  }

  RpcStreamCallBuilder<T> withBody(Object? body) {
    _request = RpcStreamRequest(
      method: _request.method,
      path: _request.path,
      query: _request.query,
      body: _cloneBody(body),
    );
    return this;
  }

  RpcStreamCallBuilder<T> addBodyField(String name, Object? value) {
    final current = _request.body;
    final Map<String, Object?> body;
    if (current == null) {
      body = <String, Object?>{};
    } else if (current is Map) {
      body = Map<String, Object?>.from(current);
    } else {
      throw const RpcStreamException(
        'addBodyField requires an object RPC body',
      );
    }
    body[name] = value;
    return withBody(body);
  }

  /// Sole transport-open / network-I/O boundary.
  Future<RpcStreamClient<T>> stream() async {
    if (_opened) {
      throw StateError('an RPC stream call builder can only be opened once');
    }
    _opened = true;

    final id = _owner._takeId();
    final call = RpcStreamCall(
      id: id,
      key: _key,
      method: _request.method,
      path: _request.path,
      query: List.of(_request.query),
      body: _cloneBody(_request.body),
    );

    late final RpcStreamSession session;
    try {
      session = await _owner._stream.open(call);
    } on Object catch (error) {
      throw RpcStreamException('$_key: opening stream failed: $error');
    }

    return RpcStreamClient<T>._(
      session: session,
      id: id,
      key: _key,
      carrier: _owner._stream.carrier,
      decoder: _decoder,
    );
  }
}

final class OresRpcStreamClient {
  OresRpcStreamClient({
    required FramedRpcStream stream,
    required Iterable<String> operations,
    String idPrefix = 'stream-',
  })  : _stream = stream,
        _operations = Set.of(operations),
        _idPrefix = idPrefix {
    if (stream.carrier != RpcStreamCarrier.websocket &&
        stream.carrier != RpcStreamCarrier.tcp) {
      throw ArgumentError.value(
        stream.carrier,
        'stream.carrier',
        'must be websocket or tcp',
      );
    }
  }

  final FramedRpcStream _stream;
  final Set<String> _operations;
  final String _idPrefix;
  int _sequence = 0;

  RpcStreamCallBuilder<T> prepare<T>(
    String key,
    RpcStreamRequest request,
    T Function(Object?) decoder,
  ) {
    if (!_operations.contains(key)) {
      throw ArgumentError.value(
        key,
        'key',
        'RPC stream operation not generated for this audience',
      );
    }
    if (request.method.isEmpty || request.path.isEmpty) {
      throw ArgumentError(
        'RPC stream request requires non-empty method and path',
      );
    }
    return RpcStreamCallBuilder<T>._(
      owner: this,
      key: key,
      request: request,
      decoder: decoder,
    );
  }

  String _takeId() {
    _sequence += 1;
    return '$_idPrefix$_sequence';
  }
}

Object? _cloneBody(Object? body) {
  if (body is Map) {
    return Map<Object?, Object?>.from(body);
  }
  if (body is List) {
    return List<Object?>.from(body);
  }
  return body;
}
