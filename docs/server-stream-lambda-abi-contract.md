# Streaming compatibility invariants

1. Existing unary `lambda::run(state, input) -> RpcV1Receipt` remains unchanged.
2. `server_stream` adds a separate provider-neutral stream entry point; it does not overload or buffer the unary receipt ABI.
3. `OperationSpec::ResponseBody` is one semantic stream item, not a collection and not a provider response object.
4. The authored Rust function returns `OperationServerStream<ResponseBody, Error>` so the compiler ties each item/error to the same operation contract that generates clients.
5. Frames reuse the existing `RpcStreamFrame` protocol and correlation id.
6. Provider/local wrappers own framing and transport. Generated `lambda.rs` owns no listener and no provider `main()`.
7. PPR local execution must write frames incrementally and enforce a hard 90-second child lifetime.
