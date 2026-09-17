# Typed RPC SDK v2 invariants

`rpc_client_bundle_v2` consumes two views of the same reviewed operation contract:

- the audience-filtered route map for transport/projection metadata; and
- the handlers-authoritative normalized `RpcOperationContract` IR for operation identity and semantic schemas.

Generated public method names come from `source.operation`, which is the authored `handlers.rs` operation function. The dotted operation key remains the stable wire identity.

All generated RPC facades use canonical `POST /v1/rpc`. An HTTP projection such as `GET /v1/version` is compatibility metadata and is never used as the RPC SDK transport endpoint.

A semantic request/response/error slot with a real schema must generate a concrete language type. Unsupported JSON Schema composition, references, or open untyped objects fail generation instead of degrading the public SDK to `unknown`, `any`, `serde_json::Value`, `Object?`, or `dynamic.Dynamic`.

`NoSection` is represented by an absent normalized schema. `ores-stack` is responsible for checking the authored/generated `OperationSpec` section identity against the normalized IR before invoking client generation.