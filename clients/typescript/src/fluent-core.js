// Shared plan assembly for the unary and streaming fluent RPC builders.
//
// Both surfaces install their method set from the generated option table, so a
// method exists on a builder only if the catalog declares it for that surface.
// Spending an exclusive group produces a new builder whose method set genuinely
// omits that group: the property is absent, not merely guarded.

import {
  ENUM_HEADER_EFFECTS,
  LIST_VALUED_HEADERS,
  OPTIONS_BY_SURFACE,
  PLAN_VERSION,
  REDACTED,
  REDACTED_HEADER_NAMES,
  REDACTED_HEADER_PATTERNS,
  REDACTED_QUERY_NAMES,
  REDACTED_URL_FIELDS,
  REDACTED_URL_USERINFO,
} from "./options.generated.js";

export class RpcOptionError extends Error {
  constructor(message) {
    super(message);
    this.name = "RpcOptionError";
  }
}

function cloneObject(value) {
  return value === undefined ? undefined : { ...value };
}

function cloneBody(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? { ...value }
    : value;
}

/**
 * Mutable accumulator behind a chain. Builders are immutable facades over a
 * state that is copied on every narrowing step.
 */
/** Node's console.log / util.inspect customization hook. Inert elsewhere. */
export const INSPECT = Symbol.for("nodejs.util.inspect.custom");

export class RpcChainState {
  constructor(surface, key, rpcPath, args = {}) {
    this.surface = surface;
    this.key = key;
    this.rpcPath = rpcPath;
    this.spentGroups = new Set();
    this.hookCounts = new Map();
    this.hooks = new Map();
    this.capabilities = new Set();
    this.secrets = new Map();
    this.plan = {
      plan_version: PLAN_VERSION,
      kind: surface,
      key,
      rpc_path: rpcPath,
      serial_strategy: "json",
    };
    this.request = {
      path: cloneObject(args.path),
      query: cloneObject(args.query),
      headers: cloneObject(args.headers),
      body: cloneBody(args.body),
    };
    this.droppedHeaders = new Set();
    this.wireHeaders = {};
    // Header names written by a credential-carrying option. Their values reach
    // the wire but are redacted out of every plan, log line and debug dump.
    this.secretHeaders = new Set();
  }

  copy() {
    const next = Object.create(RpcChainState.prototype);
    next.surface = this.surface;
    next.key = this.key;
    next.rpcPath = this.rpcPath;
    next.spentGroups = new Set(this.spentGroups);
    next.hookCounts = new Map(this.hookCounts);
    next.hooks = new Map([...this.hooks].map(([k, v]) => [k, [...v]]));
    next.capabilities = new Set(this.capabilities);
    next.secrets = new Map(this.secrets);
    next.plan = { ...this.plan };
    next.request = {
      path: cloneObject(this.request.path),
      query: cloneObject(this.request.query),
      headers: cloneObject(this.request.headers),
      body: cloneBody(this.request.body),
    };
    next.droppedHeaders = new Set(this.droppedHeaders);
    next.wireHeaders = { ...this.wireHeaders };
    next.secretHeaders = new Set(this.secretHeaders);
    return next;
  }

  /** Canonical request plan: sorted keys, no credentials, no hook bodies. */
  toPlan() {
    return sortKeys(redactDocument(assembleDocument(this), this));
  }

  /**
   * Identity of this call for caching and in-flight deduplication.
   *
   * NEVER key a cache on toPlan(). A plan is redacted by design, so two callers
   * holding different credentials produce identical plans, and a plan-keyed
   * cache serves one principal's response to another. That happened here twice:
   * first through the bearer token, then — after the real headers were added to
   * the key — through a credential carried in a query field, which redaction
   * collapses just the same.
   *
   * This is the PRE-redaction document, so everything redaction can collapse is
   * present by construction: headers, query fields, URL userinfo, and any class
   * of field redaction learns to hide later. It holds raw credentials: keep it
   * in memory, prefer executionDigest() for anything stored, and never log or
   * serialize it.
   */
  executionIdentity() {
    return JSON.stringify(this.wirePlan());
  }

  /**
   * The call as it goes on the wire, credentials intact: what a transport
   * EXECUTES, where toPlan() is what anyone LOGS.
   *
   * A transport handed only the plan cannot do its job. viaProxy() with
   * credentials in the URL reached it as `http://redacted@proxy`, so the proxy
   * answered 407 and nothing said why. Same fields as the plan, same order,
   * nothing redacted — never log or persist it.
   */
  wirePlan() {
    return sortKeys(assembleDocument(this));
  }

  /**
   * How a call is SHOWN: JSON.stringify(call), console.log(call), an error
   * reporter serializing whatever was attached to the error. All of them walk
   * the object's own fields, and those hold everything redaction exists to
   * hide — `wireHeaders.authorization`, the `secrets` map, a credential in a
   * query field. Both hooks answer with the redacted plan instead.
   */
  toJSON() {
    return this.toPlan();
  }

  [INSPECT]() {
    return { RpcChainState: this.toPlan() };
  }
}

/**
 * Does this header name carry a credential?
 *
 * Matched case-insensitively against the contract's exact names and its
 * substring patterns, so `X-Api-Key` and `x-tenant-api-key` are both caught
 * without enumerating every vendor spelling.
 */
/** Lowercase and map `_` to `-`, so `Access_Token` and `access-token` are one name. */
function normalizeFieldName(name) {
  return String(name).toLowerCase().replaceAll("_", "-");
}

export function headerIsSensitive(name) {
  const normalized = normalizeFieldName(name);
  return (
    REDACTED_HEADER_NAMES.includes(normalized) ||
    REDACTED_HEADER_PATTERNS.some((pattern) => normalized.includes(pattern))
  );
}

/** Does this query-field name carry a credential? */
export function queryFieldIsSensitive(name) {
  const normalized = normalizeFieldName(name);
  return (
    REDACTED_QUERY_NAMES.includes(normalized) ||
    REDACTED_HEADER_PATTERNS.some((pattern) => normalized.includes(pattern))
  );
}

/**
 * The exact bytes a plan is compared by: compact JSON, sorted keys. JavaScript
 * already writes an integral number as an integer, which is the canonical
 * spelling the Rust client normalizes to.
 */
export function canonicalPlanString(plan) {
  return JSON.stringify(plan);
}

/**
 * Replace `user:password@` in a URL without otherwise rewriting it.
 *
 * The replacement is REDACTED_URL_USERINFO, not REDACTED: RFC 3986 userinfo
 * admits only unreserved, pct-encoded and sub-delim characters, so "[redacted]"
 * would turn every redacted proxy URL into an invalid URI.
 *
 * Deliberately textual rather than URL-parsing: a plan must redact the same
 * bytes in every language, and parser normalization differs between them.
 */
export function stripUrlUserinfo(url) {
  const schemeEnd = url.indexOf("://");
  if (schemeEnd < 0) return url;
  const authorityStart = schemeEnd + 3;
  const rest = url.slice(authorityStart);
  const match = /[/?#]/.exec(rest);
  const authorityEnd = match ? match.index : rest.length;
  const authority = rest.slice(0, authorityEnd);
  const at = authority.lastIndexOf("@");
  if (at < 0) return url;
  return (
    url.slice(0, authorityStart) + REDACTED_URL_USERINFO + authority.slice(at) + rest.slice(authorityEnd)
  );
}

/**
 * The complete call document, before any redaction.
 *
 * toPlan() and executionIdentity() both start here, which is what keeps them
 * from drifting: redaction is a pure function applied afterwards, so it can only
 * remove information the identity already has.
 */
function assembleDocument(state) {
  const document = { ...state.plan };
  for (const [field, count] of state.hookCounts) document[field] = count;
  if (state.request.path !== undefined) document.path = state.request.path;
  if (state.request.query !== undefined) document.query = state.request.query;
  if (state.request.body !== undefined) document.body = state.request.body;
  const headers = wireHeadersFor(state);
  if (Object.keys(headers).length > 0) document.headers = headers;
  return document;
}

/** Is this header a comma-separated directive list, per the contract? */
export function headerIsListValued(name) {
  return LIST_VALUED_HEADERS.includes(String(name).toLowerCase());
}

/**
 * Union of two comma-separated directive lists: trimmed, de-duplicated and
 * sorted, so the result does not depend on the order the parts were written.
 * Sorted by UTF-16 code unit, which for these ASCII directives is the byte
 * order the Rust client sorts by.
 */
export function mergeDirectives(existing, added) {
  const directives = new Set();
  for (const part of `${existing},${added}`.split(",")) {
    const directive = part.trim();
    if (directive !== "") directives.add(directive);
  }
  return [...directives].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join(", ");
}

/** Record a header written by an option. A list-valued header accumulates. */
function writeWireHeader(state, name, value) {
  const existing = state.wireHeaders[name];
  state.wireHeaders[name] =
    typeof existing === "string" && headerIsListValued(name)
      ? mergeDirectives(existing, value)
      : value;
}

/**
 * Final-boundary redaction. An option-level secret flag cannot cover this: a
 * caller can put a credential into any header through addHeader, into a query
 * field, or into a URL as userinfo. Every value is judged by the name of the
 * field carrying it, whatever wrote it; headers a secret option declared are
 * redacted as well, whatever they are called.
 */
function redactDocument(document, state) {
  const plan = { ...document };
  if (plan.headers !== undefined) {
    plan.headers = { ...plan.headers };
    for (const name of state.secretHeaders) {
      if (name in plan.headers) plan.headers[name] = REDACTED;
    }
    for (const name of Object.keys(plan.headers)) {
      if (headerIsSensitive(name)) plan.headers[name] = REDACTED;
    }
  }
  for (const field of REDACTED_URL_FIELDS) {
    if (typeof plan[field] === "string") plan[field] = stripUrlUserinfo(plan[field]);
  }
  if (plan.query !== undefined) {
    plan.query = { ...plan.query };
    for (const name of Object.keys(plan.query)) {
      if (queryFieldIsSensitive(name)) plan.query[name] = REDACTED;
    }
  }
  return plan;
}

/**
 * SHA-256 of the execution identity, for use as a cache or dedupe key.
 *
 * A digest rather than the identity itself, so the scheduler's maps do not
 * retain a second copy of every credential as a long-lived string key. If
 * WebCrypto is unavailable, fail closed instead of returning the raw identity:
 * cache/dedupe are optional optimizations, while retaining credentials in map
 * keys is a security regression.
 */
export async function executionDigest(state) {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) {
    throw new RpcOptionError(
      "secure cache/dedupe identity requires WebCrypto SHA-256; refusing to retain the raw execution identity",
    );
  }
  const identity = state.executionIdentity();
  const bytes = new TextEncoder().encode(identity);
  const digest = new Uint8Array(await subtle.digest("SHA-256", bytes));
  let hex = "";
  for (const byte of digest) hex += byte.toString(16).padStart(2, "0");
  return hex;
}

function sortKeys(value) {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value === null || typeof value !== "object") return value;
  const out = {};
  for (const key of Object.keys(value).sort()) out[key] = sortKeys(value[key]);
  return out;
}

function checkParam(optionId, param, value) {
  const { type, minimum, maximum, maxLength } = param;
  if (type === "u8" || type === "u32" || type === "i64") {
    if (!Number.isInteger(value)) {
      throw new RpcOptionError(`${optionId}: ${param.name} must be an integer`);
    }
  } else if (type === "f64") {
    if (typeof value !== "number" || !Number.isFinite(value)) {
      throw new RpcOptionError(`${optionId}: ${param.name} must be a finite number`);
    }
  } else if (type === "bool") {
    if (typeof value !== "boolean") {
      throw new RpcOptionError(`${optionId}: ${param.name} must be a boolean`);
    }
  } else if (type === "string" || type === "secret_string" || type === "url") {
    if (typeof value !== "string" || value.length === 0) {
      throw new RpcOptionError(`${optionId}: ${param.name} must be a non-empty string`);
    }
  } else if (type === "callback") {
    if (typeof value !== "function") {
      throw new RpcOptionError(`${optionId}: ${param.name} must be a function`);
    }
  }
  if (minimum !== undefined && typeof value === "number" && value < minimum) {
    throw new RpcOptionError(
      `${optionId}: ${param.name} must be at least ${minimum}, received ${value}`,
    );
  }
  if (maximum !== undefined && typeof value === "number" && value > maximum) {
    throw new RpcOptionError(
      `${optionId}: ${param.name} must be at most ${maximum}, received ${value}`,
    );
  }
  if (maxLength !== undefined && typeof value === "string" && value.length > maxLength) {
    throw new RpcOptionError(
      `${optionId}: ${param.name} must be at most ${maxLength} characters`,
    );
  }
}

/** Request-shape options are accumulators, not plan-field writers. */
const REQUEST_SHAPE = new Set([
  "add_header",
  "add_headers",
  "add_path_field",
  "add_query_field",
  "add_body_field",
  "with_body",
]);

function applyRequestShape(state, optionId, args) {
  const request = state.request;
  switch (optionId) {
    case "add_header":
      request.headers ??= {};
      request.headers[String(args[0]).toLowerCase()] = args[1];
      return;
    case "add_headers":
      request.headers = { ...(request.headers ?? {}) };
      for (const [name, value] of Object.entries(args[0] ?? {})) {
        request.headers[name.toLowerCase()] = value;
      }
      return;
    case "add_path_field":
      request.path ??= {};
      request.path[args[0]] = args[1];
      return;
    case "add_query_field":
      request.query ??= {};
      request.query[args[0]] = args[1];
      return;
    case "add_body_field":
      if (request.body === undefined) request.body = {};
      if (
        request.body === null ||
        typeof request.body !== "object" ||
        Array.isArray(request.body)
      ) {
        throw new RpcOptionError("add_body_field requires an object RPC body");
      }
      request.body[args[0]] = args[1];
      return;
    case "with_body":
      request.body = cloneBody(args[0]);
      return;
    default:
      throw new RpcOptionError(`unhandled request-shape option ${optionId}`);
  }
}

function applyWire(state, descriptor, args) {
  const wire = descriptor.wire;
  if (!wire) return;
  for (const name of wire.dropHeaders ?? []) state.droppedHeaders.add(name);
  for (const [name, template] of Object.entries(wire.headers ?? {})) {
    // Three cache options each used to assign `cache-control` outright: the
    // last one won, the others' directives vanished, and the plan depended on
    // the order the options were called in.
    writeWireHeader(state, name, renderTemplate(template, descriptor, args, state));
    if (descriptor.secret) state.secretHeaders.add(name);
  }
  if (wire.headersFromEnum) {
    const param = descriptor.params.find((p) => p.type === "enum");
    const effects = ENUM_HEADER_EFFECTS[param?.enumId ?? ""]?.[args[0]];
    for (const [name, value] of Object.entries(effects ?? {})) {
      writeWireHeader(state, name, value);
    }
  }
}

function renderTemplate(template, descriptor, args, state) {
  return template.replace(/\{([^}]+)\}/g, (_match, expression) => {
    // `{enabled:keep-alive|close}` selects on a boolean parameter.
    const [name, choices] = expression.split(":");
    const index = descriptor.params.findIndex((p) => p.name === name);
    const value = index >= 0 ? args[index] : undefined;
    if (choices) {
      const [whenTrue, whenFalse] = choices.split("|");
      return value ? whenTrue : whenFalse;
    }
    const param = descriptor.params[index];
    if (param?.type === "secret_string") {
      // The credential reaches the wire but never the plan or a log line.
      state.secrets.set(descriptor.id, String(value));
      return String(value);
    }
    return String(value);
  });
}

/**
 * Build the method set for a surface, excluding every spent exclusive group.
 * `construct` receives the next state and returns the next builder facade.
 */
export function installMethods(target, state, construct) {
  const descriptors = OPTIONS_BY_SURFACE[state.surface];
  for (const descriptor of descriptors) {
    if (descriptor.exclusiveGroup && state.spentGroups.has(descriptor.exclusiveGroup)) {
      // Genuinely omitted: the property is absent from the narrowed builder.
      continue;
    }
    Object.defineProperty(target, descriptor.method, {
      enumerable: false,
      configurable: false,
      writable: false,
      value: (...args) => applyOption(state, descriptor, args, construct),
    });
  }
  return target;
}

function applyOption(state, descriptor, args, construct) {
  if (descriptor.requiresCapability && !state.capabilities.has(descriptor.requiresCapability)) {
    throw new RpcOptionError(
      `${descriptor.id} requires the ${descriptor.requiresCapability} capability, which this client was not granted`,
    );
  }
  descriptor.params.forEach((param, index) => checkParam(descriptor.id, param, args[index]));

  const next = state.copy();
  if (descriptor.exclusiveGroup) next.spentGroups.add(descriptor.exclusiveGroup);

  if (REQUEST_SHAPE.has(descriptor.id)) {
    applyRequestShape(next, descriptor.id, args);
    return construct(next);
  }

  if (descriptor.planField) {
    if (descriptor.params.some((p) => p.type === "callback")) {
      const field = descriptor.planField;
      next.hookCounts.set(field, (next.hookCounts.get(field) ?? 0) + 1);
      const existing = next.hooks.get(descriptor.id) ?? [];
      next.hooks.set(descriptor.id, [...existing, args[0]]);
    } else if (descriptor.planValue !== undefined) {
      next.plan[descriptor.planField] = descriptor.planValue;
    } else if (descriptor.params.length > 1) {
      const composite = {};
      descriptor.params.forEach((param, index) => {
        composite[param.name] = args[index];
      });
      next.plan[descriptor.planField] = composite;
    } else {
      next.plan[descriptor.planField] = args[0];
    }
  }

  applyWire(next, descriptor, args);
  return construct(next);
}

/** Redact every credential this chain captured, for debug output. */
export function redactedHeaders(state) {
  const headers = wireHeadersFor(state);
  // Debug output redacts by the same rule as the plan, so turning on debug()
  // cannot reveal what toPlan() is careful to hide.
  for (const name of Object.keys(headers)) {
    if (headerIsSensitive(name)) headers[name] = REDACTED;
  }
  for (const value of state.secrets.values()) {
    for (const [name, headerValue] of Object.entries(headers)) {
      if (typeof headerValue === "string" && headerValue.includes(value)) {
        headers[name] = headerValue.replace(value, REDACTED);
      }
    }
  }
  return headers;
}

/** Headers as they go on the wire, credentials intact. */
export function wireHeadersFor(state) {
  const headers = { ...(state.request.headers ?? {}) };
  for (const [name, value] of Object.entries(state.wireHeaders)) {
    // An option's directive joins the caller's own rather than replacing it:
    // addHeader("cache-control", "max-age=0") then requireFresh() sends both.
    const existing = headers[name];
    headers[name] =
      typeof existing === "string" && typeof value === "string" && headerIsListValued(name)
        ? mergeDirectives(existing, value)
        : value;
  }
  for (const name of state.droppedHeaders) delete headers[name];
  return headers;
}
