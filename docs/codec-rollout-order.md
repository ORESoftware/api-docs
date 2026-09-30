# Staged RPC codec rollout and rollback order

This document defines the safe deployment order for adding non-JSON RPC payload codecs without creating a window where metadata, generated clients, runtime binaries, or documentation overstate executable support.

## Required rollout order

1. **Admit metadata first.** Land the reviewed codec registry and operation-level `allowed_codecs` / `default_codec` contract. This means the contract system can describe a codec; it does not mean a deployed runtime can execute it.
2. **Generate client/catalog surfaces.** Regenerate typed clients and codec-selection metadata from the admitted operation contract. Client generation must retain operation-specific allowed/default codecs and must not silently widen them from a global catalog.
3. **Land runtime encode/decode and negotiation.** Server/runtime code must implement the codec, request admission, response negotiation, size ceilings, full-input decode, and response-media verification before any deployment claims support.
4. **Adopt the shared runtime in product servers.** Product servers consume the reviewed shared runtime revision rather than copying serializers or declaring support locally.
5. **Enable non-JSON client defaults only after deployment evidence.** A generated client may expose an admitted codec before a concrete target supports it, but it must not default to that codec until the target capability manifest proves executable support for the exact deployment/build.
6. **Publish deployment-specific docs last.** Concrete deployment docs advertise the intersection of operation-admitted codecs and exact runtime/deployment capabilities, never the union.

## Rollback and downgrade rules

- An older JSON-only runtime receiving an explicit MessagePack, CBOR, Protobuf, or raw request must reject it deterministically; it must never reinterpret those bytes as JSON.
- A newer client must not silently retry an explicitly selected new codec as JSON unless the caller chose a reviewed fallback policy.
- An old JSON client continues to work against a newer runtime only when JSON remains admitted for the operation and executable on that deployment.
- Before changing an operation's default codec, deployment admission must prove the target capability set includes that default. A default unsupported by the target is a deployment error, not a runtime fallback opportunity.
- Rolling a runtime back must not leave generated docs or target capability manifests claiming support the rolled-back binary no longer has. Capability identity participates in deployment/build evidence and cache invalidation.

## Authority and evidence boundaries

TypeSpec and independently authored Draft 2020-12 JSON Schema remain peer semantic authorities. The codec catalog, generated clients, runtime capability manifest, deployment receipt, and documentation are downstream evidence/projections.

A codec can therefore be in one of three materially different states:

- **admitted by operation contract** — semantically permitted;
- **implemented by a runtime build** — executable by that exact binary/target;
- **enabled/defaulted for a deployment/client** — safe to select for that exact deployment.

Promotion must preserve those distinctions. A passed metadata/generator check cannot substitute for executable runtime evidence, and a runtime implementation cannot widen an operation's admitted codec set.

## Minimum rollback conformance matrix

| Client | Runtime | Selection | Expected result |
| --- | --- | --- | --- |
| old JSON client | new runtime | JSON | succeeds when JSON remains admitted/executable |
| new client | old JSON-only runtime | explicit CBOR/MessagePack/Protobuf | deterministic unsupported-media/capability failure |
| new client | rolled-back runtime | newer preferred codec | failure unless caller explicitly allowed a reviewed fallback |
| new client | new runtime lacking operation admission | globally supported codec | client/runtime must reject operation-local selection |
| new client | new runtime | target-supported admitted default | succeeds with response media verified before decode |

Every release/rollback proof should bind the exact client generator revision, client artifact revision, runtime build revision, operation-contract digest, and target capability digest. Skipped or zero-step CI is not rollout evidence.
