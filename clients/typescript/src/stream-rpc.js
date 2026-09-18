export class RpcStreamTransportError extends Error {
  constructor(message, reason, code, options) {
    super(message, options);
    this.name = "RpcStreamTransportError";
    this.reason = reason;
    this.code = code;
  }
}

function cloneQuery(query) {
  return Array.isArray(query) ? query.map(([name, value]) => [name, value]) : [];
}

function cloneBody(body) {
  return body !== null && typeof body === "object" && !Array.isArray(body)
    ? { ...body }
    : body;
}

export class RpcStreamClient {
  constructor(session, id, key, carrier, decode) {
    this.session = session;
    this.id = id;
    this.key = key;
    this.carrier = carrier;
    this.decode = decode;
    this.ended = false;
    this.cancelled = false;
    this.error = undefined;
  }

  get context() {
    return {
      id: this.id,
      key: this.key,
      carrier: this.carrier,
      ended: this.ended,
      cancelled: this.cancelled,
      error: this.error,
    };
  }

  async cancel() {
    if (this.ended || this.cancelled) return;
    try {
      await this.session.cancel?.();
      this.cancelled = true;
    } catch (cause) {
      const error = new RpcStreamTransportError(
        `${this.key}: stream cancellation failed`,
        "carrier",
        undefined,
        { cause },
      );
      this.error = error;
      throw error;
    }
  }

  async *[Symbol.asyncIterator]() {
    try {
      for await (const frame of this.session.incoming) {
        if (frame?.id !== this.id) {
          throw new RpcStreamTransportError(
            `frame for correlation id ${String(frame?.id)} arrived on the stream for ${this.id}`,
            "protocol",
          );
        }
        switch (frame.t) {
          case "data": {
            if (!Object.prototype.hasOwnProperty.call(frame, "body")) {
              throw new RpcStreamTransportError(
                "a stream data frame arrived without a body",
                "protocol",
              );
            }
            try {
              yield this.decode(frame.body);
            } catch (cause) {
              throw new RpcStreamTransportError(
                `${this.key}: stream data failed typed decoding`,
                "protocol",
                undefined,
                { cause },
              );
            }
            break;
          }
          case "end":
            this.ended = true;
            return;
          case "error": {
            const error = new RpcStreamTransportError(
              frame.message ?? `remote ${frame.code ?? "unknown"}`,
              "remote",
              frame.code,
            );
            this.error = error;
            throw error;
          }
          case "cancel":
            this.cancelled = true;
            return;
          case "call":
            throw new RpcStreamTransportError(
              "a call frame cannot arrive inside its response stream",
              "protocol",
            );
          default:
            throw new RpcStreamTransportError(
              `unknown stream frame type ${String(frame.t)}`,
              "protocol",
            );
        }
      }
      if (!this.ended && !this.cancelled) {
        throw new RpcStreamTransportError(
          "stream transport ended without an end or cancel frame",
          "protocol",
        );
      }
    } catch (cause) {
      const error =
        cause instanceof RpcStreamTransportError
          ? cause
          : new RpcStreamTransportError(
              `${this.key}: stream failed`,
              "carrier",
              undefined,
              { cause },
            );
      this.error = error;
      throw error;
    }
  }
}

export class RpcStreamCallBuilder {
  constructor(framedStream, nextId, key, request, decode) {
    this.framedStream = framedStream;
    this.nextId = nextId;
    this.key = key;
    this.request = {
      method: request.method,
      path: request.path,
      query: cloneQuery(request.query),
      body: cloneBody(request.body),
    };
    this.decode = decode;
    this.opened = false;
  }

  addQueryField(name, value) {
    this.request.query.push([String(name), String(value)]);
    return this;
  }

  withBody(body) {
    this.request.body = cloneBody(body);
    return this;
  }

  addBodyField(name, value) {
    if (this.request.body === undefined) this.request.body = {};
    if (
      this.request.body === null ||
      typeof this.request.body !== "object" ||
      Array.isArray(this.request.body)
    ) {
      throw new TypeError("addBodyField requires an object RPC body");
    }
    this.request.body[name] = value;
    return this;
  }

  async stream() {
    if (this.opened) {
      throw new RpcStreamTransportError(
        "an RPC stream call builder can only be opened once",
        "protocol",
      );
    }
    this.opened = true;

    const carrier = this.framedStream?.carrier;
    if (carrier !== "websocket" && carrier !== "tcp") {
      throw new RpcStreamTransportError(
        `stream carrier must be websocket or tcp, got ${String(carrier)}`,
        "protocol",
      );
    }

    const id = this.nextId();
    const call = {
      v: 1,
      id,
      t: "call",
      key: this.key,
      method: this.request.method,
      path: this.request.path,
      query: cloneQuery(this.request.query),
      ...(this.request.body === undefined
        ? {}
        : { body: cloneBody(this.request.body) }),
    };

    let session;
    try {
      // Critical invariant: this is the sole transport-open / network-I/O boundary.
      session = await this.framedStream.open(call);
    } catch (cause) {
      throw new RpcStreamTransportError(
        `${this.key}: opening stream failed`,
        "carrier",
        undefined,
        { cause },
      );
    }

    if (
      session === null ||
      typeof session !== "object" ||
      session.incoming === undefined ||
      session.incoming?.[Symbol.asyncIterator] === undefined
    ) {
      throw new RpcStreamTransportError(
        "stream transport returned an invalid session",
        "protocol",
      );
    }

    return new RpcStreamClient(
      session,
      id,
      this.key,
      carrier,
      this.decode,
    );
  }
}

export class OresRpcStreamClient {
  constructor({
    framedStream,
    operations,
    idPrefix = "stream-",
  }) {
    if (
      framedStream === null ||
      typeof framedStream !== "object" ||
      typeof framedStream.open !== "function"
    ) {
      throw new TypeError("RPC framedStream with open(call) is required");
    }
    if (framedStream.carrier !== "websocket" && framedStream.carrier !== "tcp") {
      throw new TypeError("RPC framedStream carrier must be websocket or tcp");
    }
    this.framedStream = framedStream;
    this.operations = new Set(operations);
    this.idPrefix = idPrefix;
    this.sequence = 0;
  }

  prepare(key, request, decode = (value) => value) {
    if (!this.operations.has(key)) {
      throw new Error(
        `RPC stream operation not generated for this audience: ${String(key)}`,
      );
    }
    if (
      request === null ||
      typeof request !== "object" ||
      typeof request.method !== "string" ||
      typeof request.path !== "string"
    ) {
      throw new TypeError(
        "RPC stream request requires string method and path",
      );
    }
    if (typeof decode !== "function") {
      throw new TypeError("RPC stream decoder must be a function");
    }

    const nextId = () => {
      this.sequence += 1;
      return `${this.idPrefix}${this.sequence}`;
    };

    return new RpcStreamCallBuilder(
      this.framedStream,
      nextId,
      key,
      request,
      decode,
    );
  }
}
