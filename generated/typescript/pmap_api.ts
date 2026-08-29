/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
export type RpcTransport = "http" | "tcp" | "websocket" | "nats";

export const SERVICE = "pmap-api-server" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_matter": {
    key: "create_matter",
    path: "/v1/matters" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_matter": {
    key: "get_matter",
    path: "/v1/matters/{id}" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "id": string }) => "/v1/matters/{id}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "walk_matter": {
    key: "walk_matter",
    path: "/v1/matters/{id}/walk" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "id": string }) => "/v1/matters/{id}/walk".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "get_documents": {
    key: "get_documents",
    path: "/v1/matters/{id}/documents" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "id": string }) => "/v1/matters/{id}/documents".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "get_facts": {
    key: "get_facts",
    path: "/v1/matters/{id}/facts" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "id": string }) => "/v1/matters/{id}/facts".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "avenues": {
    key: "avenues",
    path: "/v1/avenues" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "geography": {
    key: "geography",
    path: "/v1/geography" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "CheckFieldSanity": {
    key: "CheckFieldSanity",
    path: "/pmap.v1.Interview/CheckFieldSanity" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "AskCounsel": {
    key: "AskCounsel",
    path: "/pmap.v1.Interview/AskCounsel" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "check_field_sanity_rest": {
    key: "check_field_sanity_rest",
    path: "/v1/fields/sanity" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "create_matter": { path: Record<string, never>; query: Record<string, never>; body: void; response: { "id": string } };
  "get_matter": { path: { "id": string }; query: { "include"?: "facts" | "documents" }; body: void; response: { "id": string } };
  "walk_matter": { path: { "id": string }; query: Record<string, never>; body: { "choice_id": string }; response: unknown };
  "get_documents": { path: { "id": string }; query: Record<string, never>; body: void; response: unknown };
  "get_facts": { path: { "id": string }; query: Record<string, never>; body: void; response: unknown };
  "avenues": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "geography": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "CheckFieldSanity": { path: Record<string, never>; query: Record<string, never>; body: { "matter_id"?: string | null; "node_id"?: string | null; "fields": Array<Record<string, unknown>> }; response: { "report": Record<string, unknown> } };
  "AskCounsel": { path: Record<string, never>; query: Record<string, never>; body: { "matter_id": string; "question"?: string; "scope"?: "question" | "options" | "review"; "document"?: string | null }; response: { "round_table": Record<string, unknown>; "providers_configured": Array<string> } };
  "check_field_sanity_rest": { path: Record<string, never>; query: Record<string, never>; body: { "matter_id"?: string | null; "node_id"?: string | null; "fields": Array<Record<string, unknown>> }; response: unknown };
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

