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

  test('rpc-transports example: get_item on http, tcp, websocket', () {
    final map = RouteMap.fromJson({
      'schema_version': '1.0.0',
      'service': 'example-rpc',
      'map': {
        'healthz': '/healthz',
        'get_item': {
          'path': '/v1/items/{id}',
          'methods': ['GET'],
          'transports': ['http', 'tcp', 'websocket'],
          'path_params': {
            'type': 'object',
            'required': ['id'],
            'properties': {
              'id': {'type': 'string', 'minLength': 1},
            },
          },
        },
        'websocket': {
          'path': '/ws',
          'methods': ['GET'],
          'transports': ['websocket'],
        },
        'tcp_ping': {
          'path': '/rpc/ping',
          'methods': ['POST'],
          'transports': ['tcp'],
        },
      },
    });
    expect(map.lookup('get_item')?.transports, ['http', 'tcp', 'websocket']);
    expect(map.lookup('tcp_ping')?.transports, ['tcp']);
    expect(map.lookup('websocket')?.transports, ['websocket']);
    expect(map.lookup('healthz')?.transports, ['http']);
    expect(map.lookup('get_item')?.path, '/v1/items/{id}');
  });
}
