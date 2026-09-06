// v2 ridl frames. JSON Schema witness: json-schema/rpc-frame.schema.json.
// HTTP does not use this envelope. Discriminated models here vs if/then in
// JSON Schema is an expected generator delta (see idl/expected-deltas.json).

namespace Ores.Rpc.V2;

union RpcFrame {
  call: RpcCallFrame,
  data: RpcDataFrame,
  end: RpcEndFrame,
  error: RpcErrorFrame,
  cancel: RpcCancelFrame,
}

model RpcCallFrame {
  v: 1;
  t: "call";

  @minLength(1)
  @maxLength(128)
  id: string;

  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  key: string;

  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";

  @pattern("^/[^\\s]*$")
  path: string;

  query?: string[][];
  body?: unknown;
  meta?: Record<string>;
}

model RpcDataFrame {
  v: 1;
  t: "data";

  @minLength(1)
  @maxLength(128)
  id: string;

  body: unknown;
  meta?: Record<string>;
}

model RpcEndFrame {
  v: 1;
  t: "end";

  @minLength(1)
  @maxLength(128)
  id: string;

  meta?: Record<string>;
}

model RpcErrorFrame {
  v: 1;
  t: "error";

  @minLength(1)
  @maxLength(128)
  id: string;

  @minLength(1)
  @maxLength(64)
  code: string;

  @maxLength(2048)
  message?: string;

  body?: unknown;
  meta?: Record<string>;
}

model RpcCancelFrame {
  v: 1;
  t: "cancel";

  @minLength(1)
  @maxLength(128)
  id: string;

  meta?: Record<string>;
}
