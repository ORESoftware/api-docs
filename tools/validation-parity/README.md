# Independent contract-authority parity tool

`parity-tool.mjs` is the shared, dependency-free gate used by `*-interfaces` repositories. It treats TypeSpec and JSON Schema/OpenAPI as **independent, peer, human-authored authorities**. Neither source is generated from, nor allowed to overwrite, the other.

For each declared visibility scope, the tool independently parses both sources into semantic signatures, compares requiredness, scalar/reference/array types, defaults, enums, ranges, lengths, patterns, formats, object closure, and model membership, then independently renders TypeScript, Rust, Go, and Gleam candidates. Final artifacts and a digest receipt are written only when both semantic signatures and all generated candidates agree.

## Scope and runtime policy

The manifest recognizes four non-overlapping scopes:

- `isomorphic`: safe in clients, edge, and servers.
- `client`: client-only definitions.
- `edge`: edge-only definitions.
- `server`: server-only/private definitions, kept in separate authority and generated folders.

TypeScript entrypoints are emitted independently for browser, Node.js, Deno, Bun, and edge runtimes. Browser and edge entrypoints fail closed if configured to export `server` scope. Rust, Go, and Gleam outputs remain separated by scope.

## Commands

```bash
node .deps/api-docs/tools/validation-parity/parity-tool.mjs --self-test
node .deps/api-docs/tools/validation-parity/parity-tool.mjs --write
node .deps/api-docs/tools/validation-parity/parity-tool.mjs --check
```

`--write` is for reviewed regeneration. `--check` is the CI/release gate and proves that committed artifacts exactly reproduce from both authorities. Mismatch means stop, report the semantic diff, and do not update final definitions.

A manifest may bind `validation/route-bindings.v1.json`; each binding must use a unique, non-empty `operationId` owned by `ORESoftware/api-docs`. Its semantic digest and operation set are included in the parity receipt so route and validation signatures can be reviewed together.
