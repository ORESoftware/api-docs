export const RPC_VERSION = 1;
export const MAX_FRAME_BYTES = 8 * 1024 * 1024;
export const LENGTH_PREFIX_BYTES = 4;
const TRANSPORTS = new Set(["http", "tcp", "websocket", "nats"]);
const CALL_FIELDS = new Set(["v", "op", "id", "key", "transport", "path", "query", "headers", "body", "traceId", "spanId"]);
const RECEIPT_FIELDS = new Set(["v", "op", "id", "key", "transport", "ok", "status", "body", "error", "traceId", "spanId"]);
const CALL_INPUT_FIELDS = CALL_FIELDS;
const RECEIPT_INPUT_FIELDS = RECEIPT_FIELDS;
const has = (object, name) => Object.prototype.hasOwnProperty.call(object, name);
function isUnicodeScalarString(value) {
  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      if (index + 1 >= value.length) return false;
      const next = value.charCodeAt(++index);
      if (next < 0xdc00 || next > 0xdfff) return false;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return false;
    }
  }
  return true;
}

export class RpcV1Error extends Error {
  constructor(message) { super(message); this.name = "RpcV1Error"; }
}
function fail(message) { throw new RpcV1Error(message); }
function isPlainObject(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}
function validateJson(value, path = "value", seen = new Set()) {
  if (value === null || typeof value === "boolean") return;
  if (typeof value === "string") {
    if (!isUnicodeScalarString(value)) fail(`${path} must contain Unicode scalar values`);
    return;
  }
  if (typeof value === "number") { if (!Number.isFinite(value)) fail(`${path} must be finite JSON`); return; }
  if (Array.isArray(value)) {
    if (seen.has(value)) fail(`${path} must not be cyclic`);
    seen.add(value);
    value.forEach((item, index) => validateJson(item, `${path}[${index}]`, seen));
    seen.delete(value);
    return;
  }
  if (isPlainObject(value)) {
    if (seen.has(value)) fail(`${path} must not be cyclic`);
    seen.add(value);
    for (const name of Reflect.ownKeys(value)) {
      if (typeof name !== "string") fail(`${path} object keys must be strings`);
      validateJson(value[name], `${path}.${name}`, seen);
    }
    seen.delete(value);
    return;
  }
  fail(`${path} must contain one JSON value`);
}
function validateString(value, name, max) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    [...value].length > max ||
    !isUnicodeScalarString(value)
  ) fail(`${name} must be 1..${max} Unicode scalar values`);
}
function validateCommon(frame, op, fields) {
  if (!isPlainObject(frame)) fail(`${op} must be a plain object`);
  const ownKeys = Reflect.ownKeys(frame);
  if (ownKeys.some((name) => typeof name !== "string")) {
    fail(`${op} member names must be strings`);
  }
  const unknown = ownKeys.filter((name) => !fields.has(name)).sort();
  if (unknown.length) fail(`unknown ${op} member(s): ${unknown.join(", ")}`);
  if (frame.v !== RPC_VERSION) fail(`unsupported RPC version ${String(frame.v)}`);
  if (frame.op !== op) fail(`expected op ${op}`);
  validateString(frame.id, "id", 128);
  if (typeof frame.key !== "string" || !/^[A-Za-z][A-Za-z0-9_]*$/.test(frame.key)) fail("key must be a portable RPC identifier");
  if (has(frame, "transport") && !TRANSPORTS.has(frame.transport)) fail(`unknown transport ${String(frame.transport)}`);
  if (has(frame, "traceId")) validateString(frame.traceId, "traceId", 64);
  if (has(frame, "spanId")) validateString(frame.spanId, "spanId", 32);
}
function validateHeaders(headers) {
  validateJson(headers, "headers");
  for (const name of Object.keys(headers)) {
    if (name.length > 128 || !/^[!#$%&'*+.^_`|~0-9a-z-]+$/.test(name)) {
      fail(`header name ${name} must be a canonical lowercase HTTP field name`);
    }
  }
}
export function validateCall(frame) {
  validateCommon(frame, "call", CALL_FIELDS);
  for (const name of ["path", "query", "headers"]) {
    if (has(frame, name)) {
      if (!isPlainObject(frame[name])) fail(`${name} must be a JSON object`);
      if (name === "headers") validateHeaders(frame[name]);
      else validateJson(frame[name], name);
    }
  }
  if (has(frame, "body")) validateJson(frame.body, "body");
  return frame;
}
export function validateReceipt(frame) {
  validateCommon(frame, "receipt", RECEIPT_FIELDS);
  if (typeof frame.ok !== "boolean") fail("ok must be a boolean");
  if (has(frame, "status") && (!Number.isInteger(frame.status) || frame.status < 100 || frame.status > 599)) fail("status must be an integer from 100 to 599");
  if (has(frame, "body")) validateJson(frame.body, "body");
  if (has(frame, "error")) {
    if (!isPlainObject(frame.error)) fail("error must be a JSON object");
    validateJson(frame.error, "error");
  }
  if (frame.ok) {
    if (has(frame, "error")) fail("a successful receipt must not carry error");
    if (has(frame, "status") && (frame.status < 200 || frame.status > 399)) fail("a successful receipt status must be 200..399");
  } else {
    if (has(frame, "body")) fail("an error receipt must not carry body");
    if (!has(frame, "error")) fail("an error receipt needs error");
    if (has(frame, "status") && (frame.status < 400 || frame.status > 599)) fail("an error receipt status must be 400..599");
  }
  return frame;
}
function copyOptional(target, source, name) { if (has(source, name)) target[name] = source[name]; }
function validateConstructorInput(input, name, fields) {
  if (!isPlainObject(input)) fail(`${name} input must be a plain object`);
  const keys = Reflect.ownKeys(input);
  if (keys.some((key) => typeof key !== "string")) fail(`${name} input member names must be strings`);
  const unknown = keys.filter((key) => !fields.has(key)).sort();
  if (unknown.length) fail(`unknown ${name} input member(s): ${unknown.join(", ")}`);
}
export function encodeCall(input) {
  validateConstructorInput(input, "call", CALL_INPUT_FIELDS);
  if (has(input, "v") && input.v !== RPC_VERSION) fail(`unsupported RPC version ${String(input.v)}`);
  if (has(input, "op") && input.op !== "call") fail("expected op call");
  const frame = { v: RPC_VERSION, op: "call", id: input.id, key: input.key };
  for (const name of ["transport", "path", "query", "headers", "body", "traceId", "spanId"]) copyOptional(frame, input, name);
  validateCall(frame);
  return Object.freeze(frame);
}
export function encodeReceipt(input) {
  validateConstructorInput(input, "receipt", RECEIPT_INPUT_FIELDS);
  if (has(input, "v") && input.v !== RPC_VERSION) fail(`unsupported RPC version ${String(input.v)}`);
  if (has(input, "op") && input.op !== "receipt") fail("expected op receipt");
  const frame = { v: RPC_VERSION, op: "receipt", id: input.id, key: input.key };
  copyOptional(frame, input, "transport");
  frame.ok = input.ok;
  for (const name of ["status", "body", "error", "traceId", "spanId"]) copyOptional(frame, input, name);
  validateReceipt(frame);
  return Object.freeze(frame);
}
function textFromPayload(payload) {
  if (typeof payload === "string") {
    const size = new TextEncoder().encode(payload).length;
    if (size > MAX_FRAME_BYTES) fail(`frame is ${size} bytes, over the ${MAX_FRAME_BYTES} limit`);
    return payload;
  }
  if (!(payload instanceof Uint8Array)) fail("payload must be a string or Uint8Array");
  if (payload.length > MAX_FRAME_BYTES) fail(`frame is ${payload.length} bytes, over the ${MAX_FRAME_BYTES} limit`);
  try { return new TextDecoder("utf-8", { fatal: true }).decode(payload); }
  catch (error) { fail(`frame is not UTF-8: ${String(error)}`); }
}
function parsePayload(payload) {
  const text = textFromPayload(payload);
  try { return JSON.parse(text); }
  catch (error) { fail(`frame is not JSON: ${String(error)}`); }
}
export function decodeCall(payload) { return encodeCall(validateCall(parsePayload(payload))); }
export function decodeReceipt(payload) { return encodeReceipt(validateReceipt(parsePayload(payload))); }
function canonicalBytes(frame) {
  const validated = frame?.op === "call" ? encodeCall(frame) : frame?.op === "receipt" ? encodeReceipt(frame) : fail("unknown RPC envelope op");
  let text;
  try { text = JSON.stringify(validated); } catch (error) { fail(`frame is not JSON-encodable: ${String(error)}`); }
  const bytes = new TextEncoder().encode(text);
  if (bytes.length > MAX_FRAME_BYTES) fail(`frame is ${bytes.length} bytes, over the ${MAX_FRAME_BYTES} limit`);
  return bytes;
}
export function toNdjson(frame) { return `${new TextDecoder().decode(canonicalBytes(frame))}\n`; }
function stripOneTerminator(text) {
  if (text.endsWith("\r\n")) text = text.slice(0, -2);
  else if (text.endsWith("\n")) text = text.slice(0, -1);
  if (text.includes("\n") || text.includes("\r")) fail("NDJSON input must contain exactly one JSON object");
  if (!text) fail("NDJSON input is empty");
  return text;
}
export function callFromNdjson(payload) { return decodeCall(stripOneTerminator(textFromPayload(payload))); }
export function receiptFromNdjson(payload) { return decodeReceipt(stripOneTerminator(textFromPayload(payload))); }
export function encodeLengthPrefixed(frame) {
  const payload = canonicalBytes(frame);
  const out = new Uint8Array(LENGTH_PREFIX_BYTES + payload.length);
  new DataView(out.buffer).setUint32(0, payload.length, false);
  out.set(payload, LENGTH_PREFIX_BYTES);
  return out;
}
export function splitLengthPrefixed(buffer) {
  if (!(buffer instanceof Uint8Array)) buffer = new Uint8Array(buffer);
  const frames = [];
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  let offset = 0;
  while (buffer.length - offset >= LENGTH_PREFIX_BYTES) {
    const length = view.getUint32(offset, false);
    if (length > MAX_FRAME_BYTES) fail(`declared frame length ${length} is over the ${MAX_FRAME_BYTES} limit`);
    const start = offset + LENGTH_PREFIX_BYTES;
    if (buffer.length - start < length) break;
    frames.push(buffer.subarray(start, start + length));
    offset = start + length;
  }
  return { frames, rest: buffer.subarray(offset) };
}
export function assertReceiptForCall(call, receipt) {
  validateCall(call); validateReceipt(receipt);
  if (receipt.id !== call.id) fail("receipt id does not match call id");
  if (receipt.key !== call.key) fail("receipt key does not match call key");
  if (has(call, "transport") && has(receipt, "transport") && receipt.transport !== call.transport) fail("receipt transport does not match call transport");
  return receipt;
}
export class Correlator {
  #next = 0n;
  constructor(prefix = "") {
    if (typeof prefix !== "string" || !isUnicodeScalarString(prefix)) {
      fail("correlation prefix must contain Unicode scalar values");
    }
    this.prefix = prefix;
  }
  take() {
    this.#next += 1n;
    const value = `${this.prefix}${this.#next}`;
    validateString(value, "correlation id", 128);
    return value;
  }
}

export const callToNdjson = toNdjson;
export const receiptToNdjson = toNdjson;
