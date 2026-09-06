/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "canonical-api-server";

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
  static const list_quotes = RouteMeta(key: "list_quotes", path: "/api/v1/quotes", methods: ["GET"], transports: ["http"]);
  static const create_quote = RouteMeta(key: "create_quote", path: "/api/v1/quotes", methods: ["POST"], transports: ["http"]);
  static const get_quote = RouteMeta(key: "get_quote", path: "/api/v1/quotes/{quoteId}", methods: ["GET"], transports: ["http"]);
  static const retry_quote = RouteMeta(key: "retry_quote", path: "/api/v1/quotes/{quoteId}/retry", methods: ["POST"], transports: ["http"]);
  static const quote_events = RouteMeta(key: "quote_events", path: "/api/v1/quotes/{quoteId}/events", methods: ["GET"], transports: ["http"]);
  static const list_readiness_frameworks = RouteMeta(key: "list_readiness_frameworks", path: "/api/v1/readiness/frameworks", methods: ["GET"], transports: ["http"]);
  static const get_readiness_framework = RouteMeta(key: "get_readiness_framework", path: "/api/v1/readiness/frameworks/{frameworkId}", methods: ["GET"], transports: ["http"]);
  static const list_readiness_assessments = RouteMeta(key: "list_readiness_assessments", path: "/api/v1/readiness/assessments", methods: ["GET"], transports: ["http"]);
  static const create_readiness_assessment = RouteMeta(key: "create_readiness_assessment", path: "/api/v1/readiness/assessments", methods: ["POST"], transports: ["http"]);
  static const get_readiness_assessment = RouteMeta(key: "get_readiness_assessment", path: "/api/v1/readiness/assessments/{assessmentId}", methods: ["GET"], transports: ["http"]);
  static const sync_changes = RouteMeta(key: "sync_changes", path: "/api/v1/sync/changes", methods: ["GET"], transports: ["http"]);
  static const sync_mutations = RouteMeta(key: "sync_mutations", path: "/api/v1/sync/mutations", methods: ["POST"], transports: ["http"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "list_quotes": list_quotes,
    "create_quote": create_quote,
    "get_quote": get_quote,
    "retry_quote": retry_quote,
    "quote_events": quote_events,
    "list_readiness_frameworks": list_readiness_frameworks,
    "get_readiness_framework": get_readiness_framework,
    "list_readiness_assessments": list_readiness_assessments,
    "create_readiness_assessment": create_readiness_assessment,
    "get_readiness_assessment": get_readiness_assessment,
    "sync_changes": sync_changes,
    "sync_mutations": sync_mutations,
  };
}
