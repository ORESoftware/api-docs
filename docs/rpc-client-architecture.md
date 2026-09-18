# RPC client architecture

Hand-authored companion to the generated [`rpc-client-options.md`](rpc-client-options.md).
That file lists *what* the surface is; this one explains *why* it is shaped this way
and how the pieces are kept honest.

## One authority, several projections

`contracts/rpc-client-options/v1/catalog.json` is the only place the chaining
surface is authored. Everything else is a projection of it:

| Artifact | Purpose |
| --- | --- |
| `docs/rpc-client-options.md` | Human documentation |
| `generated/rpc-client-options/plan.derived.schema.json` | Schema evidence, reconciled against the authored peer |
| `generated/rpc-client-options/plan-fixtures.json` | Positive/negative corpus that makes the docs falsifiable |
| `generated/rpc-client-options/chain-conformance.json` | Plans the Rust builder produced, for other languages to match |
| `clients/typescript/src/options.generated.{js,d.ts}` | TypeScript runtime table and type-state types |
| `rust/src/rpc_client_surface.rs` | Rust type-state option methods |

Per-language method names are **derived** from `option_id`, never authored:
`with_timeout` becomes `withTimeout` in TypeScript and Dart, `WithTimeout` in Go,
and stays `with_timeout` in Rust and Gleam. A client that spells a method
differently is out of contract, and the gate says so by name.

Regenerate with:

```sh
cargo run --bin ores-rpc-docs -- generate   # write every artifact
cargo run --bin ores-rpc-docs -- check      # determinism + staleness
cargo run --bin ores-rpc-docs -- verify     # correctness against the authored schema
```

No network, no clock, no model: the output is a pure function of the catalog bytes.

## Two surfaces, not one client with a mode flag

A unary chain terminates in `make_call()`. A streaming chain terminates in
`stream()`. They are different types and neither carries the other's terminal,
so "did this call stream?" is answered by the type rather than by a runtime
branch. Options follow the same split: `with_fallback` and `dedupe` exist only
on the unary surface, `with_backpressure` and `sample_each` only on the
streaming one, and the plan schema rejects a plan that crosses the line.

```ts
// unary
const [user, ctx] = await client
  .prepare("demo.users.find_user")
  .addPathField("user_id", "user-42")
  .useMessagePack()
  .withTimeout(2_000)
  .withRetries(3)
  .makeCall();

// streaming
const events = await streams
  .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
  .withBackpressure("drop_oldest")
  .withStreamBuffer(256)
  .sampleEach(100)
  .stream();

for await (const event of events) render(event);
```

## Contradictions are unrepresentable, not rejected

Options that contradict each other share an *exclusive group*. Selecting one
returns a builder that no longer carries any member of that group. This is the
type-state pattern, realized natively in each language:

* **Rust** — the group is a type parameter that moves from `Unset` to `Set`. The
  methods that spend it are implemented only for the `Unset` position.
* **TypeScript** — the group is a phantom member of a `Used` union, and the
  builder type is a conditional intersection that drops the group's interface
  once it is spent.

Both are checked. `rust/src/rpc_fluent.rs` carries `compile_fail` doctests, and
`clients/typescript/src/fluent-v2.type-test.ts` carries `@ts-expect-error`
assertions; in both cases the test fails if the invalid chain starts compiling.

At runtime the TypeScript builder installs its method set from the generated
table, so a spent group's methods are genuinely **absent** from the object —
`"useJson" in builder === false` — rather than present and throwing.

The groups today: `serialization`, `auth_mode`, `ip_version`, `rate_limit`
(unary) and `stream_rate_limit` (streaming).

## Flow control comes from Rx, not from hand-rolled timers

Retries, backoff, timeouts, throttling, debouncing, sampling and stream
backpressure are expressed as RxJS operator pipelines in the TypeScript client
(`retry`, `timeout`, `throttleTime`, `debounceTime`, `sampleTime`, `auditTime`).
The semantics are the library's, which is the point: they are the same semantics
the Rx port in each other language provides, so behaviour does not fork per
runtime. Sibling clients use rx-dart, rxgo, rx-gleam and rxRust.

## The request plan is the conformance unit

Every chain can be serialized with `to_plan()` without opening the network. A
plan is canonical — sorted keys, declared bounds, no credentials — so two
clients agree exactly when their plans are byte-identical.

`generated/rpc-client-options/chain-conformance.json` records the plan the Rust
builder produces for each of a set of authored chains, including a chain and its
exact reverse, to pin down that option order does not matter. The TypeScript
suite replays those chains and compares bytes.

Credentials never enter a plan. `with_bearer_token` records
`auth_mode: "bearer_override"` and the plan carries `authorization:
"[redacted]"`; the real token reaches only the wire.

## How correctness is proved

`json-schema/rpc-request-plan.schema.json` is hand-authored and stays an
independent peer of the catalog, in line with the repository's TJSV model. The
two are reconciled *behaviourally* rather than structurally: the generator
derives its own schema from the catalog, and both schemas must return the same
verdict on all 137 generated fixtures. Structural equality is deliberately not
required, because an authored schema may legitimately express a contiguous
integer enum as a range.

The corpus is what gives the documentation teeth. For every option it contains a
plan that uses it on each documented surface, and for every surface-exclusive
option a plan that places it on the wrong surface and asserts rejection. Removing
the unary/stream split from the authored schema fails 13 fixtures by name;
widening a documented bound fails the bound fixture; allowing unknown plan fields
fails the structural fixture.

Run it all with `cargo test -p ores-api-docs --test rpc_client_options_gate`.
