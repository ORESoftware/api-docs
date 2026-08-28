/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "chptr-api-server";

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
  static const get_chapter = RouteMeta(key: "get_chapter", path: "/v1/chapters/{chapterId}", methods: ["GET"], transports: ["http"]);
  static const transition_chapter = RouteMeta(key: "transition_chapter", path: "/v1/chapters/{chapterId}/transitions", methods: ["POST"], transports: ["http"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "get_chapter": get_chapter,
    "transition_chapter": transition_chapter,
  };
}

