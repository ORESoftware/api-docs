/// Framework-neutral declaration exported by every optional
/// `src/routes/**/route.rs` API module.
///
/// The filesystem path is organizational only. `operations` must resolve to
/// reviewed keys in the authoritative `api-docs` route map at the exact same
/// canonical path; the route checker fails closed otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiRouteDefinition {
    pub operations: &'static [&'static str],
}

impl ApiRouteDefinition {
    pub const fn new(operations: &'static [&'static str]) -> Self {
        Self { operations }
    }
}

/// Exact module-level function signature required from every `route.rs`.
/// Generated compile glue assigns `module::route` to this alias so a missing or
/// incompatible function is a normal Rust compile error.
pub type RouteDefinitionFn = fn() -> ApiRouteDefinition;

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> ApiRouteDefinition {
        ApiRouteDefinition::new(&["get_item", "put_item"])
    }

    #[test]
    fn route_signature_is_stable() {
        let checked: RouteDefinitionFn = route;
        assert_eq!((checked)().operations, ["get_item", "put_item"]);
    }
}
