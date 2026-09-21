# Server-stream Lambda ABI

`server_stream` operations keep the same contract item type (`OperationSpec::ResponseBody`) while authored Rust handlers return `ServerStreamResult<OperationSpec>`. `OperationSpec::STREAM`, macro stream metadata, and the Rust return shape are compile-time-linked so drift fails during `cargo check`.

Generated provider-neutral API Lambda modules expose an additive streaming entry point alongside the existing unary `run(...)` ABI. Stream output uses the existing RPC stream frame contract (`data`, `end`, `error`, `cancel`) rather than introducing a provider-specific protocol.

Local PPR adapters serialize those frames incrementally as newline-delimited JSON over HTTP chunked transfer encoding. Provider wrappers may map the same frame stream to native streaming response mechanisms. Generated `lambda.rs` never owns a listener or provider `main()`.

The local PPR host enforces a hard 90-second child lifetime and must kill/reap a child when that deadline expires or the downstream connection is lost. The stable supervisor remains the owner of `ores-middleware` admission/finalization.
