/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "gha-indie-worker";

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
  static const readyz = RouteMeta(key: "readyz", path: "/readyz", methods: ["GET"]);
  static const list_builds = RouteMeta(key: "list_builds", path: "/builds", methods: ["GET"]);
  static const submit_build = RouteMeta(key: "submit_build", path: "/builds", methods: ["POST"]);
  static const get_build = RouteMeta(key: "get_build", path: "/builds/{job_id}", methods: ["GET"]);
  static const get_build_logs = RouteMeta(key: "get_build_logs", path: "/builds/{job_id}/logs", methods: ["GET"]);
  static const get_build_artifacts = RouteMeta(key: "get_build_artifacts", path: "/builds/{job_id}/artifacts", methods: ["GET"]);
  static const github_webhook = RouteMeta(key: "github_webhook", path: "/webhooks/github", methods: ["POST"]);
  static const registry_webhook = RouteMeta(key: "registry_webhook", path: "/webhooks/registry", methods: ["POST"]);
  static const sync_secrets = RouteMeta(key: "sync_secrets", path: "/secrets/sync", methods: ["POST"]);
  static const sync_secrets_status = RouteMeta(key: "sync_secrets_status", path: "/secrets/sync/status", methods: ["GET"]);

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

