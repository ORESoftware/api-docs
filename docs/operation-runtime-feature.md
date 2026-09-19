# The `operation-runtime` feature

`ores-api-docs` has two layers that used to be one Cargo feature:

| Feature | Provides | Links |
|---|---|---|
| `operation-runtime` | `OperationContext`, `TypedOperationContext`, `OperationPolicy`, `RpcV1HttpContext`, typed dispatch (`operation_dispatch`), `DispatchError` | `http` (for `HeaderMap`) |
| `axum` (default) | everything above, plus the Axum routers (`axum_router`, `rpc_axum`, `rpc_file_router`, `rpc_shared_operation`) | `axum`, `tower`, `http-body-util` |

`axum` implies `operation-runtime`, so default-feature consumers see no change
and every path already-generated code names still resolves, including
`ores_api_docs::rpc_shared_operation::dispatch_typed_json_operation` and
`ores_api_docs::rpc_axum::RpcV1HttpContext` (both are now re-exports).

A consumer that must not link a server framework -- a serverless function, a
worker, a CLI -- depends on the runtime alone:

```toml
ores-api-docs = { git = "...", rev = "...", default-features = false, features = ["operation-runtime"] }
```

This crate deliberately gains no AWS Lambda (or other provider) dependency.
Provider crates belong in the generated consumer or a small adapter crate.

## Transport is not execution environment

```rust
OperationTransportKind   { Http, Rpc, Event }                    // how the call arrived
ExecutionEnvironmentKind { Server, Lambda, Worker, Cli, Test }   // where it is running
```

There is no `OperationTransportKind::Lambda`. A Lambda behind API Gateway is
`Http` + `Lambda`; the same function invoked directly with an RPC envelope is
`Rpc` + `Lambda`; a queue consumer is `Event` + `Lambda`. Every constructor that
predates `ExecutionEnvironmentKind` defaults to `Server`. Both enums, and
`DispatchError`, are `#[non_exhaustive]`: match them with a wildcard arm that is
a hard error, never a silent default.

`OperationPolicyRequest` and `OperationPolicyOutcome` carry `environment`, so
the shared auth/policy boundary is not blind to it. They are
`#[non_exhaustive]`; only the runtime constructs them.

## One source of truth for trusted ingress

`OperationContext` stores at most one `RpcV1HttpContext`. `trusted_headers()`,
`trusted_ingress()` and the compatibility accessor `rpc_http_context()` are all
views of it, so replacing the trusted headers can no longer leave a stale copy
behind.

"No ingress vouched for anything" is distinct from "the ingress forwarded an
empty header set":

| Constructor | `has_trusted_ingress()` |
|---|---|
| `http(state)`, `rpc_without_ingress(state)`, `event(state)` | `false` |
| `http_with_headers(state, headers)`, `rpc(state, ctx)`, `.with_trusted_headers(h)` | `true` |

A direct Lambda invocation has no header-bearing ingress at all; caller identity
there is the provider's (IAM). A policy can read
`OperationPolicyRequest::has_trusted_ingress` and refuse header-derived identity
on that path.

### Who vouched: `IngressProvenance`

A boolean cannot say *who* vouched for the headers, and API Gateway, a load
balancer, an edge network, an operator-run proxy and a test harness guarantee
different things. `RpcV1HttpContext` therefore carries an
`IngressProvenance { Unspecified, ReverseProxy, LoadBalancer, ApiGateway,
FunctionUrl, Edge, Test }` (`#[non_exhaustive]`), exposed as
`OperationContext::ingress_provenance()` and
`OperationPolicyRequest::ingress_provenance` (`None` exactly when there is no
trusted ingress).

Every constructor that predates the enum (`from_headers`, `http_with_headers`,
`rpc`, `with_trusted_headers`) yields `Unspecified` -- the weakest claim, so
existing servers keep their behaviour and nothing becomes strongly trusted by
accident. New adapters name their ingress with
`RpcV1HttpContext::from_headers_with_provenance` +
`OperationContext::http_from_ingress`. Replacing the headers resets provenance
to `Unspecified`: whoever swapped them in did not say where they came from.

### Debug output is redacted

`{:?}` on `OperationContext` or `RpcV1HttpContext` prints header *names* and
policy-value *keys* only. Header values (authorization, cookies, access JWTs,
request signatures) and policy values (identity claims) are never printed. A
regression test plants sentinel secrets and asserts they do not appear.

### Platform identity: `ProviderIdentity`

Some invocations have no header-bearing ingress at all. A direct AWS Lambda
invocation is authenticated by IAM before the function runs; there is nothing to
read a caller out of. The shared contract for that is a typed,
`#[non_exhaustive]` value:

```rust
ProviderIdentity { provider: IdentityProvider, principal, account, source }
IdentityProvider { AwsIam, GcpIam, Workload, Test }
```

An adapter asserts it with `OperationContext::with_provider_identity`, reading
it from the provider's own request context -- never from a request header or an
envelope field the caller controls. It reaches policies as
`OperationPolicyRequest::provider_identity` and handlers as
`TypedOperationContext::provider_identity()`. It is independent of trusted
ingress: a direct invoke has identity and no ingress; an API Gateway call with
IAM auth can have both. Because it is one contract, no adapter needs an
IAM-specific side channel or closure-captured state. The values are identifiers
(an ARN, an account id), not credentials, so `Debug` prints them.

### Audit: outcome class, and what happens on rejection

`OperationPolicyOutcome` carries what an audit record needs, so a policy does
not have to stash request facts in `before()` and correlate them later:

| Field | Meaning |
|---|---|
| `outcome` | `Success` / `OperationError` / `PolicyRejected` (`#[non_exhaustive]`; treat unknown as failure) |
| `ok` | `outcome == Success`; kept for policies written before `outcome` |
| `provider_identity`, `ingress_provenance` | who called, and who vouched |
| `permit_values` | what this policy's own `before()` admitted; empty on rejection |
| `rejection` | `Some` exactly when `outcome` is `PolicyRejected` |

Lifecycle, decided deliberately:

- `after()` runs after every **admitted** invocation, success or failure. It is
  **not** called when `before()` rejected. A policy that acquires something in
  `before()` (a rate-limit token, an idempotency lease) and releases it in
  `after()` must never see a release without a matching acquire.
- `after_rejection()` runs when `before()` rejected, so denials are auditable on
  the same terms. It defaults to doing nothing -- exactly how every policy
  behaved before the hook existed -- and cannot change the rejection.

## Fallible dispatch and who owns the key set

This crate owns the reusable pieces:

- `dispatch_typed_json_operation_in(base_context, call, invoke)` -- the typed
  per-operation primitive. `dispatch_typed_json_operation(state, http_context, ...)`
  is exactly this with `OperationContext::rpc(state, http_context)`.
- `DispatchError::UnknownOperation { key }`.
- `rpc_receipt_for_dispatch_error` -- the RPC endpoint's rendering (a correlated
  404 receipt).

It does **not** own the closed set of operation keys. `ores-stack` generates
that `match` from the handlers-authoritative operation inventory, with a
wildcard arm returning `DispatchError::UnknownOperation` instead of
`unreachable!`. Nothing in `operation_dispatch` accepts a `RouteMap`: a route
map only knows operations with an HTTP projection, so consulting one would make
route-less operations undispatchable.

`DispatchError` carries no HTTP status. Each adapter maps it for its own carrier
(HTTP 404, an RPC receipt, a structured Lambda invocation error, a failed queue
record under its retry policy).

## What the dependency test does and does not claim

`rust/tests/operation_runtime_feature_isolation.rs` asserts on Cargo's resolved
graph: no `axum*` anywhere under `operation-runtime`; no *direct* edge to
`axum`, `tower`, `http-body-util`, `hyper` or `tokio`; `http` present; the
default build still links the Axum adapter; the featureless build still does
not link `http`.

It does not assert that `tower` and `hyper` are absent transitively. They reach
this crate through `jsonschema -> reqwest` regardless of features, and did
before this feature existed. Removing them means narrowing `jsonschema`'s
features, which changes remote `$ref` resolution and is a separate decision.
