export type RpcV1Transport = "http" | "tcp" | "websocket" | "nats";

export interface RpcV1Call {
  readonly v: 1;
  readonly op: "call";
  readonly id: string;
  readonly key: string;
  readonly transport?: RpcV1Transport;
  readonly path?: Readonly<Record<string, unknown>>;
  readonly query?: Readonly<Record<string, unknown>>;
  readonly headers?: Readonly<Record<string, unknown>>;
  readonly body?: unknown;
  readonly traceId?: string;
  readonly spanId?: string;
}

export type RpcV1SuccessReceipt = Readonly<{
  v: 1;
  op: "receipt";
  id: string;
  key: string;
  transport?: RpcV1Transport;
  ok: true;
  status?: number;
  body?: unknown;
  error?: never;
  traceId?: string;
  spanId?: string;
}>;

export type RpcV1ErrorReceipt = Readonly<{
  v: 1;
  op: "receipt";
  id: string;
  key: string;
  transport?: RpcV1Transport;
  ok: false;
  status?: number;
  body?: never;
  error: Readonly<Record<string, unknown>>;
  traceId?: string;
  spanId?: string;
}>;

export type RpcV1Receipt = RpcV1SuccessReceipt | RpcV1ErrorReceipt;
export type RpcV1Envelope = RpcV1Call | RpcV1Receipt;

export class RpcV1Error extends Error {}

export const RPC_VERSION: 1;
export const MAX_FRAME_BYTES: number;
export const LENGTH_PREFIX_BYTES: 4;

export function validateCall<T extends RpcV1Call>(frame: T): T;
export function validateReceipt<T extends RpcV1Receipt>(frame: T): T;

export function encodeCall(input: Omit<RpcV1Call, "v" | "op">): RpcV1Call;
export function encodeReceipt(
  input:
    | Omit<RpcV1SuccessReceipt, "v" | "op">
    | Omit<RpcV1ErrorReceipt, "v" | "op">,
): RpcV1Receipt;

export function decodeCall(payload: string | Uint8Array): RpcV1Call;
export function decodeReceipt(payload: string | Uint8Array): RpcV1Receipt;
export function toNdjson(frame: RpcV1Envelope): string;
export const callToNdjson: typeof toNdjson;
export const receiptToNdjson: typeof toNdjson;
export function callFromNdjson(payload: string | Uint8Array): RpcV1Call;
export function receiptFromNdjson(payload: string | Uint8Array): RpcV1Receipt;
export function encodeLengthPrefixed(frame: RpcV1Envelope): Uint8Array;
export function splitLengthPrefixed(
  buffer: Uint8Array | ArrayBuffer,
): { frames: Uint8Array[]; rest: Uint8Array };
export function assertReceiptForCall(
  call: RpcV1Call,
  receipt: RpcV1Receipt,
): RpcV1Receipt;

export class Correlator {
  constructor(prefix?: string);
  readonly prefix: string;
  take(): string;
}
