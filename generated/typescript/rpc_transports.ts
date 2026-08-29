/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
export type RpcTransport = "http" | "tcp" | "websocket" | "nats";

export const SERVICE = "example-rpc" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_item": {
    key: "get_item",
    path: "/v1/items/{id}" as const,
    methods: ["GET"] as const,
    transports: ["http", "tcp", "websocket"] as const,
    buildPath: (p: { "id": string }) => "/v1/items/{id}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "websocket": {
    key: "websocket",
    path: "/ws" as const,
    methods: ["GET"] as const,
    transports: ["websocket"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "tcp_ping": {
    key: "tcp_ping",
    path: "/rpc/ping" as const,
    methods: ["POST"] as const,
    transports: ["tcp"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "nats_ping": {
    key: "nats_ping",
    path: "/rpc/nats-ping" as const,
    methods: ["POST"] as const,
    transports: ["nats"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "get_item": { path: { "id": string }; query: Record<string, never>; body: void; response: unknown };
  "websocket": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "tcp_ping": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "nats_ping": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
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

