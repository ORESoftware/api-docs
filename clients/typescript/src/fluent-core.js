// Shared plan assembly for the unary and streaming fluent RPC builders.
//
// Both surfaces install their method set from the generated option table, so a
// method exists on a builder only if the catalog declares it for that surface.
// Spending an exclusive group produces a new builder whose method set genuinely
// omits that group: the property is absent, not merely guarded.

import {
  ENUM_HEADER_EFFECTS,
  OPTIONS_BY_SURFACE,
  PLAN_VERSION,
} from "./options.generated.js";

export class RpcOptionError extends Error {
  constructor(message) {
    super(message);
    this.name = "RpcOptionError";
  }
}

const REDACTED = "[redacted]";

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
    const plan = { ...this.plan };
    for (const [field, count] of this.hookCounts) plan[field] = count;
    if (this.request.path !== undefined) plan.path = this.request.path;
    if (this.request.query !== undefined) plan.query = this.request.query;
    if (this.request.body !== undefined) plan.body = this.request.body;
    const headers = { ...(this.request.headers ?? {}), ...this.wireHeaders };
    for (const name of this.droppedHeaders) delete headers[name];
    for (const name of this.secretHeaders) {
      if (name in headers) headers[name] = REDACTED;
    }
    if (Object.keys(headers).length > 0) plan.headers = headers;
    return sortKeys(plan);
  }
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
    state.wireHeaders[name] = renderTemplate(template, descriptor, args, state);
    if (descriptor.secret) state.secretHeaders.add(name);
  }
  if (wire.headersFromEnum) {
    const param = descriptor.params.find((p) => p.type === "enum");
    const effects = ENUM_HEADER_EFFECTS[param?.enumId ?? ""]?.[args[0]];
    for (const [name, value] of Object.entries(effects ?? {})) {
      state.wireHeaders[name] = value;
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
  const headers = { ...(state.request.headers ?? {}), ...state.wireHeaders };
  for (const name of state.droppedHeaders) delete headers[name];
  for (const value of state.secrets.values()) {
    for (const [name, headerValue] of Object.entries(headers)) {
      if (typeof headerValue === "string" && headerValue.includes(value)) {
        headers[name] = headerValue.replace(value, REDACTED);
      }
    }
  }
  return headers;
}

export function wireHeadersFor(state) {
  const headers = { ...(state.request.headers ?? {}), ...state.wireHeaders };
  for (const name of state.droppedHeaders) delete headers[name];
  return headers;
}
