/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "hhm-api-server";

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
  static const list_reservations = RouteMeta(key: "list_reservations", path: "/api/v1/reservations", methods: ["GET"], transports: ["http"]);
  static const create_reservation = RouteMeta(key: "create_reservation", path: "/api/v1/reservations", methods: ["POST"], transports: ["http"]);
  static const get_reservation = RouteMeta(key: "get_reservation", path: "/api/v1/reservations/{id}", methods: ["GET"], transports: ["http"]);
  static const websocket = RouteMeta(key: "websocket", path: "/ws", methods: ["GET"], transports: ["websocket"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "list_reservations": list_reservations,
    "create_reservation": create_reservation,
    "get_reservation": get_reservation,
    "websocket": websocket,
  };
}

