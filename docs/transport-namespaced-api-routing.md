# Transport-namespaced API filesystem contract

Tracking: #217 and `ORESoftware/ores-stack#142`.

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
    │   ├── resolvers.rs
    │   ├── lambda.rs
    │   └── tmp/
    │       └── ...
    └── ...
```

The executable Lambda discovery roots are:

```text
src/routes/rest
src/rpc
src/graphql
```

`src/routes/rpc/v1` and `src/routes/graphql/v1` define HTTP ingress/mounts; they do not recursively define RPC-operation or GraphQL-resolver Lambda topology.

## REST route leaves and RPC projection

`src/routes/rest/**` is the ordinary REST filesystem route tree. A REST leaf may contain:

```text
handlers.rs      # semantic operation authority when used
route.rs         # authored REST/HTTP projection
rpc.rs           # generated REST-derived RPC publication/projection
lambda.rs        # provider-neutral REST Lambda projection
```

A route-local `rpc.rs` remains a generated projection of the owning REST leaf; it is not an independent custom-RPC Lambda leaf. Every eligible operation in `handlers.rs` is REST-RPC published by default unless explicitly marked `#[ores_no_rpc]`.

Therefore canonical `/v1/rpc` aggregates operations from two sources:

```text
rest_projection -> operation projected from src/routes/rest/**/handlers.rs
rpc_leaf        -> custom operation implemented by src/rpc/**/funcs.rs
```

The normalized operation inventory must preserve that origin instead of reconstructing it from filesystem spelling.

## RPC ingress

`src/routes/rpc/v1` defines the canonical HTTP ingress/mount for RPC, normally `POST /v1/rpc`.

It does not define the RPC semantic hierarchy. A path such as:

```text
src/routes/rpc/v1/users/get_user
```

is non-canonical when it merely mirrors the operation name `users.get_user`.

After envelope admission, the stable operation key is resolved through the aggregate RPC registry. The selected operation can dispatch either to its owning REST Lambda leaf or to an independent `src/rpc/**` Lambda leaf according to recorded provenance.

## Independent custom-RPC Lambda leaves

`src/rpc/**` defines custom RPC Lambda topology. Every admitted leaf is one independently buildable/invokable Lambda and uses authored `funcs.rs` plus generated/provider-neutral `lambda.rs`.

Custom RPC functions are human-authored and must carry the explicit RPC/operation attributes required by the contract. They share the canonical `/v1/rpc` protocol with REST-derived RPC projections, but their physical build/deployment origin is distinct.

## GraphQL ingress and leaves

`src/routes/graphql/v1` defines the canonical GraphQL HTTP ingress/mount, normally `POST /v1/graphql`.

The GraphQL field/resolver hierarchy is not mirrored under `src/routes/graphql/**`. GraphQL execution selects independent leaves under `src/graphql/**`.

Every GraphQL leaf uses authored `resolvers.rs` plus generated/provider-neutral `lambda.rs`. `resolvers.rs` is intentionally distinct from RPC `funcs.rs` and REST `handlers.rs`. A single GraphQL request may invoke multiple GraphQL leaves.

## No header-selected transport

HTTP path chooses REST vs RPC vs GraphQL. Application-controlled headers do not select the transport. Headers remain available for authentication, tracing, codec/media negotiation, protocol versioning, and GraphQL-specific behavior after the transport has been selected.

## Discovery rules

The filesystem scanner must keep these concerns disjoint:

1. REST Lambda discovery starts at `src/routes/rest/`.
2. RPC HTTP ingress discovery admits the canonical/configured mount under `src/routes/rpc/`.
3. GraphQL HTTP ingress discovery admits the canonical/configured mount under `src/routes/graphql/`.
4. Custom RPC Lambda discovery starts independently at `src/rpc/` and expects `funcs.rs` leaves.
5. GraphQL Lambda discovery starts independently at `src/graphql/` and expects `resolvers.rs` leaves.
6. REST-derived RPC projections discovered under `src/routes/rest/**` remain attached to their owning REST leaf and join the aggregate RPC operation inventory with explicit origin metadata.

No scanner may reinterpret `src/routes/rpc/**` as the custom RPC tree or `src/routes/graphql/**` as the GraphQL resolver tree.

## Operation identity and provenance

Public RPC call identity must remain stable across implementation edits and file moves.

The normalized registry must distinguish stable identity from provenance/build inputs:

```text
operation_key        # public stable protocol identity
callable_id          # stable callable ABI identity
origin_kind          # rest_projection | rpc_leaf
leaf_identity        # physical dispatch/build unit
source_path          # provenance/locator
source_sha256        # provenance/build invalidation
stream_mode
```

`callable_id` must not be a hash of source path or implementation bytes. Source path and SHA are allowed to change while `operation_key`/`callable_id` remain stable. The intended callable identity changes when the callable's stable semantic/public ABI shape changes, including function identity/public callable arity, rather than on source relocation.

`ORESoftware/api-docs#219` owns the transport-leaf ABI identity contract used by downstream build hashing.

## Client generation and narrow imports

Generated client RPC functions must route to the same stable operation registry regardless of whether the server operation originated as a REST-derived projection or a custom RPC leaf.

Generated packages must also support narrow imports. Importing one callable or namespace must not eagerly import the entire generated RPC tree.

Conceptually:

```text
users/get_user entrypoint -> only users/get_user subtree
billing entrypoint        -> only billing subtree
root ergonomic facade     -> may provide dotted lookup without forcing eager whole-tree imports
```

Codegen manifests therefore need enough namespace/provenance metadata to generate deterministic subtree entrypoints and indexes without conflating server filesystem paths with public client import paths.

## Identity and manifest requirements

All normalized route/operation/resolver records include transport identity explicitly. The identity domain is disjoint across:

```text
rest
rpc
graphql
```

Transport participates in deterministic ordering, semantic digests, generated docs, collision checks, Lambda projection metadata, and downstream `ores-stack` build receipts.

Two executable leaves with the same relative spelling in different roots are distinct and must never collide.

## HTTP documentation projection

Generated API documentation shows HTTP ingress separately from semantic Lambda inventory:

- REST: actual methods/paths below `src/routes/rest/**`.
- RPC: canonical `POST /v1/rpc` plus the aggregate typed operation inventory, retaining `rest_projection` vs `rpc_leaf` provenance.
- GraphQL: canonical `POST /v1/graphql` plus schema/resolver inventory from `src/graphql/**/resolvers.rs`.

Do not fabricate REST paths for custom RPC operations or GraphQL fields merely to place them in a REST route map.

## Lambda/build-unit boundary

Every admitted REST leaf, custom RPC leaf, and GraphQL leaf is one Lambda build unit with leaf-local ignored `tmp/` material such as:

```text
tmp/
├── main.rs
├── Cargo.toml
├── Cargo.lock
├── target/
└── main.bin
```

RPC and GraphQL HTTP ingress routes are distinct from these semantic/executable leaf inventories.

## Migration and compatibility diagnostics

Legacy REST leaves directly under `src/routes/**` migrate to `src/routes/rest/**`.

Tooling should emit a targeted migration diagnostic rather than silently treating `rpc` or `graphql` as ordinary legacy REST route segments.

Migration does not move:

```text
src/rpc/**     -> src/routes/rpc/**
src/graphql/** -> src/routes/graphql/**
```

Legacy GraphQL `resolver.rs`, `funcs.rs`, and route-local `graphql.rs` files migrate to `src/graphql/**/resolvers.rs`.

## Conformance cases

The contract corpus should prove:

- REST leaves are rooted below `src/routes/rest/**`;
- legacy direct REST leaves receive a migration diagnostic;
- `src/routes/rpc/v1` is ingress, not custom-RPC hierarchy;
- `src/routes/graphql/v1` is ingress, not resolver hierarchy;
- route-local REST `rpc.rs` projections remain valid and preserve REST-leaf provenance;
- custom `src/rpc/**/funcs.rs` leaves remain independent Lambda units;
- plural `src/graphql/**/resolvers.rs` is the GraphQL authored leaf authority;
- `src/graphql/**/resolver.rs` and GraphQL `funcs.rs` are rejected as obsolete authority spellings;
- the RPC registry can contain both `rest_projection` and `rpc_leaf` operations without collisions;
- stable operation/callable identity survives source relocation;
- client codegen can import one subtree without importing every RPC operation;
- identical relative executable-leaf names across transports do not collide;
- no custom HTTP header is required to select transport.

## Superseded assumptions

The superseded design is the assumption that RPC or GraphQL semantic hierarchy should be mirrored beneath `src/routes/**`.

This does **not** ban route-local generated `rpc.rs` inside REST leaves. Those files remain valid REST-derived RPC projections. Independent custom RPC Lambdas live under `src/rpc/**`; independent GraphQL Lambdas live under `src/graphql/**`.

```text
src/routes/rest/**             # REST Lambda leaves; may publish REST-derived RPC operations
src/routes/rpc/v1              # RPC HTTP ingress only
src/routes/graphql/v1          # GraphQL HTTP ingress only
src/rpc/**                     # independent custom-RPC Lambda leaves
src/graphql/**                 # independent GraphQL Lambda leaves using resolvers.rs
```
