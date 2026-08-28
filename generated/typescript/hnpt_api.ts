/** Generated from a route-map JSON. Do not edit by hand. */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";

export const SERVICE = "hnpt-api-server" as const;

export const Routes = {
  "healthz": {
    key: "healthz",
    path: "/healthz" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_observation": {
    key: "create_observation",
    path: "/observations" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "list_decoys": {
    key: "list_decoys",
    path: "/decoys" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_decoy": {
    key: "create_decoy",
    path: "/decoys" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "trigger_decoy": {
    key: "trigger_decoy",
    path: "/decoys/{decoyId}/triggers" as const,
    methods: ["POST"] as const,
    buildPath: (p: { "decoyId": string }) => "/decoys/{decoyId}/triggers".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "list_alert_destinations": {
    key: "list_alert_destinations",
    path: "/alert-destinations" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_alert_destination": {
    key: "create_alert_destination",
    path: "/alert-destinations" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "test_alert_destination": {
    key: "test_alert_destination",
    path: "/alert-destinations/{alertDestinationId}/test" as const,
    methods: ["POST"] as const,
    buildPath: (p: { "alertDestinationId": string }) => "/alert-destinations/{alertDestinationId}/test".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "list_discoveries": {
    key: "list_discoveries",
    path: "/discoveries" as const,
    methods: ["GET"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "create_quarantine_case": {
    key: "create_quarantine_case",
    path: "/quarantine/cases" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
  "release_quarantine_case": {
    key: "release_quarantine_case",
    path: "/quarantine/cases/{caseId}/release" as const,
    methods: ["POST"] as const,
    buildPath: (p: { "caseId": string }) => "/quarantine/cases/{caseId}/release".replace(/\{([^}]+)\}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n]))),
  },
  "create_outcome": {
    key: "create_outcome",
    path: "/outcomes" as const,
    methods: ["POST"] as const,
    buildPath: undefined as ((p: Record<string, never>) => string) | undefined,
  },
} as const;

export type RouteName = keyof typeof Routes;

export interface RouteTypes {
  "healthz": { path: Record<string, never>; query: Record<string, never>; body: void; response: unknown };
  "create_observation": { path: Record<string, never>; query: Record<string, never>; body: { "decoyId": string }; response: { "id": string; "disposition": string } };
  "list_decoys": { path: Record<string, never>; query: { "cursor"?: string; "status"?: "draft" | "active" | "paused" | "retired" }; body: void; response: unknown };
  "create_decoy": { path: Record<string, never>; query: Record<string, never>; body: { "tenantId": string; "assetId": string; "decoyKey": string; "kind": "endpoint" | "credential" | "document" | "resource" | "admin_surface" | "playpen"; "profile": string; "syntheticNamespace": string }; response: unknown };
  "trigger_decoy": { path: { "decoyId": string }; query: Record<string, never>; body: { "tenantId": string; "sensorId": string; "eventId": string; "eventTime": string; "protocol": string; "sourceHash": string; "attributes": Record<string, unknown> }; response: unknown };
  "list_alert_destinations": { path: Record<string, never>; query: { "cursor"?: string }; body: void; response: unknown };
  "create_alert_destination": { path: Record<string, never>; query: Record<string, never>; body: { "tenantId": string; "destinationKey": string; "kind": "webhook" | "syslog" | "email" | "slack" | "pagerduty" | "siem"; "displayName": string; "minimumSeverity": "info" | "low" | "medium" | "high" | "critical"; "endpointSecretRef": string }; response: unknown };
  "test_alert_destination": { path: { "alertDestinationId": string }; query: Record<string, never>; body: { "mode": string }; response: unknown };
  "list_discoveries": { path: Record<string, never>; query: { "cursor"?: string; "state"?: "open" | "investigating" | "confirmed" | "dismissed" | "closed" }; body: void; response: unknown };
  "create_quarantine_case": { path: Record<string, never>; query: Record<string, never>; body: { "id": string }; response: unknown };
  "release_quarantine_case": { path: { "caseId": string }; query: Record<string, never>; body: { "reasonCode": string; "notes"?: string }; response: unknown };
  "create_outcome": { path: Record<string, never>; query: Record<string, never>; body: { "id": string }; response: unknown };
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

