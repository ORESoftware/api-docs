/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "example-rpc";

class RouteMeta {
  const RouteMeta({required this.key, required this.path, required this.methods, this.transports = const ['http']});
  final String key;
  final String path;
  final List<String> methods;
  final List<String> transports;
  String expand(Map<String, String> params) {
    var out = path;
    params.forEach((k, v) {
      out = out.replaceAll('{$k}', Uri.encodeComponent(v));
    });
    return out;
  }
}

abstract final class Routes {
  static const healthz = RouteMeta(key: "healthz", path: "/healthz", methods: ["GET"], transports: ["http"]);
  static const get_item = RouteMeta(key: "get_item", path: "/v1/items/{id}", methods: ["GET"], transports: ["http", "tcp", "websocket"]);
  static const websocket = RouteMeta(key: "websocket", path: "/ws", methods: ["GET"], transports: ["websocket"]);
  static const tcp_ping = RouteMeta(key: "tcp_ping", path: "/rpc/ping", methods: ["POST"], transports: ["tcp"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "get_item": get_item,
    "websocket": websocket,
    "tcp_ping": tcp_ping,
  };
}

