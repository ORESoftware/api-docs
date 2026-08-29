/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "gha-indie-worker";

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
  static const readyz = RouteMeta(key: "readyz", path: "/readyz", methods: ["GET"], transports: ["http"]);
  static const list_builds = RouteMeta(key: "list_builds", path: "/builds", methods: ["GET"], transports: ["http"]);
  static const submit_build = RouteMeta(key: "submit_build", path: "/builds", methods: ["POST"], transports: ["http"]);
  static const get_build = RouteMeta(key: "get_build", path: "/builds/{job_id}", methods: ["GET"], transports: ["http"]);
  static const get_build_logs = RouteMeta(key: "get_build_logs", path: "/builds/{job_id}/logs", methods: ["GET"], transports: ["http"]);
  static const get_build_artifacts = RouteMeta(key: "get_build_artifacts", path: "/builds/{job_id}/artifacts", methods: ["GET"], transports: ["http"]);
  static const github_webhook = RouteMeta(key: "github_webhook", path: "/webhooks/github", methods: ["POST"], transports: ["http"]);
  static const registry_webhook = RouteMeta(key: "registry_webhook", path: "/webhooks/registry", methods: ["POST"], transports: ["http"]);
  static const sync_secrets = RouteMeta(key: "sync_secrets", path: "/secrets/sync", methods: ["POST"], transports: ["http"]);
  static const sync_secrets_status = RouteMeta(key: "sync_secrets_status", path: "/secrets/sync/status", methods: ["GET"], transports: ["http"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "readyz": readyz,
    "list_builds": list_builds,
    "submit_build": submit_build,
    "get_build": get_build,
    "get_build_logs": get_build_logs,
    "get_build_artifacts": get_build_artifacts,
    "github_webhook": github_webhook,
    "registry_webhook": registry_webhook,
    "sync_secrets": sync_secrets,
    "sync_secrets_status": sync_secrets_status,
  };
}
