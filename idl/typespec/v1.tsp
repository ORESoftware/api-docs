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

// The two receipt states share one JSON shape but remain disjoint by `ok`.
// JSON Schema expresses the same rule with if/then/else. Protobuf is a
// flattened transport projection and cannot encode this conditional alone.
union RpcReceipt {
  success: RpcSuccessReceipt,
  error: RpcErrorReceipt,
}

model RpcSuccessReceipt {
  v: 1;
  `op`: "receipt";

  @minLength(1)
  @maxLength(128)
  id: string;

  @minLength(1)
  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  key: string;

  transport?: Transport;
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
}

model RpcErrorReceipt {
  v: 1;
  `op`: "receipt";

  @minLength(1)
  @maxLength(128)
  id: string;

  @minLength(1)
  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  key: string;

  transport?: Transport;
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
}
