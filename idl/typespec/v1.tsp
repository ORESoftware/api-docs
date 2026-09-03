// v1 unary frames. JSON Schema peer: json-schema/rpc-call.schema.json
// and json-schema/rpc-receipt.schema.json. Do not mix with v2 `t` frames.

namespace Ores.Rpc.V1;

enum Transport {
  http,
  tcp,
  websocket,
  nats,
}

model RpcCall {
  v: 1;
  `op`: "call";

  @minLength(1)
  @maxLength(128)
  id: string;

  @minLength(1)
  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  key: string;

  transport?: Transport;
  path?: Record<unknown>;
  query?: Record<unknown>;
  body?: unknown;

  @minLength(1)
  @maxLength(64)
  traceId?: string;

  @minLength(1)
  @maxLength(32)
  spanId?: string;
}

// Structural comparison witness shared with JSON Schema and the flattened
// Protobuf message. Runtime and generated APIs must expose RpcReceiptState,
// not this permissive representation-level view.
model RpcReceipt {
  v: 1;
  `op`: "receipt";

  @minLength(1)
  @maxLength(128)
  id: string;

  @minLength(1)
  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  key: string;

  transport?: Transport;
  ok: boolean;

  @minValue(100)
  @maxValue(599)
  status?: int32;

  body?: unknown;
  error?: Record<unknown>;

  @minLength(1)
  @maxLength(64)
  traceId?: string;

  @minLength(1)
  @maxLength(32)
  spanId?: string;
}

// Executable TypeSpec state aliases. JSON Schema expresses the same states
// with if/then/else. Protobuf remains append-only and flattened, so generated
// adapters must run the shared semantic validator after decoding.
alias RpcSuccessReceipt = {
  v: 1;
  `op`: "receipt";

  @minLength(1)
  @maxLength(128)
  id: string;

  @minLength(1)
  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  key: string;

  transport?: Ores.Rpc.V1.Transport;
  ok: true;

  @minValue(200)
  @maxValue(399)
  status?: int32;

  body?: unknown;

  @minLength(1)
  @maxLength(64)
  traceId?: string;

  @minLength(1)
  @maxLength(32)
  spanId?: string;
};

alias RpcErrorReceipt = {
  v: 1;
  `op`: "receipt";

  @minLength(1)
  @maxLength(128)
  id: string;

  @minLength(1)
  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  key: string;

  transport?: Ores.Rpc.V1.Transport;
  ok: false;

  @minValue(400)
  @maxValue(599)
  status?: int32;

  error: Record<unknown>;

  @minLength(1)
  @maxLength(64)
  traceId?: string;

  @minLength(1)
  @maxLength(32)
  spanId?: string;
};

alias RpcReceiptState = RpcSuccessReceipt | RpcErrorReceipt;
