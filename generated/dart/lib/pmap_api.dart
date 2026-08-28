/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "pmap-api-server";

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
  static const create_matter = RouteMeta(key: "create_matter", path: "/v1/matters", methods: ["POST"], transports: ["http"]);
  static const get_matter = RouteMeta(key: "get_matter", path: "/v1/matters/{id}", methods: ["GET"], transports: ["http"]);
  static const walk_matter = RouteMeta(key: "walk_matter", path: "/v1/matters/{id}/walk", methods: ["POST"], transports: ["http"]);
  static const get_documents = RouteMeta(key: "get_documents", path: "/v1/matters/{id}/documents", methods: ["GET"], transports: ["http"]);
  static const get_facts = RouteMeta(key: "get_facts", path: "/v1/matters/{id}/facts", methods: ["GET"], transports: ["http"]);
  static const avenues = RouteMeta(key: "avenues", path: "/v1/avenues", methods: ["GET"], transports: ["http"]);
  static const geography = RouteMeta(key: "geography", path: "/v1/geography", methods: ["GET"], transports: ["http"]);
  static const rpcCheckFieldSanity = RouteMeta(key: "CheckFieldSanity", path: "/pmap.v1.Interview/CheckFieldSanity", methods: ["POST"], transports: ["http"]);
  static const rpcAskCounsel = RouteMeta(key: "AskCounsel", path: "/pmap.v1.Interview/AskCounsel", methods: ["POST"], transports: ["http"]);
  static const check_field_sanity_rest = RouteMeta(key: "check_field_sanity_rest", path: "/v1/fields/sanity", methods: ["POST"], transports: ["http"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "create_matter": create_matter,
    "get_matter": get_matter,
    "walk_matter": walk_matter,
    "get_documents": get_documents,
    "get_facts": get_facts,
    "avenues": avenues,
    "geography": geography,
    "CheckFieldSanity": rpcCheckFieldSanity,
    "AskCounsel": rpcAskCounsel,
    "check_field_sanity_rest": check_field_sanity_rest,
  };
}

