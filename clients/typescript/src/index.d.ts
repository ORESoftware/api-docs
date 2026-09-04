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
  transport?: "http" | "tcp" | "websocket" | "nats";
  path?: object;
  query?: object;
  headers?: object;
  body?: unknown;
  traceId?: string;
  spanId?: string;
}): {
  v: 1;
  op: "call";
  id: string;
  key: string;
  transport?: "http" | "tcp" | "websocket" | "nats";
  path?: object;
  query?: object;
  headers?: object;
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
  transport?: "http" | "tcp" | "websocket" | "nats";
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
  transport?: "http" | "tcp" | "websocket" | "nats";
  traceId?: string;
  spanId?: string;
};
export function callToNdjson(frame: object): string;
export function encodeLengthPrefixed(frame: object): Uint8Array;
export function splitLengthPrefixed(buf: Uint8Array): { frames: Uint8Array[]; rest: Uint8Array };
export const MAX_FRAME_BYTES: number;
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
  header_schema?: object;
  error_schema?: object;
  alias_of?: string;
  transports?: Array<"http" | "tcp" | "websocket" | "nats">;
  tcp_framing?: "ndjson" | "length-prefixed";
  delivery?: "direct" | "opto_sync_queued";
  opto_sync?: { table: string; operation: "upsert" | "delete" };
}

export {
  Correlator as RpcV1Correlator,
  LENGTH_PREFIX_BYTES as RPC_V1_LENGTH_PREFIX_BYTES,
  MAX_FRAME_BYTES as RPC_V1_MAX_FRAME_BYTES,
  RPC_VERSION as RPC_V1_VERSION,
  RpcV1Error,
  assertReceiptForCall as assertRpcV1ReceiptForCall,
  callFromNdjson as rpcV1CallFromNdjson,
  callToNdjson as rpcV1CallToNdjson,
  decodeCall as decodeRpcV1Call,
  decodeReceipt as decodeRpcV1Receipt,
  encodeCall as encodeRpcV1Call,
  encodeLengthPrefixed as encodeRpcV1LengthPrefixed,
  encodeReceipt as encodeRpcV1Receipt,
  receiptFromNdjson as rpcV1ReceiptFromNdjson,
  receiptToNdjson as rpcV1ReceiptToNdjson,
  splitLengthPrefixed as splitRpcV1LengthPrefixed,
  toNdjson as rpcV1ToNdjson,
  validateCall as validateRpcV1Call,
  validateReceipt as validateRpcV1Receipt,
} from "./rpc.js";

export type {
  RpcV1Call,
  RpcV1Envelope,
  RpcV1ErrorReceipt,
  RpcV1Receipt,
  RpcV1SuccessReceipt,
  RpcV1Transport,
} from "./rpc.js";
