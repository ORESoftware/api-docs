# ADR: parity-gated multi-runtime interface generation

## Decision

TypeSpec and JSON Schema/OpenAPI remain unordered, independent top-level contract authorities. Product `*-interfaces` repositories run the shared parity tool from this repository at an exact commit. Each authority independently produces a normalized signature and language candidates. Only byte-identical candidate sets following semantic agreement may become committed final artifacts.

`*-interfaces` repositories classify every model as exactly one of `isomorphic`, `client`, `edge`, or `server`. Source authorities, candidate signatures, final code, and receipts are separated by scope. TypeScript final exports are additionally separated into browser, Node.js, Deno, Bun, and edge entrypoints. Browser and edge exports may never expose server scope.

Runtime validation is implemented by the corresponding `*-lib-core`: Zod for TypeScript, Garde for Rust, `go-playground/validator/v10` for Go, and Gleam decoders. `*-clients` import the public validation SDK from `*-lib-core`; they do not copy schemas or import server validators.

Validation-to-route coupling uses stable `operationId` values from `ORESoftware/api-docs`. Route-binding documents are digest-bound into parity receipts. A route-signature or validator-signature discrepancy is a stop-and-evaluate condition, not an automatic overwrite.

## Consequences

- Generated definitions are reproducible VCS artifacts, not an unchecked build side effect.
- A semantic mismatch prevents final writes and releases.
- Private/server-only contracts remain isolated even when a repository is public.
- Test organizations can certify the exact receipt and generated artifacts independently of producer repositories.
