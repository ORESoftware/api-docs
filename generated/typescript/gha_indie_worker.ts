/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";

export const SERVICE = "gha-indie-worker" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "readyz": {
    key: "readyz",
    path: "/readyz" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "list_builds": {
    key: "list_builds",
    path: "/builds" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "submit_build": {
    key: "submit_build",
    path: "/builds" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_build": {
    key: "get_build",
    path: "/builds/{job_id}" as const,
    methods: ["GET"] as const,
    buildPath: (p: { "job_id": string }) => "/builds/{job_id}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "get_build_logs": {
    key: "get_build_logs",
    path: "/builds/{job_id}/logs" as const,
    methods: ["GET"] as const,
    buildPath: (p: { "job_id": string }) => "/builds/{job_id}/logs".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "get_build_artifacts": {
    key: "get_build_artifacts",
    path: "/builds/{job_id}/artifacts" as const,
    methods: ["GET"] as const,
    buildPath: (p: { "job_id": string }) => "/builds/{job_id}/artifacts".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "github_webhook": {
    key: "github_webhook",
    path: "/webhooks/github" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "registry_webhook": {
    key: "registry_webhook",
    path: "/webhooks/registry" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "sync_secrets": {
    key: "sync_secrets",
    path: "/secrets/sync" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "sync_secrets_status": {
    key: "sync_secrets_status",
    path: "/secrets/sync/status" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "readyz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "list_builds": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "submit_build": { path: Record<string, never>; query: Record<string, never>; body: { "schemaVersion"?: string; "jobKind"?: string; "repoUrl": string; "gitRef"?: string; "image"?: string; "profile"?: string; "contextDir"?: string; "dockerfile"?: string; "push"?: boolean; "executor"?: string; "requestId"?: string }; response: unknown };
  "get_build": { path: { "job_id": string }; query: Record<string, never>; body: void; response: unknown };
  "get_build_logs": { path: { "job_id": string }; query: Record<string, never>; body: void; response: unknown };
  "get_build_artifacts": { path: { "job_id": string }; query: Record<string, never>; body: void; response: unknown };
  "github_webhook": { path: Record<string, never>; query: Record<string, never>; body: Record<string, unknown>; response: unknown };
  "registry_webhook": { path: Record<string, never>; query: Record<string, never>; body: Record<string, unknown>; response: unknown };
  "sync_secrets": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "sync_secrets_status": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
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

