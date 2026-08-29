/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
export type RpcTransport = "http" | "tcp" | "websocket" | "nats";

export const SERVICE = "hhm-api-server" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "list_reservations": {
    key: "list_reservations",
    path: "/api/v1/reservations" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_reservation": {
    key: "create_reservation",
    path: "/api/v1/reservations" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_reservation": {
    key: "get_reservation",
    path: "/api/v1/reservations/{id}" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "id": string }) => "/api/v1/reservations/{id}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "websocket": {
    key: "websocket",
    path: "/ws" as const,
    methods: ["GET"] as const,
    transports: ["websocket"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "list_reservations": { path: Record<string, never>; query: Record<string, never>; body: void; response: Array<Record<string, unknown>> };
  "create_reservation": { path: Record<string, never>; query: Record<string, never>; body: { "title": string; "summary"?: string }; response: { "id": string; "title": string; "status": "requested" | "confirmed" | "checked_in" | "completed" | "cancelled" } };
  "get_reservation": { path: { "id": string }; query: Record<string, never>; body: void; response: unknown };
  "websocket": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
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
