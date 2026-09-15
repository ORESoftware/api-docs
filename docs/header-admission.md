# Declared-only request header admission

`api-docs` treats request headers as typed validation inputs, never routing selectors. Operation identity remains HTTP method plus path template.

## Accepted names

For RIDL v2, the keys of an operation's `header_params` object are its accepted application header names. Generated clients expose those declarations as typed inputs. For v1 maps, the properties of `header_schema` are the accepted application header names.

An operation that declares no application headers has an empty application-header view.

Header names must be canonical lower-case HTTP field names. Authentication, cookies, content framing, forwarding, tracing, hop-by-hop fields, `x-forwarded-*`, `x-real-ip`, and `grpc-*` are runtime-owned and may not be claimed by a business operation.

## Server boundary

Servers must not hand the raw transport header map to an RPC/business handler. The order is:

1. HTTP/proxy/auth/CORS/tracing/framing middleware reads and validates the raw transport headers it owns.
2. The selected operation is determined only from method + path.
3. The route's declared header contract is compiled into a `HeaderAdmission` policy.
4. Required declared headers are checked.
5. A fresh application header map is constructed containing only declared names; undeclared headers are omitted.
6. Typed header values, path params, query params, and payload are validated before the handler runs.

The Rust crate exposes `HeaderAdmission::from_route` and, with the `axum` feature, `HeaderAdmission::project` for this boundary. Projection does not mutate the raw map, so `authorization`, `content-type`, trace context, and other runtime-owned values remain available to their owning middleware while being absent from the business-handler view.

## Cross-language rule

The normal generated RPC client/server surface should make undeclared business headers unrepresentable. Escape hatches that expose an arbitrary raw header map must stay below the transport/security boundary and must not be part of the typed business RPC API.

The shared manifest shape lives in `ORESoftware/ores-interfaces/contracts/http-header-policy/v1`. A per-service manifest is a projection of the route inventory, not a third authored contract authority. TypeSpec and JSON Schema/OpenAPI remain independent peer authorities and TJSV remains the fail-closed parity gate.
