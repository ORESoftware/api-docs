import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:ores_ridl_runtime/ores_ridl_runtime.dart';
import 'package:test/test.dart';

void main() {
  final fixtureFile = File('../../examples/frames/conformance.json');
  final fixtureDocument = jsonDecode(fixtureFile.readAsStringSync()) as Map<String, Object?>;
  final cases = fixtureDocument['cases']! as List<Object?>;

  test('fixture version matches the runtime', () {
    expect(fixtureDocument['frame_version'], frameVersion);
  });

  for (final value in cases) {
    final fixture = Map<String, Object?>.from(value! as Map);
    final name = fixture['name']! as String;
    final encoded = fixture['encoded']! as String;
    final prefix = fixture['tcp_prefix_hex']! as String;

    test('$name decodes and re-encodes byte-for-byte', () {
      final frame = decodeFrame(encoded);
      expect(utf8.decode(encodeFrame(frame)), encoded);
      expect(_hex(encodeTcp(frame).sublist(0, lengthPrefixBytes)), prefix);
    });
  }

  test('constructors preserve canonical order, Unicode, and null presence', () {
    final call = Frame.call(
      id: '4',
      key: 'create_note',
      method: 'POST',
      path: '/v1/notes',
      body: const PresentBody({'text': 'café — ok'}),
    );
    expect(
      utf8.decode(encodeFrame(call)),
      '{"v":1,"id":"4","t":"call","key":"create_note","method":"POST","path":"/v1/notes","body":{"text":"café — ok"}}',
    );

    final nullBody = decodeFrame('{"v":1,"id":"1","t":"data","body":null}');
    expect(nullBody.hasBody, isTrue);
    expect(nullBody.body, isNull);
    expect(decodeFrame('{"v":1,"id":"1","t":"end"}').hasBody, isFalse);
  });

  test('unknown members, invalid UTF-8, and illegal shapes fail closed', () {
    expect(
      () => decodeFrame('{"v":1,"id":"1","t":"end","deadline":"5s"}'),
      throwsA(isA<FrameException>()),
    );
    expect(
      () => decodeFrame('{"v":257,"id":"1","t":"end"}'),
      throwsA(isA<FrameException>()),
    );
    expect(
      () => decodeFrame('{"v":1,"id":"1","t":"data"}'),
      throwsA(isA<FrameException>()),
    );
    expect(
      () => decodeFrame(Uint8List.fromList([0xff])),
      throwsA(isA<FrameException>()),
    );
  });

  test('stream decoding is bounded and preserves a partial tail', () {
    final first = encodeTcp(Frame.end(id: '1'));
    final second = encodeTcp(Frame.cancel(id: '2'));
    final buffer = Uint8List(first.length + 3)
      ..setRange(0, first.length, first)
      ..setRange(first.length, first.length + 3, second.sublist(0, 3));
    final decoded = decodeStream(buffer);
    expect(decoded.frames, hasLength(1));
    expect(decoded.rest, hasLength(3));

    final huge = Uint8List.fromList([0xff, 0xff, 0xff, 0xff]);
    expect(() => decodeStream(huge), throwsA(isA<FrameException>()));
  });

  test('metadata is sorted and correlation identifiers are monotonic', () {
    final frame = Frame.end(id: '1').withMeta('z', 'last').withMeta('a', 'first');
    expect(
      utf8.decode(encodeFrame(frame)),
      '{"v":1,"id":"1","t":"end","meta":{"a":"first","z":"last"}}',
    );
    final correlator = Correlator('c7-');
    expect([correlator.take(), correlator.take()], ['c7-1', 'c7-2']);
  });
}

String _hex(List<int> bytes) =>
    bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
