# Lambda/RPC five-axis hardening contract

The callable response contract is five independent axes. No generator, adapter, runtime, documentation surface, or client may infer one axis from another.

1. **Semantic response type** — `OperationSpec::ResponseBody` (and typed `Error`). For a server stream, `ResponseBody` is one emitted item.
2. **Semantic representation** — structured, HTML, text, or binary.
3. **RPC payload codec** — JSON, MessagePack, or Protobuf. The codec serializes the typed RPC value; it does not redefine its semantics.
4. **RPC invocation shape** — unary, server stream, client stream, or bidirectional stream.
5. **HTTP/Lambda HTTP-projection framing** — single body, SSE, NDJSON, JSON-seq, length-delimited records, or raw byte chunks.

Axes 4 and 5 are especially easy to collapse incorrectly. A `server_stream` operation called through typed RPC uses RPC/RIDL stream frames. An HTTP projection of the same semantic operation may use SSE, NDJSON/JSON-seq, length-delimited records, or raw chunks. Conversely, HTTP transfer mechanics are not permission to change a unary semantic operation into an RPC stream.

## Authority and ownership

TypeSpec and independently authored Draft 2020-12 JSON Schema remain peer authorities for the contract shapes already governed by those tracks. `#[ores_operation(spec = ...)]` binds authored Rust behavior to the generated `OperationSpec`; it must not duplicate response type, representation, codec, or stream metadata as unrelated handler-local strings.

HTTP-specific response framing belongs to the HTTP projection (`#[ores_route]` / normalized HTTP route IR), not to the transport-neutral semantic operation. Direct Lambda RPC invocation consumes the RPC contract and RPC/RIDL framing. Lambda HTTP/page adapters consume the HTTP projection. They must not share a fake one-size-fits-all response ABI.

## Fail-closed compatibility matrix

The Rust response-contract layer exposes `HttpResponseFraming` and `validate_http_response_framing(...)` as the common admission primitive.

| RPC stream mode | HTTP projection framing | Admission |
| --- | --- | --- |
| unary | single | allowed |
| unary | SSE / NDJSON / JSON-seq / length-delimited / raw chunks | rejected |
| server_stream | single | rejected; never buffer a declared stream |
| server_stream | explicit streaming framing compatible with representation/media | allowed |
| client_stream | any HTTP response framing | rejected until a canonical HTTP ABI exists |
| bidi | any HTTP response framing | rejected until a canonical HTTP ABI exists |

Additional media invariants are fail-closed:

- SSE requires `text/event-stream` and a text-compatible representation.
- NDJSON requires structured items and an NDJSON media type.
- JSON-seq requires structured items and `application/json-seq`.
- Length-delimited framing rejects HTML/text representations and requires an explicit non-text media type.
- Raw chunks require binary representation and an explicit non-text media type.
- Binary response representation rejects text and JSON media types.
- HTML/text/binary representations require a string-shaped JSON Schema body when a top-level `type` is declared.
- Malformed media types are rejected rather than normalized optimistically.

## Current implementation boundary

The response-representation foundation landed before this hardening slice. This slice adds the fifth-axis type and compatibility validator plus negative tests. It does **not** claim the entire transport stack is complete.

The remaining promotion gates are:

1. carry HTTP response framing in the normalized HTTP projection IR and `#[ores_route]`-derived metadata;
2. make OpenAPI/docs/client/lambda HTTP adapters consume the normalized framing instead of inventing local defaults;
3. implement actual JSON/MessagePack/Protobuf negotiation and encode/decode in unary and server-stream RPC runtimes — advertising a codec is not sufficient;
4. keep generated Rust/TypeScript/Dart/Go/Gleam client signatures typed for every supported codec and stream shape;
5. add deterministic cross-language golden vectors for unary receipts and stream frames for each admitted codec;
6. make `ores-stack` provider adapters consume the normalized contract while preserving direct-RPC versus HTTP/page carrier separation;
7. keep client-stream and bidi fail-closed until a canonical authored Rust ABI, generated-client ABI, and transport implementation exist.

No downstream runtime should advertise generic multi-codec or streaming Lambda support until the relevant gates above have exact-head evidence.