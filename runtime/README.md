# ridl runtime

Vendorable modules that sit between generated clients and the network. None of
them is a package: copy the ones you need next to the file `ridl generate`
produced, so they compile against *your* service's operations rather than a
lowest-common-denominator copy.

```
runtime/
  rust/
    frame.rs        the wire envelope for websocket + tcp
    transport.rs    RpcTransport over http | websocket | tcp
    telemetry.rs    the ores-otel seam
    opto_sync.rs    the opto-sync seam
  typescript/
    frame.ts        same envelope, same bytes
    transport.ts    same three carriers
    telemetry.ts    the ores-otel seam
    opto-sync.ts    the opto-sync seam
```

## One request, three carriers

A generated call builds an `RpcRequest` carrying the operation key, the method,
the substituted path, the *unsubstituted* template, the query pairs and the JSON
body. That is everything all three carriers need, so **the transport is a choice
at the edge** — the same generated client works over any of them with no
regeneration.

| Carrier | Envelope | Seam to implement |
| --- | --- | --- |
| HTTP | none — the request *is* the envelope | `HttpCall` / `HttpCall` (1 method) |
| WebSocket | one frame per text message | `FramedConnection` (1 method) |
| TCP | 4-byte big-endian length, then the frame | `FramedConnection` (1 method) |

Each seam is one method wide on purpose. Reconnect, backoff, auth refresh,
multiplexing and TLS live in the application, which already has opinions about
all five; the runtime owns only the bytes and the exchange shape.

### The frame envelope

HTTP already carries a method, a path, a query and a body. WebSocket and TCP
carry none of that, so a call is framed explicitly:

```json
{"v":1,"id":"c7-2","t":"call","key":"walk_matter","method":"POST","path":"/v1/matters/abc/walk","body":{"choice_id":"c"}}
{"v":1,"id":"c7-2","t":"data","body":{"kind":"branch"}}
{"v":1,"id":"c7-2","t":"end"}
```

Frame types are `call`, `data`, `end`, `error`, `cancel`. A unary answer is one
`data` then `end`, or a single `error`. Anything else is a contract violation
and the transport says so rather than keeping the first frame and moving on.

**The encoding is normative, and that is the point.** `ridl/framing.py` is the
reference; `examples/frames/conformance.json` pins its output byte-for-byte; and
every port asserts against those fixtures:

```sh
python3 scripts/test_framing.py
node --experimental-strip-types --test runtime/typescript/frame.conformance.test.ts
cargo test --manifest-path runtime/rust/Cargo.toml frame
```

The rules a port has to reproduce are UTF-8 JSON, compact separators, a fixed
member order (not alphabetical, not map order), an absent value as an omitted
member rather than `null`, and literal non-ASCII. This ceremony exists because
the alternative has already cost us once: the Rust and TypeScript opto-sync
runtimes each hashed a minted record id from prose and produced different ids
for the same request. A fixture file is cheaper than that bug.

`meta` carries out-of-band strings — auth token, `traceparent`, deadline. It is
not part of the typed contract and generated code never reads it.

Correlation ids are monotonic per connection and never derived from the request.
A content-hashed id would make two genuinely separate calls with identical
payloads collide, which is the same trap as the minted record id above.

### Which transports an operation allows

Declared in the route map and checked at generation time, so a client is never
emitted for a combination nothing can perform:

```json
"subscribe_matter": {
  "path": "/v1/matters/{id}/events",
  "methods": ["GET"],
  "transports": ["websocket", "tcp"],
  "stream": "server_stream",
  "path_params": { "id": "MatterId" },
  "response": "MatterEvent"
}
```

`transports` defaults to `["http"]` and `stream` to `"unary"`, so every map
written before this existed keeps its meaning. `ridl check` rejects, among
others: a stream with no framed transport; a stream that also claims HTTP;
`HEAD`/`OPTIONS` off HTTP; `content_type: "form"` off HTTP; a Connect-shaped
PascalCase key that does not include HTTP; and a queued operation that is not
unary.

Streaming is **contract-only today**: the emitters do not yet produce a
stream-returning signature, so `client_routes()` withholds streaming operations
rather than emitting them as unary calls that would read the first frame and
drop the rest. `FramedStream` is the seam they will land on. Unary over all
three carriers works now.

## opto-sync interop

`opto_sync.rs` / `opto-sync.ts` route a call by the `delivery` the map declares:
`direct` goes to the carrier, `opto_sync_queued` goes into opto-sync's durable
queue and returns the caller's own write through the local view.

**The arrow only points one way.** This runtime calls opto-sync; opto-sync knows
nothing about RPC. That is enforced structurally rather than by agreement:

- The seams are traits/interfaces *declared here* — `MutationQueue`,
  `LocalReadback` — so these modules compile and test with opto-sync absent.
- Nothing imports `opto_sync_client`. An application does, and passes something
  that satisfies the seam: `ProtocolQueue`, `SqliteProtocolStore`,
  `OptoSyncClient`, or a fake.
- It respects the boundary `opto-sync-interfaces/README.md` states — interfaces
  must never depend on a sync engine or client implementation.
- On the TypeScript side it is also the consumption pattern `@opto-sync/client`
  asks for: its namespaced export surface is documented as being "for wrapper
  libraries that re-export a curated slice of opto-sync".

Two transports, one socket, still decoupled: if an application wants its RPC
frames and opto-sync's push/pull to share a connection, it implements
`FramedConnection` over the socket it already runs for opto-sync. That adapter
lives in the application. Neither library learns about the other.

Only mutations are queued. opto-sync's queue is record-shaped — a payload must
be a JSON object, a table must be a SQL-safe scope id, a queued delete is a
tombstone with no data, and there is no per-mutation response channel — so a
read has nowhere to get an answer from. Which operations are queued is decided
in the route map and validated, never guessed at runtime.

## ores-otel interop

`telemetry.rs` / `telemetry.ts` define one seam:

```rust
pub trait RpcTelemetrySink: Send + Sync {
    fn emit(&self, event: &RpcEvent<'_>) -> Result<(), String>;
}
```

Deliberately the same shape as
`opto-sync-clients/clients/rust/src/telemetry.rs`'s
`ProtocolSyncTelemetrySink`, so an application writes **one** adapter over
`ores-otel` / `next-loggers` and points both stacks at it.

- No OTel SDK is linked here, no global provider is installed, no exporter is
  shut down, and no sampling decision is made. All four belong to the process.
- Without a sink, nothing is emitted and no telemetry code runs.
- **Fail-open, always.** A sink that errors or panics changes nothing about the
  call; the failure is contained at the boundary. Telemetry that can break an
  RPC is worse than no telemetry.
- A closure is a sink (blanket impl in Rust, structural in TypeScript), so the
  cheapest possible adapter is one line.

### What an event carries, and what it never carries

`RpcEvent` is the operation key, service, method, **path template**, carrier,
outcome, duration, failure code, and the frame correlation id plus any trace and
span ids the caller already had.

It carries no request body, no response body, no path parameter values, and no
`meta` contents. A route map cannot know which fields are sensitive, so none of
the payload crosses the boundary — and the key and template are low-cardinality
by construction, which the substituted path is not, so they are safe as metric
labels. An application that wants richer spans has the payload in hand at the
call site and knows what it is allowed to record.

`correlation_id` is what stitches a client call to a server span on a framed
transport, where there is no HTTP request id to lean on.

## A note on the TypeScript modules

They import each other with explicit `.ts` extensions so they run directly under
`node --experimental-strip-types` with no build step — which is also how their
tests run. Compiling them with `tsc` needs `allowImportingTsExtensions` and
`rewriteRelativeImportExtensions` (TypeScript 5.7+); a bundler needs nothing.
They avoid constructor parameter properties for the same reason.
