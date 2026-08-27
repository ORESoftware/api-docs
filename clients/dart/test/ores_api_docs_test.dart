import 'package:ores_api_docs/ores_api_docs.dart';
import 'package:test/test.dart';

void main() {
  test('parse map and combination binding', () {
    final map = RouteMap.fromJson({
      'schema_version': '1.0.0',
      'service': 'pmap-api-server',
      'map': {
        'healthz': '/healthz',
        'CheckFieldSanity': {
          'path': '/pmap.v1.Interview/CheckFieldSanity',
          'methods': ['POST'],
          'binding': {
            'annotation': 'Rpc',
            'param_types': ['CheckFieldSanityRequest'],
            'return_type': 'CheckFieldSanityResponse',
            'function_type': 'Unary<CheckFieldSanityRequest, CheckFieldSanityResponse>',
          },
        },
      },
    });
    expect(map.lookup('healthz')?.methods, ['GET']);
    expect(map.lookup('CheckFieldSanity')?.path,
        '/pmap.v1.Interview/CheckFieldSanity');
    expect(map.lookup('CheckFieldSanity')?.binding?['annotation'], 'Rpc');
  });

  test('annotation + function type compile', () {
    const meta = Rpc('AskCounsel', '/pmap.v1.Interview/AskCounsel');
    expect(meta.methods, ['POST']);
    Unary<String, String> echo = (req) async => req;
    expect(echo, isA<Unary<String, String>>());
  });
}
