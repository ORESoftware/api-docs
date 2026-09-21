# Transport-namespaced API filesystem contract

Tracking: #217 and `ORESoftware/ores-stack#145`.

This document defines the canonical `*-api-server.rs` source grammar for REST, RPC, and GraphQL routing.

## Canonical source tree

```text
src/
├── routes/
│   ├── rest/
│   │   └── ...                     # REST leaves; each leaf = one Lambda
│   ├── rpc/
│   │   └── v1/
│   │       └── route.rs            # RPC HTTP ingress/mount only
│   └── graphql/
│       └── v1/
│           └── route.rs            # GraphQL HTTP ingress/mount only
├── rpc/
│   ├── get_user/
│   │   ├── funcs.rs
│   │   ├── lambda.rs
│   │   └── tmp/
│   │       └── ...
│   └── ...
└── graphql/
    ├── query_user/
    │   ├── resolver.rs
    │   ├── lambda.rs
    │   └── tmp/
    │       └── ...
    └── ...
```

## Authority split

The three namespaces under `src/routes/` are HTTP routing namespaces. They do not imply that all semantic work is REST-shaped.

### REST

`src/routes/rest/**` is the only ordinary REST filesystem route tree.

A REST leaf maps HTTP method/path projection to one REST Lambda function/build unit. Direct REST leaves under legacy `src/routes/**` are migration inputs, not the new canonical grammar.

### RPC ingress

`src/routes/rpc/v1` defines the HTTP ingress/mount for the canonical RPC transport (normally `/v1/rpc`).

It does not define the RPC operation hierarchy. After transport admission, the typed RPC operation identity selects a leaf under `src/rpc/**`.

The following pattern is explicitly non-canonical when it merely mirrors RPC semantic names:

```text
src/routes/rpc/v1/users/get_user
```

RPC operation names are not REST URL segments.

### RPC Lambda leaves

`src/rpc/**` defines the RPC Lambda topology. Every admitted leaf folder is one independently buildable/invokable Lambda function.

A leaf owns its RPC implementation source, generated/provider-neutral Lambda projection where applicable, and its ignored leaf-local `tmp/` build material.

The operation index must map operation identity to this leaf topology without reconstructing that identity from the HTTP route tree.

### GraphQL ingress

`src/routes/graphql/v1` defines the HTTP ingress/mount for GraphQL (normally `/v1/graphql`).

It parses/validates a GraphQL request and produces an execution plan. The GraphQL field/resolver hierarchy is not projected into HTTP route folders.

### GraphQL Lambda leaves

`src/graphql/**` defines the GraphQL Lambda topology. Every admitted leaf folder is one independently buildable/invokable GraphQL resolver/function Lambda.

A single GraphQL HTTP request may execute more than one GraphQL Lambda leaf.

## No header-selected transport

The HTTP path selects the transport namespace. A request does not become RPC or GraphQL because of a custom routing header.

Headers remain available for transport-local concerns such as authentication, tracing, codec/media negotiation, version metadata, and GraphQL protocol behavior after the path has selected the transport.

## Discovery rules

The filesystem scanner should apply disjoint rules:

1. REST discovery starts at `src/routes/rest/`.
2. RPC HTTP ingress discovery admits the configured/canonical mount under `src/routes/rpc/`.
3. GraphQL HTTP ingress discovery admits the configured/canonical mount under `src/routes/graphql/`.
4. RPC operation/Lambda discovery starts independently at `src/rpc/`.
5. GraphQL resolver/Lambda discovery starts independently at `src/graphql/`.

No scanner may recursively reinterpret `src/routes/rpc/**` as the RPC operation tree or `src/routes/graphql/**` as the GraphQL resolver tree.

## Identity and manifest requirements

All normalized route/operation/resolver records must include transport identity explicitly rather than relying on ambiguous path context.

At minimum, the normalized identity domain is disjoint across:

```text
rest
rpc
graphql
```

This transport discriminator participates in deterministic ordering, semantic digests, generated docs, collision checks, Lambda projection metadata, and downstream `ores-stack` build receipts.

Two leaves with the same relative spelling in different transport trees are distinct and must never collide.

## HTTP documentation projection

Generated API documentation should show transport ingress separately from semantic Lambda inventory:

- REST documents the actual REST methods/paths discovered below `src/routes/rest/**`.
- RPC documents canonical HTTP ingress (for example `POST /v1/rpc`) plus the typed RPC operation inventory from `src/rpc/**`.
- GraphQL documents canonical HTTP ingress (for example `POST /v1/graphql`, plus subscription transport when enabled) plus the GraphQL schema/resolver inventory from `src/graphql/**`.

Do not fabricate REST paths for route-less RPC operations or GraphQL fields merely to place them in a route map.

## Lambda/build-unit boundary

Every admitted REST, RPC, and GraphQL leaf is one Lambda function/build unit.

The `tmp/` directory is leaf-local and ignored. It may contain generated build adapter material such as:

```text
tmp/
├── main.rs
├── Cargo.toml
├── Cargo.lock
├── target/
└── main.bin
```

The transport ingress route and the semantic leaf are distinct authorities for RPC and GraphQL. In particular, `/v1/rpc` is not itself the identity of every RPC Lambda, and `/v1/graphql` is not itself the identity of every GraphQL Lambda.

## Migration and compatibility diagnostics

Legacy REST layouts may have authored leaves directly under `src/routes/**`. During migration, tooling should report a targeted diagnostic instructing the author to move REST route topology under `src/routes/rest/**`.

The compatibility layer must never guess that `src/routes/rpc/**` or `src/routes/graphql/**` is a legacy REST tree. Those namespaces are reserved for RPC and GraphQL HTTP ingress.

Migration does **not** move:

```text
src/rpc/**     -> src/routes/rpc/**
src/graphql/** -> src/routes/graphql/**
```

Those remain independent semantic/Lambda topologies.

## Conformance cases

The contract test corpus should prove:

- a REST leaf below `src/routes/rest/**` receives the expected HTTP method/path;
- a legacy direct REST leaf below `src/routes/**` receives a migration diagnostic;
- `src/routes/rpc/v1` is an RPC ingress and never a REST operation hierarchy;
- RPC operation identity maps to an admitted `src/rpc/**` Lambda leaf;
- `src/routes/graphql/v1` is a GraphQL ingress and never a REST resolver hierarchy;
- GraphQL fields/resolvers map to admitted `src/graphql/**` Lambda leaves;
- one GraphQL request may reference multiple GraphQL leaves;
- identical relative leaf spellings across transports do not collide;
- route manifests and generated docs preserve explicit transport identity;
- no custom HTTP header is needed to select REST vs RPC vs GraphQL.

## Superseded route-local projection model

Earlier drafts placed `rpc.rs` and `graphql.rs` beside ordinary REST route handlers and treated that route-local folder as the projection home for all transports. The canonical model now separates HTTP routing from RPC/GraphQL Lambda topology:

```text
src/routes/{rest,rpc,graphql}  # HTTP routing namespaces
src/rpc/**                     # RPC Lambda leaves
src/graphql/**                 # GraphQL Lambda leaves
```

Future macro and generator work must preserve this distinction.
