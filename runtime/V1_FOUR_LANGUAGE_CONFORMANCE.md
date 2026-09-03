# RPC v1 four-language conformance

The v1 compatibility envelope is executable in Rust, Dart/Flutter, Go, and
browser-safe TypeScript. It is deliberately distinct from the RIDL v2 streaming
frame generation:

| Generation | Discriminator | Variants |
|---|---|---|
| RPC v1 | `op` | `call`, `receipt` |
| RIDL v2 | `t` | `call`, `data`, `end`, `error`, `cancel` |

A v1 decoder must reject a v2 frame and a v2 decoder must reject a v1 envelope.
The common byte corpus is `examples/rpc-v1/conformance.json`; the closed source
and test inventory is `runtime/v1-conformance.json`.

## Peer contract authorities

TypeSpec (`idl/typespec/v1.tsp`) and JSON Schema Draft 2020-12
(`json-schema/rpc-call.schema.json` and `rpc-receipt.schema.json`) remain
independently authored, top-level peers. Neither is generated from or allowed to
overwrite the other. Protobuf is the stable binary/field-number projection and
keeps the append-only ledger in `idl/protobuf.lock.json`.

The four runtimes implement the intersection admitted by the peer-authority
audit. A discrepancy stops promotion; no runtime or generator becomes the
winner by being first or easiest to execute.

## Receipt state machine

For compatibility with the existing v1 authorities, `status` remains optional.
When present, it is interpreted identically on HTTP, TCP, WebSocket, and NATS:

| `ok` | `status` | `body` | `error` |
|---|---|---|---|
| `true` | absent or `200..399` | optional, including explicit JSON `null` | forbidden |
| `false` | absent or `400..599` | forbidden | required JSON object |

An absent body and an explicitly present JSON `null` body are different wire
states and must survive every decode/encode round trip.

## Framing and limits

HTTP uses native HTTP method/path/query/body primitives. WebSocket carries one
UTF-8 JSON envelope per message. TCP may use one envelope per NDJSON line or a
four-byte big-endian unsigned payload length followed by the same canonical
UTF-8 JSON bytes. The payload limit is 8 MiB and must be checked before an
allocation based on an untrusted declared length.

Connection management, TLS, authentication refresh, deadlines, cancellation,
retry policy, `ores-otel`, `opto-sync`, and concrete NATS clients stay outside
this codec. Those layers may carry the envelope but must not redefine it.
