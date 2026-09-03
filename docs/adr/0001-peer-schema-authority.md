# ADR 0001: TypeSpec and JSON Schema/OpenAPI are peer contract authorities

- **Status:** Accepted
- **Date:** 2026-09-02
- **Decision owners:** ORESoftware contract and migration maintainers
- **Applies to:** authored schemas, RPC/API contracts, generated SQL, Protobuf/gRPC, clients, ORM views, release gates, and schema-audit automation
- **Supersedes:** every P0/P1 policy that makes TypeSpec canonical over JSON Schema/OpenAPI, every policy that makes JSON Schema/OpenAPI canonical over TypeSpec, and the chain `TypeSpec -> JSON Schema/OpenAPI -> downstream`
- **Related:** `ORESoftware/my-ai#61`, Linear `DEN-3959`, Linear `DEN-3982`

## Context

The repository previously described one TypeSpec authority with JSON Schema and
Protobuf beneath it as projections. That hierarchy is rejected. It can hide a
fault in the favored source or generator, makes reconciliation dependent on
precedence instead of evidence, and contradicts the fleet architecture in which
TypeSpec and JSON Schema/OpenAPI are separately authored contract definitions.

Scheduled schema sweeps also need durable proof of what they actually inspected
and executed. A green status without exact scope, inputs, checks, outputs, and
findings cannot establish repository or fleet coverage.

## Decision

TypeSpec and JSON Schema/OpenAPI are **co-equal, human-authored, top-level
contract authorities**. Neither is generated from, subordinate to, a fallback
for, or permitted to overwrite the other.

The following topology is explicitly rejected:

```text
TypeSpec -> JSON Schema/OpenAPI/Protobuf -> wire clients
```

The required paired topology is:

```text
TypeSpec
  -> normalized contract IR_T
  -> persistence IR_T plus reviewed mapping metadata
  -> SQL_T
  -> Protobuf/proto3
  -> gRPC
  -> wire clients

JSON Schema Draft 2020-12/OpenAPI
  -> normalized contract IR_J
  -> persistence IR_J plus reviewed mapping metadata
  -> interfaces, language types, and runtime validators
  -> SQL_J
  -> HTTP/write clients
```

A repository may omit a downstream artifact only when it is genuinely outside
that repository's declared scope. It may not collapse the two source lanes.
Where persistence mapping is in scope, both lanes independently produce SQL
candidates. Where a transport is in scope, the appropriate lane independently
produces its transport/client artifacts.

## Derived witnesses

Cross-translations are useful as additional evidence:

```text
T -> J(T) -> T(J(T))
J -> T(J) -> J(T(J))
```

They are never editable authorities. Generated witnesses:

- live only under generated paths;
- carry source, tool, option, and artifact digests;
- never overwrite `idl/typespec/`, authored `json-schema/`, or authored OpenAPI;
- never resolve a discrepancy by copying one lane over the other.

Protobuf is a reviewed binary/streaming artifact of the TypeSpec lane. Its field
numbers, reservations, presence behavior, and wire encoding remain
release-critical, but Protobuf is not a third peer schema authority.

Per-service route-map JSON remains the canonical operation inventory for the
route-map bundle. That inventory role is orthogonal to the peer authority of the
type/schema definitions.

## Convergence invariant

The two authored authorities may differ in syntax and representation, but every
shared governed fact must converge after normalization. Compare at least:

- stable type, model, field, operation, error, and version identities;
- encoded names and public/private visibility;
- required, optional, nullable, default, and unknown-field behavior;
- scalar width, precision, scale, format, pattern, range, and collection rules;
- enums, unions, discriminators, recursion, and compatibility/evolution policy;
- Protobuf field numbers, reservations, presence, streaming, and gRPC service behavior;
- independently generated client/type surfaces against common fixtures;
- SQL_T and SQL_J by canonical parse plus disposable-database catalog read-back;
- generated Diesel and SeaORM model/relationship/type surfaces and common database behavior;
- source provenance, generator versions/options, and declared lossy edges.

Textual equality is not required when formatting or representation differs.
Semantic equivalence, stable identity, catalog equivalence, and shared behavior
are required.

## Discrepancy-stop protocol

Any unexplained mismatch enters `STOPPED_FOR_EVALUATION` and blocks:

- generated artifact publication;
- migration planning or application;
- automatic merge or promotion;
- package or client release;
- consumer rollout;
- deployment.

No lane wins automatically. A discrepancy record must contain:

1. a stable fingerprint;
2. exact source locations and source/artifact digests;
3. both normalized interpretations;
4. SQL/catalog, Protobuf/gRPC, generated-client/type, Diesel, and SeaORM effects where applicable;
5. compatibility and security impact;
6. owner, reviewer, status, and proposed human resolution;
7. links to GitHub and Linear records;
8. any scoped, tested, expiring waiver.

A waiver may not weaken tenant isolation, authorization, encryption, data-loss
prevention, migration safety, field-number safety, or compatibility guarantees.
After human reconciliation, edit every affected authored authority and rerun all
lanes from clean inputs. Do not mechanically copy the favored representation.

## Execution receipts

Every manual or scheduled schema audit, sweep, comparator run, or migration
parity check must emit a machine-readable execution receipt. The minimum fields
are:

```json
{
  "format": "ores.schema-audit-receipt/v1",
  "status": "passed|stopped_for_evaluation|failed|partial",
  "startedAt": "RFC3339 UTC timestamp",
  "finishedAt": "RFC3339 UTC timestamp",
  "actor": "workflow/job/user identity",
  "scope": {
    "organizations": [],
    "repositories": [],
    "branches": [],
    "commits": [],
    "services": [],
    "files": [],
    "linearRecords": [],
    "githubRecords": [],
    "paginationOrCaps": [],
    "exclusions": [],
    "inaccessibleOrReadOnly": []
  },
  "inputs": [],
  "tools": [],
  "checks": [],
  "artifacts": [],
  "findings": [],
  "discrepancyFingerprints": [],
  "zeroUnexplainedFindings": false
}
```

Each input records its commit or content digest. Each tool records its version,
configuration, dialect, vocabulary, and options. Each check records whether it
was executed, skipped, failed, or inferred; planned checks may never be reported
as executed. Artifacts include immutable or content-addressed links to logs,
reports, CI runs, and generated outputs.

A receipt with incomplete scope must use `partial`. A run with any unexplained
mismatch must use `stopped_for_evaluation`. `passed` requires all declared checks
to execute successfully and `zeroUnexplainedFindings` to be true.

A sweep without a receipt is incomplete and may not claim repository, project,
organization, or fleet coverage.

## Change workflow

1. Start from exact, fresh source revisions.
2. A change may originate in either authored lane.
3. Identify every shared fact and downstream artifact affected.
4. Compile/validate each source independently with pinned tools and options.
5. Generate independent normalized views and downstream candidates.
6. Run direct, cross-translation, round-trip, SQL/catalog, transport/client, and ORM comparisons applicable to the scope.
7. Stop on discrepancies; create or update one idempotent GitHub/Linear record per fingerprint.
8. Reconcile both authored authorities through human review.
9. Rerun from clean inputs.
10. Publish only after convergence and attach the complete execution receipt.

## Consequences

- There is no P0 TypeSpec/P1 JSON Schema hierarchy.
- A compiler AST, generated IR, SQL candidate, ORM model, OpenAPI document, or
  Protobuf descriptor cannot silently become the sole source of truth.
- Source-specific frontends remain independent enough to expose parser/emitter
  defects; they may share stable IDs, comparison schemas, and conformance
  fixtures.
- CI is intentionally fail-closed. A missing lane or missing receipt is not a
  degraded green result.
- Historical records may retain the rejected text only inside a clearly marked
  quotation or supersession section. Active guidance must state this ADR's
  peer-authority decision.

## Verification

The repository and fleet scanners must reject unqualified active claims such as:

- “TypeSpec is the canonical/P0 authority” when JSON Schema/OpenAPI is in scope;
- “JSON Schema is the secondary/P1 projection”;
- “begin every semantic change in TypeSpec”;
- “JSON Schema/OpenAPI is generated from TypeSpec” or the reverse;
- “publish the TypeSpec-lineage release”;
- a successful scheduled sweep without an execution receipt.

The scanners may allow those phrases only when the surrounding text explicitly
labels them rejected, incorrect, superseded, or historical.
