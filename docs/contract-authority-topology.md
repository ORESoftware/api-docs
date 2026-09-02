# Contract authority topology

TypeSpec and JSON Schema/OpenAPI are independent, top-level, human-authored
contract authorities. Neither is generated from, subordinated to, nor allowed
to silently overwrite the other.

```text
TypeSpec ─┬─> PostgreSQL SQL ──────────────┐
          ├─> Protobuf ─> wire types       ├─> semantic parity gates
          └─> gRPC ─────> wire clients     │
                                             ├─> admitted SQL/types
JSON Schema / OpenAPI ─┬─> interfaces      │
                       ├─> client types ────┤
                       ├─> PostgreSQL SQL ──┘
                       └─> write clients

admitted SQL ─┬─> Diesel models ─┐
              └─> SeaORM models ─┴─> ORM/catalog parity gates
```

## Fail-closed convergence

The two SQL outputs are normalized to a PostgreSQL semantic IR and compared.
The two generated type surfaces are normalized to a language-neutral type IR
and compared. Diesel and SeaORM are generated independently from admitted SQL,
then compared with each other and back to the admitted catalog contract.

A discrepancy blocks generation, commit, merge, release, and deployment. The
gate does not select a winner by source order, elapsed time, generator
preference, or historical P0/P1/P2 rank. Work pauses until reviewers have the
two authority revisions, a normalized semantic diff, a recorded decision (and
an explicit expected-delta entry when the difference is intentionally lossy),
and a fresh successful parity run.

## RPC and documentation coupling

Route maps remain the operation inventory. The digest-bound RPC bundle parses a
route map once and emits the OpenAPI/OpenRPC/Connect/Hyper-Schema documents and
the language runtime surfaces from the same normalized operation graph. A docs
artifact is acceptable only when its semantic digest matches the runtime
surface used by that language's RPC implementation.

`idl/authority-graph.json` is the machine-readable topology. Run:

```bash
python3 scripts/check-authority-graph.py
python3 -m unittest scripts/test_check_authority_graph.py
```

The existing RPC IDL and bundle checks remain mandatory; this gate adds the
higher-level independence and convergence invariants they previously left
implicit.
