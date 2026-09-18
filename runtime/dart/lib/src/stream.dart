import 'frame.dart';

abstract interface class FramedStreamSession {
  Stream<Frame> get incoming;
  Future<void> cancel();
}

abstract interface class FramedStream {
  String get carrier;
  Future<FramedStreamSession> open(Frame call);
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
  final String carrier;
  final bool ended;
  final bool cancelled;
  final Object? error;
}

final class StreamRequest {
  const StreamRequest({
    required this.key,
    required this.method,
    required this.path,
    this.query = const [],
    this.body,
  });

  final String key;
  final String method;
  final String path;
  final List<QueryPair> query;
  final PresentBody? body;
}

final class RpcStreamClient<T> {
  RpcStreamClient._({
    required FramedStreamSession session,
    required String id,
    required String key,
    required String carrier,
    required T Function(Object?) decoder,
  })  : _session = session,
        _id = id,
        _key = key,
        _carrier = carrier,
        _decoder = decoder;

  final FramedStreamSession _session;
  final String _id;
  final String _key;
  final String _carrier;
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
      final wrapped = RpcStreamException('$_key: stream cancellation failed: $error');
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
          case FrameKind.data:
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
          case FrameKind.end:
            _ended = true;
            sawTerminal = true;
            return;
          case FrameKind.error:
            final remote = RpcStreamException(
              frame.message ?? 'remote ${frame.code ?? "unknown"}',
              code: frame.code,
            );
            _error = remote;
            sawTerminal = true;
            throw remote;
          case FrameKind.cancel:
            _cancelled = true;
            sawTerminal = true;
            return;
          case FrameKind.call:
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
    required FramedStream stream,
    required Correlator correlator,
    required StreamRequest request,
    required T Function(Object?) decoder,
  })  : _stream = stream,
        _correlator = correlator,
        _request = request,
        _decoder = decoder;

  final FramedStream _stream;
  final Correlator _correlator;
  final StreamRequest _request;
  final T Function(Object?) _decoder;
  bool _opened = false;

  /// Sole stream I/O boundary. Constructing/configuring this builder performs no I/O.
  Future<RpcStreamClient<T>> stream() async {
    if (_opened) {
      throw StateError('an RPC stream call builder can only be opened once');
    }
    _opened = true;
    if (_stream.carrier != 'websocket' && _stream.carrier != 'tcp') {
      throw RpcStreamException(
        'stream carrier must be websocket or tcp, got ${_stream.carrier}',
      );
    }

    final id = _correlator.take();
    final call = Frame.call(
      id: id,
      key: _request.key,
      method: _request.method,
      path: _request.path,
      query: _request.query,
      body: _request.body,
    );
    late final FramedStreamSession session;
    try {
      session = await _stream.open(call);
    } on Object catch (error) {
      throw RpcStreamException('${_request.key}: opening stream failed: $error');
    }
    return RpcStreamClient<T>._(
      session: session,
      id: id,
      key: _request.key,
      carrier: _stream.carrier,
      decoder: _decoder,
    );
  }
}

final class FramedStreamTransport {
  FramedStreamTransport(this._stream, [String idPrefix = ''])
      : _correlator = Correlator(idPrefix);

  final FramedStream _stream;
  final Correlator _correlator;

  RpcStreamCallBuilder<T> prepare<T>(
    StreamRequest request,
    T Function(Object?) decoder,
  ) =>
      RpcStreamCallBuilder<T>._(
        stream: _stream,
        correlator: _correlator,
        request: request,
        decoder: decoder,
      );
}
