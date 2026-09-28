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
    │   ├── resolver.rs
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

Every GraphQL leaf uses authored `resolver.rs` plus generated/provider-neutral `lambda.rs`. `resolver.rs` declares only GraphQL projection metadata and delegates to the same generated semantic invoker used by RPC/HTTP/Lambda; it is not a second business-logic authority. A single GraphQL request may invoke multiple GraphQL leaves.

Server resolver exposure is separate from client GraphQL documents. `src/graphql/**/resolver.rs` defines server projection inventory, while client-authored `.graphql`/`.gql` documents may be indexed or persisted independently without becoming semantic operation authority.

## No header-selected transport

HTTP path chooses REST vs RPC vs GraphQL. Application-controlled headers do not select the transport. Headers remain available for authentication, tracing, codec/media negotiation, protocol versioning, and GraphQL-specific behavior after the transport has been selected.

## Discovery rules

The filesystem scanner must keep these concerns disjoint:

1. REST Lambda discovery starts at `src/routes/rest/`.
2. RPC HTTP ingress discovery admits the canonical/configured mount under `src/routes/rpc/`.
3. GraphQL HTTP ingress discovery admits the canonical/configured mount under `src/routes/graphql/`.
4. Custom RPC Lambda discovery starts independently at `src/rpc/` and expects `funcs.rs` leaves.
5. GraphQL Lambda discovery starts independently at `src/graphql/` and expects singular `resolver.rs` leaves.
6. REST-derived RPC projections discovered under `src/routes/rest/**` remain attached to their owning REST leaf and join the aggregate RPC operation inventory with explicit origin metadata.

No scanner may reinterpret `src/routes/rpc/**` as the custom RPC tree or `src/routes/graphql/**` as the GraphQL resolver tree.

## Operation identity and provenance

Public operation identity must remain stable across implementation edits, file moves, and transport projection changes.

The normalized registry must distinguish semantic identity from projection identity and provenance/build inputs:

```text
operation_key                   # public stable protocol identity
callable_id                     # stable callable ABI identity
operation_contract_sha256       # transport-neutral semantic operation digest
registry_contract_sha256        # complete normalized registry/catalog digest
policy_identity                 # shared scope/audience identity
type_identity                   # per-section normalized wire-schema digests
origin_kind                     # rest_projection | rpc_leaf
leaf_identity                   # physical dispatch/build unit
source_path                     # provenance/locator
source_sha256                   # provenance/build invalidation
stream_mode
```

`callable_id` must not be a hash of source path or implementation bytes. Source path and SHA are allowed to change while `operation_key`/`callable_id` remain stable. The intended callable identity changes when the callable's stable semantic/public ABI shape changes, including function identity/public callable arity, rather than on source relocation.

`operation_contract_sha256`, `policy_identity`, and `type_identity` are semantic evidence shared by admitted REST/RPC/GraphQL/Lambda projections. A projection may add transport-specific field/path/framing/build identity, but it may not redefine these shared facts. Generated exposure manifests must fail closed when copied evidence is stale or disagrees with the normalized semantic operation contract.

`ORESoftware/api-docs#219` owns the separate transport-leaf ABI/build identity contract used by downstream build hashing.

## GraphQL server exposure manifest

`generated/graphql/server-graphql-index.json` is the deterministic server resolver inventory. Each resolver entry binds the GraphQL-specific `kind`, `field`, `stream`, and resolver provenance to one shared semantic binding containing:

```text
operation_key
callable_id
operation_spec
registry_contract_sha256
operation_contract_sha256
policy_identity
type_identity
```

The outer projection `operation_key` and nested binding `operation_key` must match. Tooling must recompute the semantic contract, policy, and type evidence from the normalized operation inventory before publishing the index. GraphQL is optional: the absence of any resolver leaves is valid and must not create synthetic GraphQL exposure.

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

Client GraphQL document generation/persistence is likewise separate from `server-graphql-index.json`: a client document can reference server fields, but it cannot create a server resolver or semantic operation.

## Identity and manifest requirements

All normalized route/operation/resolver records include transport identity explicitly. The projection/build identity domain is disjoint across:

```text
rest
rpc
graphql
```

Transport participates in deterministic ordering, generated docs, collision checks, Lambda projection metadata, and downstream `ores-stack` build receipts. Transport-specific identity does not participate in the shared semantic operation digest merely because the same semantic callable is exposed through another transport.

Two executable leaves with the same relative spelling in different roots are distinct and must never collide.

## HTTP documentation projection

Generated API documentation shows HTTP ingress separately from semantic Lambda inventory:

- REST: actual methods/paths below `src/routes/rest/**`.
- RPC: canonical `POST /v1/rpc` plus the aggregate typed operation inventory, retaining `rest_projection` vs `rpc_leaf` provenance.
- GraphQL: canonical `POST /v1/graphql` plus schema/resolver inventory from `src/graphql/**/resolver.rs`.

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

Legacy GraphQL plural `resolvers.rs`, GraphQL `funcs.rs`, and route-local `graphql.rs` files migrate to singular `src/graphql/**/resolver.rs` projection leaves.

## Conformance cases

The contract corpus should prove:

- REST leaves are rooted below `src/routes/rest/**`;
- legacy direct REST leaves receive a migration diagnostic;
- `src/routes/rpc/v1` is ingress, not custom-RPC hierarchy;
- `src/routes/graphql/v1` is ingress, not resolver hierarchy;
- route-local REST `rpc.rs` projections remain valid and preserve REST-leaf provenance;
- custom `src/rpc/**/funcs.rs` leaves remain independent Lambda units;
- singular `src/graphql/**/resolver.rs` is the GraphQL authored projection authority;
- plural `src/graphql/**/resolvers.rs` and GraphQL `funcs.rs` are rejected as obsolete authority spellings;
- GraphQL resolver projection cannot create business logic or a semantic operation absent from the normalized registry;
- resolver stream mode must match the shared semantic operation stream mode;
- GraphQL outer `operation_key` must match the nested semantic binding and exact generated invoker;
- policy/type/semantic-contract drift fails closed rather than producing a server index;
- the RPC registry can contain both `rest_projection` and `rpc_leaf` operations without collisions;
- stable operation/callable identity survives source relocation and transport projection changes;
- client codegen can import one subtree without importing every RPC operation;
- client GraphQL documents do not become server resolver authority;
- identical relative executable-leaf names across transports do not collide;
- no custom HTTP header is required to select transport.

## Superseded assumptions

The superseded design is the assumption that RPC or GraphQL semantic hierarchy should be mirrored beneath `src/routes/**` or that GraphQL needs a second business-logic stack.

This does **not** ban route-local generated `rpc.rs` inside REST leaves. Those files remain valid REST-derived RPC projections. Independent custom RPC Lambdas live under `src/rpc/**`; GraphQL projection leaves live under `src/graphql/**` and bind to the shared semantic operation core.

```text
src/routes/rest/**             # REST Lambda leaves; may publish REST-derived RPC operations
src/routes/rpc/v1              # RPC HTTP ingress only
src/routes/graphql/v1          # GraphQL HTTP ingress only
src/rpc/**                     # independent custom-RPC Lambda leaves
src/graphql/**                 # GraphQL Lambda/projection leaves using resolver.rs
```
