/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
export type RpcTransport = "http" | "tcp" | "websocket" | "nats";

export const SERVICE = "cliptown-api-server" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "readyz": {
    key: "readyz",
    path: "/readyz" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "list_clips": {
    key: "list_clips",
    path: "/v1/clips" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_clip": {
    key: "create_clip",
    path: "/v1/clips" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "put_clip": {
    key: "put_clip",
    path: "/v1/clips/{clipId}" as const,
    methods: ["PUT"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "clipId": string }) => "/v1/clips/{clipId}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "delete_clip": {
    key: "delete_clip",
    path: "/v1/clips/{clipId}" as const,
    methods: ["DELETE"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "clipId": string }) => "/v1/clips/{clipId}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "sync_push": {
    key: "sync_push",
    path: "/v1/sync/push" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "sync_pull": {
    key: "sync_pull",
    path: "/v1/sync/pull" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "app_vault_sync_push": {
    key: "app_vault_sync_push",
    path: "/v1/app-vault/{appId}/sync/push" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "appId": string }) => "/v1/app-vault/{appId}/sync/push".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "app_vault_sync_pull": {
    key: "app_vault_sync_pull",
    path: "/v1/app-vault/{appId}/sync/pull" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "appId": string }) => "/v1/app-vault/{appId}/sync/pull".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "search": {
    key: "search",
    path: "/v1/search" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "rag_candidates": {
    key: "rag_candidates",
    path: "/v1/rag/candidates" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "register_device": {
    key: "register_device",
    path: "/v1/devices" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "delete_device": {
    key: "delete_device",
    path: "/v1/devices/{deviceId}" as const,
    methods: ["DELETE"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "deviceId": string }) => "/v1/devices/{deviceId}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "get_security_settings": {
    key: "get_security_settings",
    path: "/v1/settings/security" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "put_security_settings": {
    key: "put_security_settings",
    path: "/v1/settings/security" as const,
    methods: ["PUT"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "readyz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "list_clips": { path: Record<string, never>; query: { "cursor"?: string; "limit"?: number }; body: void; response: { "items": Array<unknown>; "next_cursor": string | null } };
  "create_clip": { path: Record<string, never>; query: Record<string, never>; body: { "clip_id": string; "kind": string; "payload": Record<string, unknown>; "pinned"?: boolean; "deleted"?: boolean }; response: unknown };
  "put_clip": { path: { "clipId": string }; query: Record<string, never>; body: void; response: unknown };
  "delete_clip": { path: { "clipId": string }; query: Record<string, never>; body: void; response: unknown };
  "sync_push": { path: Record<string, never>; query: Record<string, never>; body: { "mutations": Array<unknown>; "cursor"?: string | null }; response: unknown };
  "sync_pull": { path: Record<string, never>; query: Record<string, never>; body: { "cursor"?: string | null; "limit"?: number }; response: unknown };
  "app_vault_sync_push": { path: { "appId": string }; query: Record<string, never>; body: { "mutations": Array<unknown>; "base"?: Record<string, unknown> }; response: unknown };
  "app_vault_sync_pull": { path: { "appId": string }; query: Record<string, never>; body: { "after"?: Record<string, unknown>; "limit"?: number }; response: unknown };
  "search": { path: Record<string, never>; query: Record<string, never>; body: { "privacy_mode": "local_only" | "blind_index" | "opt_in_vector"; "blind_terms"?: Array<string>; "limit"?: number; "pinned_only"?: boolean }; response: unknown };
  "rag_candidates": { path: Record<string, never>; query: Record<string, never>; body: { "privacy_mode": "local_only" | "blind_index" | "opt_in_vector" }; response: unknown };
  "register_device": { path: Record<string, never>; query: Record<string, never>; body: { "device_name": string; "platform": "macos" | "windows" | "linux" | "ios" | "android" | "browser" | "cli"; "encryption_public_key": string; "signing_public_key": string }; response: unknown };
  "delete_device": { path: { "deviceId": string }; query: Record<string, never>; body: void; response: unknown };
  "get_security_settings": { path: Record<string, never>; query: Record<string, never>; body: void; response: { "reauth_interval_days": number; "reauth_max_days": number } };
  "put_security_settings": { path: Record<string, never>; query: Record<string, never>; body: { "reauth_interval_days": number }; response: unknown };
}

/** Adding a map key without a handler is a TypeScript error. */
export type RouteHandlers<Ctx> = {
  [K in RouteName]: (ctx: Ctx, args: {
    path: RouteTypes[K]["path"];
    query: RouteTypes[K]["query"];
    body: RouteTypes[K]["body"];
  }) => Promise<RouteTypes[K]["response"]> | RouteTypes[K]["response"];
};

export function lookup<K extends RouteName>(key: K): (typeof Routes)[K] {
  return Routes[key];
}
