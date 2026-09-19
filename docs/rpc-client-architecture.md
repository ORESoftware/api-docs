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
runtime. Only the TypeScript client implements this today. The Rust surface is a
type-state builder with injected `UnaryTransport` / `StreamTransport` and does no
scheduling of its own; Dart, Go and Gleam fluent clients do not exist yet. When
they are written they should use rx-dart, rxgo and rx-gleam (rxRust for a Rust
scheduler) so the semantics stay the library's rather than forking per runtime.

## The request plan is the conformance unit

Every chain can be serialized with `to_plan()` without opening the network. A
plan is canonical — sorted keys, declared bounds, no credentials — so two
clients agree exactly when their plans are byte-identical.

`generated/rpc-client-options/chain-conformance.json` records the plan the Rust
builder produces for each of a set of authored chains, including a chain and its
exact reverse, to pin down that option order does not matter. The TypeScript
suite replays those chains and compares bytes.

Byte-identical is meant literally. `chain-conformance.json` records
`plan_canonical`, the exact string the Rust client serialized, and other clients
compare their own serialization against that string unparsed. Numbers have one
spelling: an integral float is written as an integer, because JavaScript writes
`2.0` as `2` and serde_json does not. An earlier version of the conformance test
parsed and re-serialized the Rust plan first, which hid exactly that difference.

Credentials never enter a plan. Redaction is a property of the plan boundary,
not of individual options: every header and query field is judged by name,
whatever wrote it, and URL userinfo is replaced. `with_bearer_token` records
`auth_mode: "bearer_override"` and the plan carries `authorization:
"[redacted]"`; the real token reaches only the wire. URL userinfo is replaced
with the bare word `redacted` rather than `[redacted]`: square brackets are not
valid RFC 3986 userinfo, so the bracketed spelling made every redacted proxy URL
fail `format: uri`. The gate asserts formats for exactly that reason.

## One call, three views

Every chain assembles one document and shows it three ways. They are derived
from the same assembly step, so they cannot drift; redaction is a pure function
applied afterwards and can only remove what the others already have.

| View | Rust | TypeScript | Holds credentials | Use it to |
| --- | --- | --- | --- | --- |
| Plan | `to_plan()` | `toPlan()` | no | log, document, compare across languages |
| Wire plan | `wire_plan()` | `state.wirePlan()`, `request.wire` | yes | execute: what a transport sends |
| Execution identity | `execution_identity()` | `executionIdentity()`, `executionDigest()` | yes (digest: no) | key a cache or join in-flight calls |

**A plan must never key a cache.** Two callers with different credentials have
identical plans, so a plan-keyed cache serves one principal's response to
another. That shipped twice: through the bearer token, and then — after headers
were added to the key by hand — through a credential in a query field, which
redaction collapses just the same. The identity is therefore the *pre-redaction*
document, which covers every class redaction hides now or learns to hide later,
by construction rather than by enumeration. The TypeScript scheduler keys on its
SHA-256 so its maps do not retain credentials as long-lived strings.

**A transport must never execute from a plan.** Handed only the plan, a proxying
transport received `http://redacted@proxy` and the proxy answered 407. Unary
transports receive `wire` beside `plan`; a stream carrier receives
`{ plan, wire }` beside the call frame, never inside it — the frame goes to the
server, and a proxy URL has no business there.

**Showing a call shows its plan.** `{:?}` in Rust, and `JSON.stringify` /
`console.log` in TypeScript, walk an object's own fields — which hold the bearer
token, the secrets map and the raw query. All of them now answer with the
redacted plan, so `debug!(?call)` or an error reporter serializing a chain is
not a credential leak.

**A stream's plan and its frame are the same call.** The frame is built from the
chain state, the same place the plan is built from. It used to be built from the
`prepare()`-time request instead, so `addQueryField()` reached the plan and was
never sent, while a `prepare()`-time query was sent and never planned (and so
never redacted). Path fields fill `{name}` placeholders in the path template,
percent-encoded; a placeholder with no field, or a field with no placeholder, is
an error rather than a silent drop.

**A header that is a list is written as a list.** `cache-control` is a directive
list, and three options write to it. Each used to assign the header outright, so
the last one won, the others' directives vanished without a word, and the plan
depended on the order the options were called in — which the conformance corpus
then pinned, byte for byte, in both languages, under the rationale "cache
directives layer". The catalog now names its `list_valued_headers`; every write
to one, the caller's own `add_header` included, joins a sorted, de-duplicated
union. The catalog's integrity check refuses two options writing the same header
unless it is list-valued or the options are mutually exclusive, so the silent
overwrite cannot be authored again.

## What the scheduler promises

The unary pipeline is ordered, and the order is the contract:

```text
attempt -> retry/backoff -> TOTAL timeout -> lead delay -> absolute deadline
```

- `with_timeout` bounds the **whole call**: every attempt and every backoff wait
  share one budget, and expiring it is terminal. Applied per attempt — as it
  first was — `withTimeout(100)` with three retries ran for 400ms, and the
  timeout error was itself retried. Streams follow the same rule; their *idle*
  timeout, by contrast, belongs to one carrier and sits inside the retried unit,
  so a stalled session is a reason to reopen.
- `delay` and `add_jitter` are not charged to the timeout. `with_deadline` is
  absolute, so it does include them.
- A stream attempt cleans up after itself *inside* the retried unit, and the
  error path awaits the close before re-raising: a failed session is closed
  before the next one is opened, so a flapping carrier cannot stack live
  sessions.
- `debounce` and `throttle` hold across a change of interval. Replacing the
  gate when the number changed let both debounced calls out — the old gate still
  fired on its own timer — and let a throttled call straight in, since a fresh
  gate always admits its first value. A superseded call is rejected, never left
  pending; a throttled call is judged by its own window against the last call
  let through.
- Every scheduler map is bounded (1024 entries) and self-cleaning: a drained
  concurrency queue forgets its key, an idle gate removes itself, and the cache
  evicts dead entries and then the oldest. Shape and concurrency keys are
  caller-controlled, so anything else is a leak per distinct key.
- The cache remembers only a successful answer from the server. Caching an error
  pins a transient 503 for the whole TTL; caching a fallback turns one outage
  into the answer. It stores and serves its own copies, so a caller sorting a
  result in place cannot reorder it for the next caller, and an outcome that
  cannot be copied is not cached at all.
- `stale_while_revalidate` serves an entry past its TTL for the given window and
  refreshes it behind the caller — one refresh per identity however many callers
  hit the stale entry. A failed refresh keeps the stale answer.

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

The generator and `rust/src/rpc_client_surface.rs` live in one crate, so a new
generated constant that hand-written code imports cannot be generated until the
crate compiles, and the crate cannot compile until the constant exists. Seed it
by hand once, run `ores-rpc-docs generate`, and let `check` prove the seed
matched; do not split the crate to avoid this.

The scheduler and identity guarantees above are pinned by
`clients/typescript/src/fluent-identity.test.js` and the `identity_tests` module
in `rust/src/rpc_fluent.rs`. Each of those tests was falsified once — the defect
it guards was put back and the test confirmed to fail, for that reason — because
a regression test that cannot fail proves nothing. Two of them first passed
vacuously: an echo transport that read the *redacted* plan returned the same
bytes for every principal, so "Bob was not served Alice's response" compared two
equal strings. The isolation tests therefore assert their own premise first —
that the plans really do collapse, and that the echo really can tell the
principals apart.

`npm test` names its files one by one, so a test file left off the list is never
run and nothing says so; `src/test-manifest.test.js` fails when that happens.
