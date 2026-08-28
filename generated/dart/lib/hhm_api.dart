/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "hhm-api-server";

class RouteMeta {
  const RouteMeta({required this.key, required this.path, required this.methods});
  final String key;
  final String path;
  final List<String> methods;
  String expand(Map<String, String> params) {
    var out = path;
    params.forEach((k, v) {
      out = out.replaceAll('{$k}', Uri.encodeComponent(v));
    });
    return out;
  }
}

abstract final class Routes {
  static const healthz = RouteMeta(key: "healthz", path: "/healthz", methods: ["GET"]);
  static const list_reservations = RouteMeta(key: "list_reservations", path: "/api/v1/reservations", methods: ["GET"]);
  static const create_reservation = RouteMeta(key: "create_reservation", path: "/api/v1/reservations", methods: ["POST"]);
  static const get_reservation = RouteMeta(key: "get_reservation", path: "/api/v1/reservations/{id}", methods: ["GET"]);
  static const websocket = RouteMeta(key: "websocket", path: "/ws", methods: ["GET"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "list_reservations": list_reservations,
    "create_reservation": create_reservation,
    "get_reservation": get_reservation,
    "websocket": websocket,
  };
}

