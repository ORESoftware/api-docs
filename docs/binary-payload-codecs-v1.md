# Binary payload codecs v1

Status: proposed cross-repository transport contract.

This document closes a gap between the RPC operation metadata and the concrete
HTTP/TCP/WebSocket wire. `api-docs` already admits operation codec metadata such
as JSON, Protobuf, and MessagePack. The transport contract below makes those
choices executable without changing semantic operation authority.

TypeSpec and authored JSON Schema/OpenAPI remain independent peer authorities.
Protobuf is a generated projection. A successful Protobuf/MessagePack/CBOR
decode is **not** semantic admission: the decoded value still passes the same
operation/schema validators used by JSON.

## 1. Codec registry

The initial portable registry is:

| wire id | canonical name | canonical HTTP media type | notes |
| ---: | --- | --- | --- |
| `1` | `json` | `application/json` | compatibility baseline; WebSocket text is permitted only for this codec |
| `2` | `messagepack` | `application/msgpack` | binary; `application/x-msgpack` is accepted as an input alias |
| `3` | `cbor` | `application/cbor` | binary |
| `4` | `protobuf` | `application/x-protobuf` | generated projection; `application/protobuf` is accepted as an input alias |
| `5` | `raw` | `application/octet-stream` | opaque bytes when the operation contract declares a byte payload |

Wire ids are append-only. A retired codec id is never reassigned.

Additional schema-oriented codecs such as FlatBuffers, Cap'n Proto, or Avro may
be added later, but they are not aliases for Protobuf and must receive their own
stable names/ids plus conformance vectors.

Compression is not a payload codec. `gzip`, `br`, `zstd`, and similar transforms
belong to content-encoding/transport compression and are negotiated separately.

## 2. One semantic operation, multiple representations

An operation may declare more than one allowed payload codec and exactly one
default. The codec changes representation, not business meaning, validation,
authorization, idempotency, route identity, cache identity, or stream mode.

The operation contract and generated SDK metadata must expose:

- `allowed_codecs`
- `default_codec`
- request and response semantic schema identity
- whether the request/response is unary or streaming
- transport/framing metadata independently from the codec

A generator must not create a codec-specific business handler. All codec paths
land on the same typed operation after decode and semantic validation.

## 3. HTTP

HTTP uses the HTTP message body as its frame boundary.

- Request `Content-Type` selects the request codec.
- `Accept` expresses response codec preference.
- An unsupported request media type fails closed as HTTP `415`.
- If none of the requested response codecs can be produced, fail as HTTP `406`.
- The response `Content-Type` identifies the codec actually emitted.
- Binary bodies are bytes. Do not base64-wrap them merely to pass through a
  JSON-only internal API.
- Body byte ceilings are checked before allocating/decoding an untrusted
  declared payload.

A JSON compatibility endpoint may continue to default to `application/json`
when no explicit negotiation is present. New multi-codec clients should always
send explicit media headers.

## 4. TCP

Framing and serialization are independent.

Legacy v1 JSON peers may continue using NDJSON. NDJSON is JSON-only and must
never carry MessagePack, CBOR, Protobuf, or raw bytes.

Binary TCP v2 uses:

1. four-byte unsigned big-endian payload-frame length;
2. one codec byte from the registry above;
3. opaque payload bytes.

The declared length is validated against the compile-time frame ceiling before
allocation. Unknown codec ids fail closed. The response echoes an allowed codec;
a client must reject an unexpected codec rather than silently interpreting it
as JSON.

Length-prefixing permits embedded NULs, newlines, arbitrary UTF-8 failures, and
all other byte values without escaping.

## 5. WebSocket

WebSocket already supplies message boundaries.

- JSON compatibility may use a text message containing the existing JSON RPC
  envelope.
- Binary RPC uses a WebSocket **binary** message.
- The first byte of a binary application message is the codec id; the remaining
  bytes are the codec payload.
- WebSocket ping/pong/close frames remain transport control frames and are never
  fed into RPC decoding.
- The socket layer only enforces connection/frame policy and preserves bytes.
  `api-docs` owns RPC codec selection, envelope decoding, semantic validation,
  dispatch, and typed response encoding.

The binary path must not infer authorization, route identity, or caller identity
from the connection or request id.

## 6. Protobuf projection boundary

The authored TypeSpec and JSON Schema tracks remain the semantic authorities.
The checked-in Protobuf projection remains append-only and independently
reviewed.

For dynamic JSON-shaped fields in the existing `ores.rpc.v1.RpcCall` /
`RpcReceipt` Protobuf projection, canonical JSON bytes remain the compatibility
representation until the operation has a generated typed Protobuf message for
that field. New generated operation-specific Protobuf types should encode typed
fields natively and adapt to the shared semantic operation input after decode.

Generated Protobuf adapters must:

1. decode without a JSON round trip for fields that have generated Protobuf
   types;
2. reject over-limit input before expensive decode/allocation where possible;
3. map reserved/escaped identifiers back to the exact authored wire name;
4. run the same semantic validator as other codecs;
5. preserve operation id / correlation id checks.

## 7. MessagePack and CBOR

MessagePack and CBOR may use the serde/object-model path when the authored
semantic shape can be represented losslessly. They must not first serialize to
JSON text and then wrap those JSON bytes in the binary codec.

Integer range, map-key, duplicate-key/canonicalization, and binary-vs-text
semantics must be covered by conformance vectors. Where a codec can represent a
value the shared semantic contract cannot, semantic validation wins and the
value is rejected.

## 8. Raw bytes

`raw` is only legal when the operation contract explicitly declares a binary
request or response representation. It is appropriate for files, encrypted
records, compressed blocks, audio chunks, images, and other byte sequences.

`raw` must never be used as an escape hatch around typed validation for an
operation that declares a structured semantic payload.

## 9. Streaming

Streaming mode and payload codec are separate axes.

- WebSocket: one application message is one stream record unless a higher-level
  stream contract explicitly says otherwise.
- TCP: one length-delimited frame is one stream record.
- HTTP: unary uses one body; streaming uses the operation's declared HTTP
  framing (for example length-delimited records or raw byte chunks).
- NDJSON/JSON-seq remain JSON-specific stream framings.

A streamed record carries the same codec semantics as a unary payload. A codec
change mid-stream is illegal unless the stream protocol explicitly negotiates
it.

## 10. Conformance requirements

Every supported codec/transport pair needs fixtures proving the same semantic
operation result for equivalent values.

Minimum vectors:

- JSON, MessagePack, CBOR semantic round-trip;
- generated Protobuf round-trip for at least one operation-specific typed
  request and response;
- raw bytes containing NUL, `0xff`, invalid UTF-8, and newline bytes;
- HTTP `Content-Type` / `Accept` negotiation and 415/406 failures;
- TCP binary length ceiling, unknown codec id, partial frame, and multiple
  frames in one read;
- WebSocket text JSON plus binary MessagePack/CBOR/Protobuf, with byte-for-byte
  preservation by the socket layer;
- malformed payloads fail before handler dispatch;
- decoded values still fail when the semantic schema rejects them;
- request/reply correlation remains enforced on stateful transports;
- compression, when enabled, is applied outside codec encoding and bounded
  against decompression bombs.

Fixtures should be generated from the same semantic input and compared by
semantic value, not by expecting JSON, MessagePack, CBOR, and Protobuf to have
identical bytes.

## 11. Cross-repository ownership

- `ORESoftware/api-docs`: codec registry semantics, operation metadata,
  documentation/client generation, codec-aware RPC decode/encode, conformance
  authority.
- `ORESoftware/ores-transport`: bounded HTTP/TCP/broker byte transport and
  shared codec/framing primitives.
- `ORESoftware/ores-websocket`: WebSocket upgrade/session plumbing and bounded
  byte-preserving binary message pass-through only.
- `ORESoftware/typespec-json-schema-validator`: validates peer-authority
  semantic parity and representation metadata without making Protobuf or any
  runtime codec authoritative.
- `ORESoftware/ores-stack` and product CLIs: surface generation/check commands,
  capability diagnostics, and conformance execution; they do not redefine the
  codec contract.

Product `*-interfaces` repositories declare the operation semantics and consume
these shared contracts. Product web/API/daemon runtimes should depend on shared
implementations rather than cloning codec logic per org.
