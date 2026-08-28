/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";

export const SERVICE = "chptr-api-server" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_chapter": {
    key: "get_chapter",
    path: "/v1/chapters/{chapterId}" as const,
    methods: ["GET"] as const,
    buildPath: (p: { "chapterId": string }) => "/v1/chapters/{chapterId}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "transition_chapter": {
    key: "transition_chapter",
    path: "/v1/chapters/{chapterId}/transitions" as const,
    methods: ["POST"] as const,
    buildPath: (p: { "chapterId": string }) => "/v1/chapters/{chapterId}/transitions".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "get_chapter": { path: { "chapterId": string }; query: Record<string, never>; body: void; response: { "id": string } };
  "transition_chapter": { path: { "chapterId": string }; query: Record<string, never>; body: { "to": string; "revision"?: string }; response: unknown };
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

