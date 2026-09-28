# Server-stream Lambda ABI

`server_stream` operations keep the same contract item type (`OperationSpec::ResponseBody`) while authored Rust handlers return `ServerStreamResult<OperationSpec>`. `OperationSpec::STREAM`, macro stream metadata, and the Rust return shape are compile-time-linked so drift fails during `cargo check`.

Generated provider-neutral API Lambda modules expose an additive streaming entry point alongside the existing unary `run(...)` ABI. Semantic RPC stream output uses the shared `RpcStreamFrame` contract (`data`, `end`, `error`, `cancel`) rather than introducing a provider-specific RPC protocol.

RPC stream framing and HTTP/Lambda HTTP-projection framing are separate contracts. A `server_stream` operation does **not** by itself imply NDJSON, SSE, HTTP/1.1 chunked transfer encoding, length-delimited binary records, or raw chunks. The HTTP projection must select an admitted framing explicitly from normalized route/projection metadata. Local PPR and provider wrappers may then map the same semantic RPC item stream to that declared HTTP framing without buffering the stream into a unary response.

Legacy PPR implementations that currently render RPC frames as newline-delimited JSON are a compatibility lane, not the semantic default for all server streams. Transport-level HTTP chunking is likewise an implementation detail chosen by the HTTP stack/protocol and must not be treated as the application framing contract.

Generated `lambda.rs` never owns a listener or provider `main()`. The local PPR host enforces a hard 90-second child lifetime and must kill/reap a child when that deadline expires or the downstream connection is lost. The stable supervisor remains the owner of `ores-middleware` admission/finalization.

The typed HTTP/Lambda framing matrix, bounds, cancellation/backpressure behavior, media-type compatibility, and deterministic projection digest are tracked by the response-framing contract work. Until an explicit projection framing is admitted, generators and adapters must fail closed rather than infer framing from `server_stream` alone.
