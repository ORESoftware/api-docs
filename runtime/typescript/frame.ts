/**
 * The RIDL frame envelope: HTTP-free addressing for WebSocket and TCP.
 *
 * This is a browser-safe port of `ridl/framing.py`. It has no Node imports and
 * produces the same canonical UTF-8 JSON bytes as the Rust, Dart, Go, and
 * Python ports.
 */

export const FRAME_VERSION = 1 as const;
export const MAX_FRAME_BYTES = 8 * 1024 * 1024;
export const LENGTH_PREFIX_BYTES = 4;

const FIELD_ORDER = [
  "v",
  "id",
  "t",
  "key",
  "method",
  "path",
  "query",
  "body",
  "code",
  "message",
  "meta",
] as const;
const FRAME_KINDS = new Set(["call", "data", "end", "error", "cancel"] as const);

export type FrameKind = "call" | "data" | "end" | "error" | "cancel";

export class FrameError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "FrameError";
  }
}

export interface Frame {
  readonly v: number;
  readonly id: string;
  readonly t: FrameKind;
  readonly key?: string;
  readonly method?: string;
  readonly path?: string;
  readonly query: ReadonlyArray<readonly [string, string]>;
  readonly body?: unknown;
  readonly hasBody: boolean;
  readonly code?: string;
  readonly message?: string;
  readonly meta: ReadonlyArray<readonly [string, string]>;
}

function bare(t: FrameKind, id: string): Frame {
  return { v: FRAME_VERSION, id, t, query: [], hasBody: false, meta: [] };
}

export function callFrame(
  id: string,
  key: string,
  method: string,
  path: string,
  query: ReadonlyArray<readonly [string, string]> = [],
  body?: { value: unknown },
): Frame {
  return {
    ...bare("call", id),
    key,
    method,
    path,
    query,
    body: body?.value,
    hasBody: body !== undefined,
  };
}

export const dataFrame = (id: string, body: unknown): Frame => ({
  ...bare("data", id),
  body,
  hasBody: true,
});
export const endFrame = (id: string): Frame => bare("end", id);
export const cancelFrame = (id: string): Frame => bare("cancel", id);
export const errorFrame = (
  id: string,
  code: string,
  message?: string,
  body?: { value: unknown },
): Frame => ({
  ...bare("error", id),
  code,
  message,
  body: body?.value,
  hasBody: body !== undefined,
});

export function withMeta(frame: Frame, name: string, value: string): Frame {
  const withoutName = frame.meta.filter(([key]) => key !== name);
  return { ...frame, meta: [...withoutName, [name, value] as const] };
}

export function validate(frame: Frame): void {
  if (frame.v !== FRAME_VERSION) {
    throw new FrameError(`unsupported frame version ${frame.v}`);
  }
  if (!FRAME_KINDS.has(frame.t)) {
    throw new FrameError(`unknown frame type ${String(frame.t)}`);
  }
  if (typeof frame.id !== "string" || !frame.id || [...frame.id].length > 128) {
    throw new FrameError("id must be 1..128 characters");
  }
  if (!Array.isArray(frame.query)) {
    throw new FrameError("query must be an array of [name, value] pairs");
  }
  for (const pair of frame.query) {
    if (
      !Array.isArray(pair) ||
      pair.length !== 2 ||
      typeof pair[0] !== "string" ||
      typeof pair[1] !== "string"
    ) {
      throw new FrameError("each query entry must be a [name, value] pair of strings");
    }
  }

  if (frame.t === "call") {
    if (typeof frame.key !== "string" || !frame.key) {
      throw new FrameError("a call frame needs an operation key");
    }
    if (typeof frame.method !== "string" || !frame.method) {
      throw new FrameError("a call frame needs a method");
    }
    if (typeof frame.path !== "string" || !frame.path.startsWith("/")) {
      throw new FrameError("a call frame needs a path starting with /");
    }
  } else if (frame.key || frame.method || frame.path || frame.query.length) {
    throw new FrameError(`a ${frame.t} frame carries no addressing fields`);
  }

  if (frame.hasBody && frame.body === undefined) {
    throw new FrameError("a present body cannot be JavaScript undefined");
  }
  if (frame.t === "data" && !frame.hasBody) {
    throw new FrameError("a data frame needs a body");
  }
  if (frame.t === "error") {
    if (typeof frame.code !== "string" || !frame.code) {
      throw new FrameError("an error frame needs a code");
    }
    if (frame.message !== undefined && typeof frame.message !== "string") {
      throw new FrameError("an error message must be a string");
    }
  } else if (frame.code !== undefined || frame.message !== undefined) {
    throw new FrameError(`a ${frame.t} frame carries no code or message`);
  }

  if (!Array.isArray(frame.meta)) {
    throw new FrameError("meta must be an array of [name, value] pairs");
  }
  const metaNames = new Set<string>();
  for (const pair of frame.meta) {
    if (
      !Array.isArray(pair) ||
      pair.length !== 2 ||
      typeof pair[0] !== "string" ||
      typeof pair[1] !== "string"
    ) {
      throw new FrameError("each meta entry must be a [name, value] pair of strings");
    }
    if (metaNames.has(pair[0])) {
      throw new FrameError(`duplicate meta member ${pair[0]}`);
    }
    metaNames.add(pair[0]);
  }
}

function toObject(frame: Frame): Record<string, unknown> {
  const raw: Record<string, unknown> = { v: frame.v, id: frame.id, t: frame.t };
  if (frame.t === "call") {
    raw.key = frame.key;
    raw.method = frame.method;
    raw.path = frame.path;
    if (frame.query.length) raw.query = frame.query.map(([key, value]) => [key, value]);
  }
  if (frame.hasBody) raw.body = frame.body;
  if (frame.t === "error") {
    raw.code = frame.code;
    if (frame.message !== undefined) raw.message = frame.message;
  }
  if (frame.meta.length) {
    raw.meta = Object.fromEntries(
      [...frame.meta].sort(([left], [right]) => left.localeCompare(right)),
    );
  }

  const ordered: Record<string, unknown> = {};
  for (const name of FIELD_ORDER) {
    if (name in raw) ordered[name] = raw[name];
  }
  return ordered;
}

export function encode(frame: Frame): Uint8Array {
  validate(frame);
  let text: string;
  try {
    const encoded = JSON.stringify(toObject(frame));
    if (encoded === undefined) {
      throw new FrameError("frame is not JSON-encodable");
    }
    text = encoded;
  } catch (cause) {
    if (cause instanceof FrameError) throw cause;
    throw new FrameError(`frame is not JSON-encodable: ${String(cause)}`);
  }
  const bytes = new TextEncoder().encode(text);
  if (bytes.length > MAX_FRAME_BYTES) {
    throw new FrameError(`frame is ${bytes.length} bytes, over the ${MAX_FRAME_BYTES} limit`);
  }
  return bytes;
}

export function encodeTcp(frame: Frame): Uint8Array {
  const payload = encode(frame);
  const out = new Uint8Array(LENGTH_PREFIX_BYTES + payload.length);
  new DataView(out.buffer).setUint32(0, payload.length, false);
  out.set(payload, LENGTH_PREFIX_BYTES);
  return out;
}

export function decode(payload: Uint8Array | string): Frame {
  let text: string;
  if (typeof payload === "string") {
    const size = new TextEncoder().encode(payload).length;
    if (size > MAX_FRAME_BYTES) {
      throw new FrameError(`frame is ${size} bytes, over the ${MAX_FRAME_BYTES} limit`);
    }
    text = payload;
  } else {
    if (payload.length > MAX_FRAME_BYTES) {
      throw new FrameError(`frame is ${payload.length} bytes, over the ${MAX_FRAME_BYTES} limit`);
    }
    try {
      text = new TextDecoder("utf-8", { fatal: true }).decode(payload);
    } catch (cause) {
      throw new FrameError(`frame is not UTF-8: ${String(cause)}`);
    }
  }

  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (cause) {
    throw new FrameError(`frame is not JSON: ${String(cause)}`);
  }
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    throw new FrameError("a frame must be a JSON object");
  }
  const obj = raw as Record<string, unknown>;
  const unknown = Object.keys(obj).filter(
    (key) => !(FIELD_ORDER as readonly string[]).includes(key),
  );
  if (unknown.length) {
    throw new FrameError(`unknown frame member(s): ${unknown.sort().join(", ")}`);
  }

  const query: Array<readonly [string, string]> = [];
  if (obj.query !== undefined) {
    if (!Array.isArray(obj.query)) {
      throw new FrameError("query must be an array of [name, value] pairs");
    }
    for (const pair of obj.query) {
      if (
        !Array.isArray(pair) ||
        pair.length !== 2 ||
        pair.some((value) => typeof value !== "string")
      ) {
        throw new FrameError("each query entry must be a [name, value] pair of strings");
      }
      query.push([pair[0] as string, pair[1] as string]);
    }
  }

  const meta: Array<readonly [string, string]> = [];
  if (obj.meta !== undefined) {
    if (typeof obj.meta !== "object" || obj.meta === null || Array.isArray(obj.meta)) {
      throw new FrameError("meta must be an object");
    }
    for (const [key, value] of Object.entries(obj.meta as Record<string, unknown>).sort(
      ([left], [right]) => left.localeCompare(right),
    )) {
      if (typeof value !== "string") {
        throw new FrameError(`meta.${key} must be a string`);
      }
      meta.push([key, value]);
    }
  }

  const frame: Frame = {
    v: typeof obj.v === "number" ? obj.v : 0,
    id: typeof obj.id === "string" ? obj.id : "",
    t: obj.t as FrameKind,
    key: obj.key as string | undefined,
    method: obj.method as string | undefined,
    path: obj.path as string | undefined,
    query,
    body: obj.body,
    hasBody: "body" in obj,
    code: obj.code as string | undefined,
    message: obj.message as string | undefined,
    meta,
  };
  validate(frame);
  return frame;
}

export function decodeStream(buffer: Uint8Array): {
  frames: Frame[];
  rest: Uint8Array;
} {
  const frames: Frame[] = [];
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  let offset = 0;
  while (buffer.length - offset >= LENGTH_PREFIX_BYTES) {
    const length = view.getUint32(offset, false);
    if (length > MAX_FRAME_BYTES) {
      throw new FrameError(
        `declared frame length ${length} is over the ${MAX_FRAME_BYTES} limit`,
      );
    }
    const start = offset + LENGTH_PREFIX_BYTES;
    if (buffer.length - start < length) break;
    frames.push(decode(buffer.subarray(start, start + length)));
    offset = start + length;
  }
  return { frames, rest: buffer.subarray(offset) };
}

export class Correlator {
  #next = 0n;
  readonly #prefix: string;

  constructor(prefix = "") {
    this.#prefix = prefix;
  }

  take(): string {
    this.#next += 1n;
    return this.#prefix ? `${this.#prefix}${this.#next}` : String(this.#next);
  }
}
