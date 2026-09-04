/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
export type RpcTransport = "http" | "tcp" | "websocket" | "nats";

export const SERVICE = "canonical-api-server" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "list_quotes": {
    key: "list_quotes",
    path: "/api/v1/quotes" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_quote": {
    key: "create_quote",
    path: "/api/v1/quotes" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_quote": {
    key: "get_quote",
    path: "/api/v1/quotes/{quoteId}" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "quoteId": string }) => "/api/v1/quotes/{quoteId}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "retry_quote": {
    key: "retry_quote",
    path: "/api/v1/quotes/{quoteId}/retry" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "quoteId": string }) => "/api/v1/quotes/{quoteId}/retry".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "quote_events": {
    key: "quote_events",
    path: "/api/v1/quotes/{quoteId}/events" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "quoteId": string }) => "/api/v1/quotes/{quoteId}/events".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "list_readiness_frameworks": {
    key: "list_readiness_frameworks",
    path: "/api/v1/readiness/frameworks" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_readiness_framework": {
    key: "get_readiness_framework",
    path: "/api/v1/readiness/frameworks/{frameworkId}" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "frameworkId": string }) => "/api/v1/readiness/frameworks/{frameworkId}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "list_readiness_assessments": {
    key: "list_readiness_assessments",
    path: "/api/v1/readiness/assessments" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_readiness_assessment": {
    key: "create_readiness_assessment",
    path: "/api/v1/readiness/assessments" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "get_readiness_assessment": {
    key: "get_readiness_assessment",
    path: "/api/v1/readiness/assessments/{assessmentId}" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: (p: { "assessmentId": string }) => "/api/v1/readiness/assessments/{assessmentId}".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "sync_changes": {
    key: "sync_changes",
    path: "/api/v1/sync/changes" as const,
    methods: ["GET"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "sync_mutations": {
    key: "sync_mutations",
    path: "/api/v1/sync/mutations" as const,
    methods: ["POST"] as const,
    transports: ["http"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "list_quotes": { path: Record<string, never>; query: { "cursor"?: string; "limit"?: number }; headers: Record<string, never>; body: void; response: { "quotes": Array<unknown>; "nextCursor"?: string } };
  "create_quote": { path: Record<string, never>; query: Record<string, never>; headers: Record<string, never>; body: { "organizationName": string; "contactEmail": string }; response: { "quoteId": string; "status": string } };
  "get_quote": { path: { "quoteId": string }; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "retry_quote": { path: { "quoteId": string }; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "quote_events": { path: { "quoteId": string }; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "list_readiness_frameworks": { path: Record<string, never>; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "get_readiness_framework": { path: { "frameworkId": string }; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "list_readiness_assessments": { path: Record<string, never>; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "create_readiness_assessment": { path: Record<string, never>; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "get_readiness_assessment": { path: { "assessmentId": string }; query: Record<string, never>; headers: Record<string, never>; body: void; response: unknown };
  "sync_changes": { path: Record<string, never>; query: { "cursor"?: string; "limit"?: number }; headers: Record<string, never>; body: void; response: unknown };
  "sync_mutations": { path: Record<string, never>; query: Record<string, never>; headers: Record<string, never>; body: { "operations": Array<unknown> }; response: unknown };
}

/** Adding a map key without a handler is a TypeScript error. */
export type RouteHandlers<Ctx> = {
  [K in RouteName]: (ctx: Ctx, args: {
    path: RouteTypes[K]["path"];
    query: RouteTypes[K]["query"];
    headers: RouteTypes[K]["headers"];
    body: RouteTypes[K]["body"];
  }) => Promise<RouteTypes[K]["response"]> | RouteTypes[K]["response"];
};

export function lookup<K extends RouteName>(key: K): (typeof Routes)[K] {
  return Routes[key];
}
