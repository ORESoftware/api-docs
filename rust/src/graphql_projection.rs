//! Typed compiler/runtime metadata for an explicitly authored GraphQL projection.
//!
//! GraphQL remains optional. A semantic operation has no GraphQL exposure unless
//! an authored `#[ores_graphql(...)]` resolver exists. This Rust descriptor is a
//! generated projection of that authored metadata; it is not a third contract
//! authority and is intentionally not a serialized interchange format.
//!
//! TypeSpec plus independently authored Draft 2020-12 JSON Schema remain the
//! peer authorities for persisted GraphQL projection manifests. Request,
//! response, and error types remain owned by the semantic [`crate::OperationSpec`]
//! reached through the generated `__ores_invoke_*` boundary.

use thiserror::Error;

use crate::RpcStreamMode;

pub const GRAPHQL_V1_HTTP_PATH: &str = "/v1/graphql";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphqlProjectionKind {
    Query,
    Mutation,
    Subscription,
}

impl GraphqlProjectionKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        return match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphqlProjectionDescriptor {
    pub endpoint: &'static str,
    pub operation_key: &'static str,
    pub invoke: &'static str,
    pub kind: GraphqlProjectionKind,
    pub field: &'static str,
    pub stream: RpcStreamMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum GraphqlProjectionDescriptorError {
    #[error("GraphQL projection endpoint must be /v1/graphql, got {actual:?}")]
    Endpoint { actual: String },
    #[error("GraphQL projection operation_key must be a stable dotted lowercase object key")]
    OperationKey,
    #[error("GraphQL projection invoke must be a crate-local generated __ores_invoke_* boundary")]
    InvokeBoundary,
    #[error("GraphQL projection field is not a valid non-introspection GraphQL name")]
    Field,
    #[error("GraphQL query/mutation projections require unary semantic response shape")]
    UnaryKindRequiresUnary,
    #[error("GraphQL subscription projections require server_stream semantic response shape")]
    SubscriptionRequiresServerStream,
}

impl GraphqlProjectionDescriptor {
    pub fn validate(&self) -> Result<(), GraphqlProjectionDescriptorError> {
        if self.endpoint != GRAPHQL_V1_HTTP_PATH {
            return Err(GraphqlProjectionDescriptorError::Endpoint {
                actual: self.endpoint.to_owned(),
            });
        }
        if !valid_operation_key(self.operation_key) {
            return Err(GraphqlProjectionDescriptorError::OperationKey);
        }
        if !valid_invoke_boundary(self.invoke) {
            return Err(GraphqlProjectionDescriptorError::InvokeBoundary);
        }
        if !valid_graphql_name(self.field) {
            return Err(GraphqlProjectionDescriptorError::Field);
        }

        return match (self.kind, self.stream) {
            (GraphqlProjectionKind::Query | GraphqlProjectionKind::Mutation, RpcStreamMode::Unary) => {
                Ok(())
            }
            (GraphqlProjectionKind::Subscription, RpcStreamMode::ServerStream) => Ok(()),
            (GraphqlProjectionKind::Query | GraphqlProjectionKind::Mutation, _) => {
                Err(GraphqlProjectionDescriptorError::UnaryKindRequiresUnary)
            }
            (GraphqlProjectionKind::Subscription, _) => {
                Err(GraphqlProjectionDescriptorError::SubscriptionRequiresServerStream)
            }
        };
    }
}

fn valid_operation_key(key: &str) -> bool {
    let mut parts = key.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if !valid_operation_key_part(first) {
        return false;
    }
    let mut part_count = 1_usize;
    for part in parts {
        if !valid_operation_key_part(part) {
            return false;
        }
        part_count += 1;
    }
    return part_count >= 2;
}

fn valid_operation_key_part(part: &str) -> bool {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    return chars.all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '_'
            || character == '-'
    });
}

fn valid_invoke_boundary(invoke: &str) -> bool {
    if !invoke.starts_with("crate::") {
        return false;
    }
    let Some(terminal) = invoke.rsplit("::").next() else {
        return false;
    };
    return terminal.starts_with("__ores_invoke_") && terminal.len() > "__ores_invoke_".len();
}

fn valid_graphql_name(value: &str) -> bool {
    if value.is_empty() || value.starts_with("__") {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    return chars.all(|character| character == '_' || character.is_ascii_alphanumeric());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> GraphqlProjectionDescriptor {
        return GraphqlProjectionDescriptor {
            endpoint: GRAPHQL_V1_HTTP_PATH,
            operation_key: "users.get_user",
            invoke: "crate::routes::rest::users::handlers::__ores_invoke_get_user",
            kind: GraphqlProjectionKind::Query,
            field: "get_user",
            stream: RpcStreamMode::Unary,
        };
    }

    #[test]
    fn explicit_query_descriptor_is_valid() {
        query()
            .validate()
            .expect("explicit query descriptor should validate");
    }

    #[test]
    fn graphql_is_explicit_not_inferred() {
        let descriptor = query();
        assert_eq!(descriptor.endpoint, "/v1/graphql");
        assert_eq!(descriptor.kind, GraphqlProjectionKind::Query);
        assert_eq!(descriptor.field, "get_user");
    }

    #[test]
    fn subscription_requires_server_stream() {
        let mut descriptor = query();
        descriptor.kind = GraphqlProjectionKind::Subscription;
        descriptor.field = "watch_users";
        assert_eq!(
            descriptor.validate().unwrap_err(),
            GraphqlProjectionDescriptorError::SubscriptionRequiresServerStream
        );
        descriptor.stream = RpcStreamMode::ServerStream;
        descriptor
            .validate()
            .expect("server-stream subscription should validate");
    }

    #[test]
    fn query_rejects_streaming_response_shape() {
        let mut descriptor = query();
        descriptor.stream = RpcStreamMode::ServerStream;
        assert_eq!(
            descriptor.validate().unwrap_err(),
            GraphqlProjectionDescriptorError::UnaryKindRequiresUnary
        );
    }

    #[test]
    fn descriptor_rejects_nonlocal_or_nonpolicy_invoker() {
        let mut descriptor = query();
        descriptor.invoke = "other_crate::handlers::__ores_invoke_get_user";
        assert_eq!(
            descriptor.validate().unwrap_err(),
            GraphqlProjectionDescriptorError::InvokeBoundary
        );
        descriptor.invoke = "crate::routes::rest::users::handlers::get_user";
        assert_eq!(
            descriptor.validate().unwrap_err(),
            GraphqlProjectionDescriptorError::InvokeBoundary
        );
    }

    #[test]
    fn descriptor_rejects_introspection_field_names() {
        let mut descriptor = query();
        descriptor.field = "__schema";
        assert_eq!(
            descriptor.validate().unwrap_err(),
            GraphqlProjectionDescriptorError::Field
        );
    }
}
