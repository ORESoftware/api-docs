# Four-language RIDL runtime conformance

The custom RPC stack has two deliberately separate generations:

- **v1 route-map call/receipt envelopes** use `op: call | receipt` and remain
  governed by `idl/typespec/v1.tsp`, the peer JSON Schema/OpenAPI authority, and
  the independently maintained Protobuf ledger.
- **v2 RIDL framed transports** use `t: call | data | end | error | cancel`.
  This document and `runtime/conformance.json` cover that second generation.

A decoder must never accept one generation as the other.

## One byte contract

`ridl/framing.py` is the reference implementation for the RIDL frame envelope.
`examples/frames/conformance.json` is its committed byte-for-byte witness. The
Rust, Dart/Flutter, Go, and TypeScript ports all decode every fixture and must
re-encode the exact same bytes and TCP prefix.

| Runtime | Source | Verification |
| --- | --- | --- |
| Rust | `runtime/rust/frame.rs` | compiled inside `runtime/rust`; unit and fixture tests |
| Dart / Flutter | `runtime/dart/lib/src/frame.dart` | dependency-free analyze and fixture tests |
| Go | `runtime/go/frame.go` | standard-library-only `go vet` and race-enabled tests |
| TypeScript / browser | `runtime/typescript/frame.ts` | Node-free source and fixture/hardening tests |

The shared admission script `scripts/test_rpc_runtime_manifest.py` fails closed
when a language disappears, a source or test is missing, a wire constant drifts,
the TypeScript module acquires Node-only dependencies, the Dart library acquires
`dart:io` or package dependencies, the Go implementation imports outside its
reviewed standard-library set, or the Rust frame module is no longer compiled.

## Required invariants

Every port enforces:

- UTF-8 JSON with compact separators and a fixed top-level member order;
- literal non-ASCII text;
- a maximum JSON payload of 8 MiB before allocation from a TCP length prefix;
- four-byte unsigned big-endian TCP length prefixes;
- strict rejection of unknown members and wrong field types;
- exact distinction between an absent body and a present JSON `null` body;
- addressing fields only on `call` frames;
- bodies on `data` frames and codes on `error` frames;
- deterministic metadata ordering;
- monotonic correlation IDs that are not derived from request contents;
- preservation of an incomplete TCP tail for the next read.

## Effects boundary

The frame modules perform no networking. HTTP already carries method, path,
query, body, and status, so it uses no RIDL frame. WebSocket carries one complete
canonical JSON text frame per message. TCP carries the length-prefixed bytes.
Connection pools, TLS, authentication refresh, deadlines, cancellation policy,
retries, NATS clients, ores-otel adapters, and opto-sync adapters remain in the
application or the existing transport seams.

This conformance slice proves codec interoperability. It does not claim that a
service is deployed, that generated service clients implement streaming, or
that the separate v1 call/receipt envelope has been replaced.
