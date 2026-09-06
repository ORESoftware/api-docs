/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "hnpt-api-server";

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
  static const create_observation = RouteMeta(key: "create_observation", path: "/observations", methods: ["POST"], transports: ["http"]);
  static const list_decoys = RouteMeta(key: "list_decoys", path: "/decoys", methods: ["GET"], transports: ["http"]);
  static const create_decoy = RouteMeta(key: "create_decoy", path: "/decoys", methods: ["POST"], transports: ["http"]);
  static const trigger_decoy = RouteMeta(key: "trigger_decoy", path: "/decoys/{decoyId}/triggers", methods: ["POST"], transports: ["http"]);
  static const list_alert_destinations = RouteMeta(key: "list_alert_destinations", path: "/alert-destinations", methods: ["GET"], transports: ["http"]);
  static const create_alert_destination = RouteMeta(key: "create_alert_destination", path: "/alert-destinations", methods: ["POST"], transports: ["http"]);
  static const test_alert_destination = RouteMeta(key: "test_alert_destination", path: "/alert-destinations/{alertDestinationId}/test", methods: ["POST"], transports: ["http"]);
  static const list_discoveries = RouteMeta(key: "list_discoveries", path: "/discoveries", methods: ["GET"], transports: ["http"]);
  static const create_quarantine_case = RouteMeta(key: "create_quarantine_case", path: "/quarantine/cases", methods: ["POST"], transports: ["http"]);
  static const release_quarantine_case = RouteMeta(key: "release_quarantine_case", path: "/quarantine/cases/{caseId}/release", methods: ["POST"], transports: ["http"]);
  static const create_outcome = RouteMeta(key: "create_outcome", path: "/outcomes", methods: ["POST"], transports: ["http"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "create_observation": create_observation,
    "list_decoys": list_decoys,
    "create_decoy": create_decoy,
    "trigger_decoy": trigger_decoy,
    "list_alert_destinations": list_alert_destinations,
    "create_alert_destination": create_alert_destination,
    "test_alert_destination": test_alert_destination,
    "list_discoveries": list_discoveries,
    "create_quarantine_case": create_quarantine_case,
    "release_quarantine_case": release_quarantine_case,
    "create_outcome": create_outcome,
  };
}
