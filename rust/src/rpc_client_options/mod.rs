//! RPC client option catalog: the authority for the chaining surface, its
//! deterministic Markdown/JSON Schema projections, and the fixture corpus that
//! makes the projections falsifiable.

pub mod emit_markdown;
pub mod conformance;
pub mod emit_rust;
pub mod emit_schema;
pub mod emit_typescript;
pub mod fixtures;
pub mod model;
pub mod names;

pub use model::{Catalog, CatalogError};

/// Repository-relative location of the authored catalog.
pub const CATALOG_PATH: &str = "contracts/rpc-client-options/v1/catalog.json";
/// Repository-relative location of the authored peer plan schema.
pub const AUTHORED_PLAN_SCHEMA_PATH: &str = "json-schema/rpc-request-plan.schema.json";
/// Directory holding every generated artifact for this contract family.
pub const GENERATED_DIR: &str = "generated/rpc-client-options";
/// Repository-relative location of the generated Markdown.
pub const MARKDOWN_PATH: &str = "docs/rpc-client-options.md";
/// Generated TypeScript runtime option table.
pub const TS_RUNTIME_PATH: &str = "clients/typescript/src/options.generated.js";
/// Generated TypeScript type-state declarations.
pub const TS_TYPES_PATH: &str = "clients/typescript/src/options.generated.d.ts";
/// Generated Rust type-state option surface.
pub const RUST_SURFACE_PATH: &str = "rust/src/rpc_client_surface.rs";

/// Canonical JSON serialization used for every generated artifact: two-space
/// indent, sorted keys via `serde_json`'s preserve-order-free default, and a
/// trailing newline. Byte stability is what makes `check` meaningful.
pub fn canonical_json(value: &serde_json::Value) -> String {
    let mut buffer = Vec::with_capacity(8 * 1024);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    serde::Serialize::serialize(value, &mut serializer).expect("json value is serializable");
    let mut text = String::from_utf8(buffer).expect("serde_json emits utf-8");
    text.push('\n');
    text
}
