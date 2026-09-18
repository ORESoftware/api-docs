import 'package:ores_ridl_runtime/ores_ridl_runtime.dart';
import 'package:test/test.dart';

final class _Session implements FramedStreamSession {
  _Session(this.frames);

  final List<Frame> frames;
  int cancels = 0;

  @override
  Stream<Frame> get incoming => Stream<Frame>.fromIterable(frames);

  @override
  Future<void> cancel() async {
    cancels += 1;
  }
}

final class _Stream implements FramedStream {
  _Stream(this.carrier, this.session);

  @override
  final String carrier;
  final _Session session;
  int opens = 0;

  @override
  Future<FramedStreamSession> open(Frame call) async {
    opens += 1;
    return session;
  }
}

void main() {
  test('stream builder performs no I/O until stream()', () async {
    final session = _Session([
      Frame.data(id: 's-1', body: 1),
      Frame.data(id: 's-1', body: 2),
      Frame.end(id: 's-1'),
    ]);
    final wire = _Stream('websocket', session);
    final builder = FramedStreamTransport(wire, 's-').prepare<int>(
      const StreamRequest(
        key: 'demo.events.watch_stream',
        method: 'GET',
        path: '/v1/events',
      ),
      (value) => value! as int,
    );

    expect(wire.opens, 0);
    final client = await builder.stream();
    expect(wire.opens, 1);
    expect(await client.values.toList(), [1, 2]);
    expect(client.context.ended, isTrue);
    expect(client.context.cancelled, isFalse);
  });

  test('stream builder cannot open twice', () async {
    final wire = _Stream('tcp', _Session([Frame.end(id: 'once-1')]));
    final builder = FramedStreamTransport(wire, 'once-').prepare<int>(
      const StreamRequest(
        key: 'demo.watch_stream',
        method: 'GET',
        path: '/v1/watch',
      ),
      (value) => value! as int,
    );
    await builder.stream();
    await expectLater(builder.stream(), throwsStateError);
  });

  test('stream client rejects correlation mismatch', () async {
    final wire = _Stream(
      'websocket',
      _Session([Frame.data(id: 'wrong', body: 1)]),
    );
    final client = await FramedStreamTransport(wire, 'expected-').prepare<int>(
      const StreamRequest(
        key: 'demo.watch_stream',
        method: 'GET',
        path: '/v1/watch',
      ),
      (value) => value! as int,
    ).stream();

    await expectLater(client.values.toList(), throwsA(isA<RpcStreamException>()));
    expect(client.context.error, isA<RpcStreamException>());
  });

  test('stream cancellation delegates exactly once', () async {
    final session = _Session(const []);
    final wire = _Stream('tcp', session);
    final client = await FramedStreamTransport(wire, 'cancel-').prepare<int>(
      const StreamRequest(
        key: 'demo.watch_stream',
        method: 'GET',
        path: '/v1/watch',
      ),
      (value) => value! as int,
    ).stream();

    await client.cancel();
    await client.cancel();
    expect(session.cancels, 1);
    expect(client.context.cancelled, isTrue);
  });
}
