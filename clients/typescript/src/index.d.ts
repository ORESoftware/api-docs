/** Named function type: param type is the request, return type is the response. */
export type UnaryFn<Req, Res> = (req: Req) => Promise<Res>;

/** Route identity as types (path is a string-literal type). */
export type RouteKey<
  K extends string,
  P extends `/${string}`,
  M extends "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS",
> = {
  readonly key: K;
  readonly path: P;
  readonly methods: readonly M[];
};

export const SCHEMA_VERSION: "1.0.0";

export function route<K extends string, P extends `/${string}`, M extends string>(
  key: K,
  path: P,
  methods: readonly M[],
): { readonly key: K; readonly path: P; readonly methods: readonly M[] };

export function compileValidator(schemaName?: string): (data: unknown) => boolean;
export function parseRouteMap(json: string | unknown): RouteMapJson;
export function inferMethods(key: string): string[];
export function inferTransports(key: string, path: string): string[];
export function encodeCall(input: {
  id: string;
  key: string;
  transport?: "http" | "tcp" | "websocket";
  path?: object;
  query?: object;
  body?: unknown;
  traceId?: string;
  spanId?: string;
}): {
  v: 1;
  op: "call";
  id: string;
  key: string;
  transport?: "http" | "tcp" | "websocket";
  path?: object;
  query?: object;
  body?: unknown;
  traceId?: string;
  spanId?: string;
};
export function encodeReceipt(input: {
  id: string;
  key: string;
  ok: boolean;
  status?: number;
  body?: unknown;
  error?: object;
  transport?: "http" | "tcp" | "websocket";
  traceId?: string;
  spanId?: string;
}): {
  v: 1;
  op: "receipt";
  id: string;
  key: string;
  ok: boolean;
  status?: number;
  body?: unknown;
  error?: object;
  transport?: "http" | "tcp" | "websocket";
  traceId?: string;
  spanId?: string;
};
export function callToNdjson(frame: object): string;
export function pathTemplateVars(path: string): string[];
export function expandPath(template: string, params: Record<string, string>): string;
export function lookup(map: RouteMapJson, key: string): unknown;
export function envelopeRouteMap(
  map: RouteMapJson,
  updatedAt: string,
): {
  id: string;
  scope: "ores.api-docs.route-map";
  kind: "ores.api-docs.route-map";
  record_id: string;
  updatedAt: string;
  payload: RouteMapJson;
};

export const OPTO_SYNC_SCOPE: "ores.api-docs.route-map";

export interface RouteMapJson {
  schema_version: "1.0.0";
  service: string;
  map: Record<string, string | RouteValue>;
}

export interface RouteValue {
  path: `/${string}` | string;
  methods?: string[];
  summary?: string;
  binding?: {
    annotation?: string;
    param_types?: string[];
    return_type?: string;
    function_type?: string;
    file?: string;
    symbol?: string;
  };
  request_schema?: object;
  response_schema?: object;
  path_params?: object;
  query_schema?: object;
  error_schema?: object;
  alias_of?: string;
  transports?: Array<"http" | "tcp" | "websocket">;
  tcp_framing?: "ndjson" | "length-prefixed";
}
