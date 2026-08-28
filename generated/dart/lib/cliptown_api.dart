/// Generated from a route-map JSON. Do not edit by hand.
library;

const String kService = "cliptown-api-server";

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
  static const list_clips = RouteMeta(key: "list_clips", path: "/v1/clips", methods: ["GET"], transports: ["http"]);
  static const create_clip = RouteMeta(key: "create_clip", path: "/v1/clips", methods: ["POST"], transports: ["http"]);
  static const put_clip = RouteMeta(key: "put_clip", path: "/v1/clips/{clipId}", methods: ["PUT"], transports: ["http"]);
  static const delete_clip = RouteMeta(key: "delete_clip", path: "/v1/clips/{clipId}", methods: ["DELETE"], transports: ["http"]);
  static const sync_push = RouteMeta(key: "sync_push", path: "/v1/sync/push", methods: ["POST"], transports: ["http"]);
  static const sync_pull = RouteMeta(key: "sync_pull", path: "/v1/sync/pull", methods: ["POST"], transports: ["http"]);
  static const app_vault_sync_push = RouteMeta(key: "app_vault_sync_push", path: "/v1/app-vault/{appId}/sync/push", methods: ["POST"], transports: ["http"]);
  static const app_vault_sync_pull = RouteMeta(key: "app_vault_sync_pull", path: "/v1/app-vault/{appId}/sync/pull", methods: ["POST"], transports: ["http"]);
  static const search = RouteMeta(key: "search", path: "/v1/search", methods: ["POST"], transports: ["http"]);
  static const rag_candidates = RouteMeta(key: "rag_candidates", path: "/v1/rag/candidates", methods: ["POST"], transports: ["http"]);
  static const register_device = RouteMeta(key: "register_device", path: "/v1/devices", methods: ["POST"], transports: ["http"]);
  static const delete_device = RouteMeta(key: "delete_device", path: "/v1/devices/{deviceId}", methods: ["DELETE"], transports: ["http"]);
  static const get_security_settings = RouteMeta(key: "get_security_settings", path: "/v1/settings/security", methods: ["GET"], transports: ["http"]);
  static const put_security_settings = RouteMeta(key: "put_security_settings", path: "/v1/settings/security", methods: ["PUT"], transports: ["http"]);

  static const Map<String, RouteMeta> byKey = {
    "healthz": healthz,
    "readyz": readyz,
    "list_clips": list_clips,
    "create_clip": create_clip,
    "put_clip": put_clip,
    "delete_clip": delete_clip,
    "sync_push": sync_push,
    "sync_pull": sync_pull,
    "app_vault_sync_push": app_vault_sync_push,
    "app_vault_sync_pull": app_vault_sync_pull,
    "search": search,
    "rag_candidates": rag_candidates,
    "register_device": register_device,
    "delete_device": delete_device,
    "get_security_settings": get_security_settings,
    "put_security_settings": put_security_settings,
  };
}

