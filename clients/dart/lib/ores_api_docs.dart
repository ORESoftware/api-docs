/// Route map client. A key may be a Dart annotation, a typed parameter,
/// a return type, a [Unary] typedef, or any combination.

library ores_api_docs;

class Rpc {
  final String key;
  final String path;
  final List<String> methods;
  const Rpc(this.key, this.path, {this.methods = const ['POST']});
}

/// Function type: param type is the request, return type is the response.
typedef Unary<Req, Res> = Future<Res> Function(Req req);

class RouteEntry {
  RouteEntry({
    required this.path,
    required this.methods,
    this.summary,
    this.binding,
    this.querySchema,
    this.pathParams,
  });

  final String path;
  final List<String> methods;
  final String? summary;
  final Map<String, Object?>? binding;
  final Map<String, Object?>? querySchema;
  final Map<String, Object?>? pathParams;
}

class RouteMap {
  RouteMap({
    required this.schemaVersion,
    required this.service,
    required this.map,
  });

  final String schemaVersion;
  final String service;
  final Map<String, RouteEntry> map;

  RouteEntry? lookup(String key) => map[key];

  factory RouteMap.fromJson(Map<String, dynamic> json) {
    if (json['schema_version'] != '1.0.0') {
      throw FormatException('unsupported schema_version');
    }
    final service = json['service'];
    final rawMap = json['map'];
    if (service is! String || rawMap is! Map) {
      throw FormatException('service and map are required');
    }
    final map = <String, RouteEntry>{};
    rawMap.forEach((key, value) {
      if (key is String) {
        map[key] = _entry(key, value);
      }
    });
    if (map.isEmpty) {
      throw FormatException('map must not be empty');
    }
    return RouteMap(schemaVersion: '1.0.0', service: service, map: map);
  }
}

RouteEntry _entry(String key, Object? value) {
  if (value is String) {
    if (!value.startsWith('/')) {
      throw FormatException('$key path must start with /');
    }
    return RouteEntry(path: value, methods: _infer(key));
  }
  if (value is Map) {
    final obj = Map<String, dynamic>.from(value);
    final path = obj['path'];
    if (path is! String || !path.startsWith('/')) {
      throw FormatException('$key missing path');
    }
    final methods = obj['methods'];
    final list = methods is List
        ? methods.whereType<String>().toList()
        : _infer(key);
    final binding = obj['binding'];
    return     RouteEntry(
      path: path,
      methods: list.isEmpty ? _infer(key) : list,
      summary: obj['summary'] as String?,
      binding: binding is Map ? Map<String, Object?>.from(binding) : null,
      querySchema: obj['query_schema'] is Map
          ? Map<String, Object?>.from(obj['query_schema'] as Map)
          : null,
      pathParams: obj['path_params'] is Map
          ? Map<String, Object?>.from(obj['path_params'] as Map)
          : null,
    );
  }
  throw FormatException('$key: expected path or object');
}

List<String> _infer(String key) {
  if (key.isNotEmpty && key[0] == key[0].toUpperCase() && key[0] != key[0].toLowerCase()) {
    return const ['POST'];
  }
  final lower = key.toLowerCase();
  if (lower.startsWith('delete')) {
    return const ['DELETE'];
  }
  if (lower.startsWith('put') || lower.startsWith('update') || lower.startsWith('replace')) {
    return const ['PUT'];
  }
  if (lower.startsWith('patch')) {
    return const ['PATCH'];
  }
  if (lower.contains('create') ||
      lower.contains('walk') ||
      lower.contains('check') ||
      lower.contains('ask') ||
      lower.startsWith('post') ||
      lower.startsWith('submit')) {
    return const ['POST'];
  }
  return const ['GET'];
}
