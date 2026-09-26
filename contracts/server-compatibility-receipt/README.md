# Server compatibility receipt

This contract is the shared evidence envelope for ORES-family Rust `*-web-server.rs`
and `*-api-server.rs` certification across standalone hosting, Scintilla, AWS Lambda,
GCP Cloud Run, and Cloud Run functions.

It is deliberately an **evidence shape**, not a claim that a server is compatible.
A receipt binds server, infra, toolchain, contract, adapter, artifact, capability, and
check identities. Cross-field policy additionally makes zero-step "passed" receipts
invalid and rejects the retired `ORESoftware/ores-stack` CLI repository.

The TypeSpec and JSON Schema files are independently authored peer authorities.
TJSV-generated schemas and Contract IR are comparison evidence only.

A product receipt is admissible only when its repository-specific checks were actually
executed against the exact identities recorded here. Provider capability and behavioral
parity remain the responsibility of the certification suite tracked by api-docs issue #240.
