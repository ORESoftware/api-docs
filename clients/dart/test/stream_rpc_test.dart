import 'package:ores_api_docs/ores_api_docs.dart';
import 'package:test/test.dart';

final class _Session implements RpcStreamSession {
  _Session(this.frames);

  final List<RpcStreamFrame> frames;
  int cancels = 0;

  @override
  Stream<RpcStreamFrame> get incoming =>
      Stream<RpcStreamFrame>.fromIterable(frames);

  @override
  Future<void> cancel() async {
    cancels += 1;
  }
}

final class _Wire implements FramedRpcStream {
  _Wire(this.carrier, this.session);

  @override
  final RpcStreamCarrier carrier;
  final _Session session;
  late _Session openedSession;
  int opens = 0;

  @override
  Future<RpcStreamSession> open(RpcStreamCall call) async {
    opens += 1;
    final rebound = session.frames.map((frame) {
      switch (frame.kind) {
        case RpcStreamFrameKind.data:
          return RpcStreamFrame.data(id: call.id, body: frame.body);
        case RpcStreamFrameKind.end:
          return RpcStreamFrame.end(id: call.id);
        case RpcStreamFrameKind.error:
          return RpcStreamFrame.error(
            id: call.id,
            code: frame.code ?? 'unknown',
            message: frame.message,
          );
        case RpcStreamFrameKind.cancel:
          return RpcStreamFrame.cancel(id: call.id);
        case RpcStreamFrameKind.call:
          return RpcStreamFrame.call(id: call.id);
      }
    }).toList();
    openedSession = _Session(rebound);
    return openedSession;
  }
}

void main() {
  test('prepare/configure is inert until stream()', () async {
    final wire = _Wire(
      RpcStreamCarrier.websocket,
      _Session([
        RpcStreamFrame.data(id: '', body: {'value': 1}),
        RpcStreamFrame.data(id: '', body: {'value': 2}),
        RpcStreamFrame.end(id: ''),
      ]),
    );
    final client = OresRpcStreamClient(
      stream: wire,
      operations: const ['demo.events.watch_stream'],
      idPrefix: 's-',
    );
    final builder = client
        .prepare<int>(
          'demo.events.watch_stream',
          const RpcStreamRequest(method: 'GET', path: '/v1/events'),
          (value) => (value! as Map)['value'] as int,
        )
        .addQueryField('room_id', 7)
        .withBody({'active': true});

    expect(wire.opens, 0);
    final stream = await builder.stream();
    expect(wire.opens, 1);
    expect(await stream.values.toList(), [1, 2]);
    expect(stream.context.ended, isTrue);
  });

  test('stream builder can open only once', () async {
    final wire = _Wire(
      RpcStreamCarrier.tcp,
      _Session([RpcStreamFrame.end(id: '')]),
    );
    final builder = OresRpcStreamClient(
      stream: wire,
      operations: const ['demo.watch_stream'],
    ).prepare<int>(
      'demo.watch_stream',
      const RpcStreamRequest(method: 'GET', path: '/v1/watch'),
      (value) => value as int,
    );

    await builder.stream();
    await expectLater(builder.stream(), throwsStateError);
  });

  test('cancellation delegates once', () async {
    final wire = _Wire(RpcStreamCarrier.tcp, _Session(const []));
    final stream = await OresRpcStreamClient(
      stream: wire,
      operations: const ['demo.watch_stream'],
    ).prepare<int>(
      'demo.watch_stream',
      const RpcStreamRequest(method: 'GET', path: '/v1/watch'),
      (value) => value as int,
    ).stream();

    await stream.cancel();
    await stream.cancel();
    expect(wire.openedSession.cancels, 1);
    expect(stream.context.cancelled, isTrue);
  });
}
