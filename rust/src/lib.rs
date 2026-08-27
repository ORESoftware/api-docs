//! Route-map API documentation.
//!
//! The interchange contract is a JSON object whose **keys are operations** and
//! whose **values are HTTP routes**. Languages may author those keys as
//! annotations, param types, return types, function types, or a mix; they
//! serialize to the same map.
//!
//! The map is projected into several standards closely (not one of them
//! perfectly): OpenAPI 3.1, JSON Schema 2020-12, Connect JSON unary, OpenRPC
//! 1.3, and JSON Hyper-Schema links. Every projection is checked with JSON
//! Schema.

pub mod binding;
pub mod catalog;
pub mod headers;
pub mod html;
pub mod infer;
pub mod map;
pub mod paths;
pub mod project;
pub mod schema;

#[cfg(feature = "axum")]
pub mod axum_router;

pub use binding::{RouteBinding, RpcMethod, UnaryFn};
pub use catalog::Catalog;
pub use map::{RouteEntry, RouteMap};

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const GENERATED_BY: &str = "ores-api-docs";
