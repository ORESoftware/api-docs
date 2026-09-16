/// One HTTP verb exposed by a filesystem `src/routes/**/route.rs` module.
///
/// A route file is path-centric: one file owns one canonical filesystem-derived
/// path and may expose several HTTP verbs. Each verb maps to one reviewed RPC
/// operation key. This mirrors Next-style route modules without collapsing the
/// distinct RPC operation identities used by generated clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApiRouteOperation {
    pub method: &'static str,
    pub operation: &'static str,
}

impl ApiRouteOperation {
    #[must_use]
    pub const fn new(method: &'static str, operation: &'static str) -> Self {
        Self { method, operation }
    }
}

/// Framework-neutral declaration for an optional filesystem API route module.
///
/// The filesystem path is organizational and authoritative for the HTTP path.
/// `operations` declares the verb-to-operation projection for that path. The
/// generated checker verifies every pair against the reviewed `api-docs`
/// contract and fails closed on missing, extra, duplicate, or mismatched verbs.
///
/// A single `route.rs` therefore commonly contains both GET and POST (and may
/// contain PUT/PATCH/DELETE/HEAD/OPTIONS as admitted by the contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiRouteDefinition {
    pub operations: &'static [ApiRouteOperation],
}

impl ApiRouteDefinition {
    #[must_use]
    pub const fn new(operations: &'static [ApiRouteOperation]) -> Self {
        Self { operations }
    }

    /// Compatibility helper for the metadata-only route checker. New executable
    /// filesystem routing derives method + operation pairs directly from the
    /// handwritten HTTP verb exports instead of requiring this metadata surface.
    #[must_use]
    pub fn operation_keys(&self) -> Vec<&'static str> {
        self.operations.iter().map(|item| item.operation).collect()
    }
}

/// Exact module-level metadata function signature used by compatibility glue.
/// Newer filesystem server codegen derives the same verb inventory from the
/// authored HTTP handlers and generates the RPC adapter/bindings from it.
pub type RouteDefinitionFn = fn() -> ApiRouteDefinition;

#[cfg(test)]
mod tests {
    use super::*;

    static OPERATIONS: &[ApiRouteOperation] = &[
        ApiRouteOperation::new("GET", "get_item"),
        ApiRouteOperation::new("POST", "create_item"),
    ];

    fn route() -> ApiRouteDefinition {
        ApiRouteDefinition::new(OPERATIONS)
    }

    #[test]
    fn one_route_file_can_own_multiple_http_verbs() {
        let checked: RouteDefinitionFn = route;
        assert_eq!((checked)().operations, OPERATIONS);
        assert_eq!((checked)().operation_keys(), ["get_item", "create_item"]);
    }
}
