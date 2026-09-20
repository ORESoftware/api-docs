export class RpcRemoteError extends Error {
  constructor(ctx) {
    super(`RPC ${ctx.key} failed with status ${ctx.status}`);
    this.name = "RpcRemoteError";
    this.ctx = ctx;
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

const RPC_ENDPOINT_TARGETS = new Set(["default", "standalone", "lambda"]);

function requireBaseUrl(name, value, { optional = false } = {}) {
  if (value === undefined && optional) return undefined;
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`RPC ${name} must be a non-empty string`);
  }
  return value;
}

function normalizeEndpointTarget(options = {}, defaultTarget = "standalone") {
  const explicit = options.target ?? "default";
  if (!RPC_ENDPOINT_TARGETS.has(explicit)) {
    throw new TypeError(
      `RPC endpoint target must be default, standalone, or lambda; received ${String(explicit)}`,
    );
  }
  const wantsLambda = options.lambda === true;
  const wantsStandalone = options.standalone === true;
  if (wantsLambda && wantsStandalone) {
    throw new TypeError("RPC endpoint selection cannot enable lambda and standalone together");
  }
  if (wantsLambda && explicit !== "default" && explicit !== "lambda") {
    throw new TypeError("RPC endpoint target conflicts with lambda=true");
  }
  if (wantsStandalone && explicit !== "default" && explicit !== "standalone") {
    throw new TypeError("RPC endpoint target conflicts with standalone=true");
  }
  const selected = wantsLambda ? "lambda" : wantsStandalone ? "standalone" : explicit;
  return selected === "default" ? defaultTarget : selected;
}

export class RpcCallBuilder {
  constructor(baseUrl, rpcPath, fetchImpl, key, args = {}, endpointTarget = "standalone") {
    this.baseUrl = baseUrl;
    this.rpcPath = rpcPath;
    this.fetchImpl = fetchImpl;
    this.key = key;
    this.endpointTarget = endpointTarget;
    this.args = {
      ...args,
      path: cloneObject(args.path),
      query: cloneObject(args.query),
      headers: cloneObject(args.headers),
      body: cloneBody(args.body),
    };
  }

  addHeader(name, value) {
    this.args.headers ??= {};
    this.args.headers[name] = value;
    return this;
  }

  addHeaders(values) {
    this.args.headers = { ...(this.args.headers ?? {}), ...values };
    return this;
  }

  addPathField(name, value) {
    this.args.path ??= {};
    this.args.path[name] = value;
    return this;
  }

  addQueryField(name, value) {
    this.args.query ??= {};
    this.args.query[name] = value;
    return this;
  }

  addBodyField(name, value) {
    if (this.args.body === undefined) this.args.body = {};
    if (
      this.args.body === null ||
      typeof this.args.body !== "object" ||
      Array.isArray(this.args.body)
    ) {
      throw new TypeError("addBodyField requires an object RPC body");
    }
    this.args.body[name] = value;
    return this;
  }

  withBody(body) {
    this.args.body = cloneBody(body);
    return this;
  }

  withTraceId(traceId) {
    this.args.traceId = traceId;
    return this;
  }

  withSpanId(spanId) {
    this.args.spanId = spanId;
    return this;
  }

  async makeCall() {
    const id =
      globalThis.crypto?.randomUUID?.() ??
      `ores-${Date.now()}-${Math.random()}`;
    const envelope = {
      v: 1,
      op: "call",
      id,
      key: this.key,
      transport: "http",
      ...(this.args.path === undefined ? {} : { path: this.args.path }),
      ...(this.args.query === undefined ? {} : { query: this.args.query }),
      ...(this.args.headers === undefined ? {} : { headers: this.args.headers }),
      ...(this.args.body === undefined ? {} : { body: this.args.body }),
      ...(this.args.traceId === undefined ? {} : { traceId: this.args.traceId }),
      ...(this.args.spanId === undefined ? {} : { spanId: this.args.spanId }),
    };

    // Critical invariant: endpoint placement is local client policy only. The
    // exact same RPC envelope is sent to standalone and Lambda HTTP ingress.
    // Direct provider invocation is a different carrier and is never selected
    // by this endpoint switch.
    const response = await this.fetchImpl(new URL(this.rpcPath, this.baseUrl), {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(envelope),
    });

    let receipt;
    try {
      receipt = await response.json();
    } catch (cause) {
      throw new Error("RPC receipt is not valid JSON", { cause });
    }
    if (receipt === null || typeof receipt !== "object" || Array.isArray(receipt)) {
      throw new Error("RPC receipt must be an object");
    }
    if (receipt.v !== 1 || receipt.op !== "receipt") {
      throw new Error("RPC receipt protocol discriminator mismatch");
    }
    if (receipt.id !== id || receipt.key !== this.key) {
      throw new Error("RPC receipt correlation mismatch");
    }
    if (typeof receipt.ok !== "boolean") {
      throw new Error("RPC receipt is missing boolean ok");
    }

    const status = Number.isInteger(receipt.status)
      ? receipt.status
      : response.status;
    const transport =
      typeof receipt.transport === "string" ? receipt.transport : "http";
    const errors = receipt.error === undefined ? [] : [receipt.error];
    const traceIds =
      typeof receipt.traceId === "string" ? [receipt.traceId] : [];

    const ctx = {
      ok: receipt.ok && status < 400,
      status,
      id: receipt.id,
      key: receipt.key,
      transport,
      endpointTarget: this.endpointTarget,
      headers:
        receipt.headers && typeof receipt.headers === "object"
          ? receipt.headers
          : {},
      trailers:
        receipt.trailers && typeof receipt.trailers === "object"
          ? receipt.trailers
          : {},
      errors,
      traceId:
        typeof receipt.traceId === "string" ? receipt.traceId : undefined,
      traceIds,
      spanId: typeof receipt.spanId === "string" ? receipt.spanId : undefined,
    };
    return [receipt.body, ctx];
  }

  async makeCallOrThrow() {
    const [value, ctx] = await this.makeCall();
    if (!ctx.ok) throw new RpcRemoteError(ctx);
    if (value === undefined) {
      throw new Error(`RPC ${ctx.key} succeeded without a body`);
    }
    return value;
  }
}

export class OresRpcClient {
  constructor({
    baseUrl,
    standaloneBaseUrl = baseUrl,
    lambdaBaseUrl,
    defaultTarget = "standalone",
    rpcPath = "/v1/rpc",
    operations,
    fetchImpl = globalThis.fetch?.bind(globalThis),
  }) {
    this.baseUrl = requireBaseUrl("baseUrl", baseUrl);
    this.standaloneBaseUrl = requireBaseUrl("standaloneBaseUrl", standaloneBaseUrl);
    this.lambdaBaseUrl = requireBaseUrl("lambdaBaseUrl", lambdaBaseUrl, { optional: true });
    if (!RPC_ENDPOINT_TARGETS.has(defaultTarget) || defaultTarget === "default") {
      throw new TypeError("RPC defaultTarget must be standalone or lambda");
    }
    if (defaultTarget === "lambda" && this.lambdaBaseUrl === undefined) {
      throw new TypeError("RPC defaultTarget=lambda requires lambdaBaseUrl");
    }
    if (typeof rpcPath !== "string" || !rpcPath.startsWith("/")) {
      throw new TypeError("RPC rpcPath must be an absolute path");
    }
    if (typeof fetchImpl !== "function") {
      throw new TypeError("RPC fetch implementation is required");
    }
    this.defaultTarget = defaultTarget;
    this.rpcPath = rpcPath;
    this.fetchImpl = fetchImpl;
    this.operations = new Set(operations);
  }

  prepare(key, args = {}, endpoint = {}) {
    if (!this.operations.has(key)) {
      throw new Error(
        `RPC operation not generated for this audience: ${String(key)}`,
      );
    }
    const endpointTarget = normalizeEndpointTarget(endpoint, this.defaultTarget);
    const baseUrl =
      endpointTarget === "lambda" ? this.lambdaBaseUrl : this.standaloneBaseUrl;
    if (baseUrl === undefined) {
      throw new Error(
        `RPC endpoint target ${endpointTarget} is not configured for ${String(key)}`,
      );
    }
    return new RpcCallBuilder(
      baseUrl,
      this.rpcPath,
      this.fetchImpl,
      key,
      args,
      endpointTarget,
    );
  }

  call(key, args = {}, endpoint = {}) {
    return this.prepare(key, args, endpoint);
  }
}
