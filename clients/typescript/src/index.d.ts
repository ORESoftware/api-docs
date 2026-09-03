export const SCHEMA_VERSION: "1.0.0";
export const OPTO_SYNC_SCOPE: "ores.api-docs.route-map";

export type HttpMethod =
  | "GET"
  | "POST"
  | "PUT"
  | "PATCH"
  | "DELETE"
  | "HEAD"
  | "OPTIONS"
  | "CONNECT"
  | "TRACE";
export type DeliveryMode = "direct" | "opto-sync";

export interface JsonSchemaObject {
  readonly $schema?: string;
  readonly $id?: string;
  readonly type?: string | readonly string[];
  readonly properties?: Readonly<Record<string, JsonSchemaObject | boolean>>;
  readonly required?: readonly string[];
  readonly additionalProperties?: JsonSchemaObject | boolean;
  readonly [keyword: string]: unknown;
}

export interface OptoSyncPolicy {
  readonly scope: string;
  readonly queue: string;
  readonly conflict_policy: string;
  readonly [key: string]: unknown;
}

export interface RouteOperation {
  readonly path: string;
  readonly methods: readonly HttpMethod[];
  readonly transports?: readonly string[];
  readonly request_schema?: JsonSchemaObject;
  readonly response_schema?: JsonSchemaObject;
  readonly error_schema?: JsonSchemaObject;
  readonly path_schema?: JsonSchemaObject;
  readonly query_schema?: JsonSchemaObject;
  readonly delivery?: DeliveryMode;
  readonly opto_sync?: OptoSyncPolicy;
  readonly [key: string]: unknown;
}

export interface RouteMapDocument {
  readonly version: string;
  readonly map: Readonly<Record<string, RouteOperation>>;
  readonly aliases?: Readonly<Record<string, string>>;
}

export interface RouteMetadata {
  readonly key: string;
  readonly path: string;
  readonly methods: readonly string[];
}

export interface CallOptions {
  readonly method?: HttpMethod;
  readonly path?: Readonly<Record<string, string | number | boolean>>;
  readonly query?: Readonly<
    Record<
      string,
      | string
      | number
      | boolean
      | null
      | undefined
      | readonly (string | number | boolean)[]
    >
  >;
  readonly headers?: Readonly<Record<string, string>>;
  readonly body?: unknown;
}

export interface ApiResponse<T = unknown> {
  readonly status: number;
  readonly body: T;
}

export interface ApiDocsClientOptions {
  readonly baseUrl: string;
  readonly fetchImpl?: typeof fetch;
  readonly routeMap?: RouteMapDocument | { readonly map: typeof apiDocs };
}

export class ApiDocsClient {
  constructor(options: ApiDocsClientOptions);
  readonly baseUrl: string;
  readonly fetchImpl: typeof fetch;
  readonly routeMap: Readonly<Record<string, RouteOperation>>;
  call<T = unknown>(key: string, options?: CallOptions): Promise<ApiResponse<T>>;
  createMatter<T = unknown>(body: unknown, options?: CallOptions): Promise<ApiResponse<T>>;
  getMatter<T = unknown>(matterId: string, options?: CallOptions): Promise<ApiResponse<T>>;
  updateMatter<T = unknown>(
    matterId: string,
    body: unknown,
    options?: CallOptions,
  ): Promise<ApiResponse<T>>;
  walkMatter<T = unknown>(
    matterId: string,
    body: unknown,
    options?: CallOptions,
  ): Promise<ApiResponse<T>>;
}

export function route(
  key: string,
  path: string,
  methods: readonly string[],
): RouteMetadata;

export const apiDocs: Readonly<{
  createMatter: RouteMetadata;
  getMatter: RouteMetadata;
  updateMatter: RouteMetadata;
  walkMatter: RouteMetadata;
}>;

export function validateRouteMap<T = RouteMapDocument>(value: unknown): T;
export function validateRpcCall<T = unknown>(value: unknown): T;
export function validateRpcReceipt<T = unknown>(value: unknown): T;
export function loadRouteMap(path: string): RouteMapDocument;
export function createClient(options: ApiDocsClientOptions): ApiDocsClient;

export {
  Correlator as RpcV1Correlator,
  LENGTH_PREFIX_BYTES as RPC_V1_LENGTH_PREFIX_BYTES,
  MAX_FRAME_BYTES as RPC_V1_MAX_FRAME_BYTES,
  RPC_VERSION as RPC_V1_VERSION,
  RpcV1Error,
  assertReceiptForCall,
  callFromNdjson,
  callToNdjson,
  decodeCall,
  decodeReceipt,
  encodeCall,
  encodeLengthPrefixed,
  encodeReceipt,
  receiptFromNdjson,
  receiptToNdjson,
  splitLengthPrefixed,
  toNdjson,
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
