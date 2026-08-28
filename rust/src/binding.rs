//! Language surfaces: a route key may be an annotation, param types, a
//! return type, a function type, or any combination.

use serde::{Deserialize, Serialize};

/// How source code attaches a map key to a handler.
///
/// JSON Schema: `json-schema/route-binding.schema.json`. At least one of
/// `annotation`, `param_types`, `return_type`, `function_type` must be set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

impl RouteBinding {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.annotation.is_none()
            && self.param_types.is_empty()
            && self.return_type.is_none()
            && self.function_type.is_none()
    }

    /// Annotations, types, or both — combination is the common case.
    #[must_use]
    pub fn is_combination(&self) -> bool {
        let n = usize::from(self.annotation.is_some())
            + usize::from(!self.param_types.is_empty())
            + usize::from(self.return_type.is_some())
            + usize::from(self.function_type.is_some());
        n >= 2
    }
}

/// Typed RPC method: param type + return type imply the route.
/// Languages without attributes (Gleam, some TS) use this instead of
/// annotations. Languages with attributes may still implement it.
pub trait RpcMethod {
    const KEY: &'static str;
    const PATH: &'static str;
    const METHODS: &'static [&'static str];
    type Params;
    type Output;
}

/// Function type for a unary JSON handler (Connect-shaped POST).
pub type UnaryFn<M> = fn(<M as RpcMethod>::Params) -> <M as RpcMethod>::Output;

/// HTTP-shaped RPC: typed path params + query string in addition to JSON body.
pub trait RpcHttp: RpcMethod {
    type PathParams;
    type Query;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Healthz;

    impl RpcMethod for Healthz {
        const KEY: &'static str = "healthz";
        const PATH: &'static str = "/healthz";
        const METHODS: &'static [&'static str] = &["GET"];
        type Params = ();
        type Output = &'static str;
    }

    impl RpcHttp for Healthz {
        type PathParams = ();
        type Query = ();
    }

    fn healthz_handler(_: ()) -> &'static str {
        "ok"
    }

    #[test]
    fn function_type_and_associated_types() {
        let _f: UnaryFn<Healthz> = healthz_handler;
        assert_eq!(Healthz::PATH, "/healthz");
        let binding = RouteBinding {
            annotation: Some("get(\"/healthz\")".into()),
            param_types: vec!["()".into()],
            return_type: Some("&'static str".into()),
            function_type: Some("UnaryFn<Healthz>".into()),
            file: None,
            symbol: Some("healthz_handler".into()),
        };
        assert!(binding.is_combination());
    }
}
