//! Typed compiler/runtime metadata for an explicitly authored GraphQL projection.
//!
//! GraphQL remains optional. A semantic operation has no GraphQL exposure unless
//! an authored `#[ores_graphql(...)]` resolver exists. This Rust descriptor is a
//! generated projection of that authored metadata; it is not a third contract
//! authority and is intentionally not a serialized interchange format.
//!
//! TypeSpec plus independently authored Draft 2020-12 JSON Schema remain the
//! peer authorities for persisted GraphQL projection manifests. Request,
//! response, error, policy, and callable identity remain owned by the semantic
//! operation contract reached through the generated `__ores_invoke_*` boundary.

use thiserror::Error;

use crate::{
    validate_operation_key, OperationSemanticBinding, RpcOperationContract, RpcStreamMode,
    SharedOperationSource,
};

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
    #[error("GraphQL projection operation_key must match the authored operation-key grammar")]
    OperationKey,
    #[error("GraphQL projection invoke must be a crate-local generated __ores_invoke_* boundary")]
    InvokeBoundary,
    #[error("GraphQL projection field is not a valid non-introspection GraphQL name")]
    Field,
    #[error("GraphQL query/mutation projections require unary semantic response shape")]
    UnaryKindRequiresUnary,
    #[error("GraphQL subscription projections require server_stream semantic response shape")]
    SubscriptionRequiresServerStream,
    #[error("GraphQL semantic binding is invalid: {0}")]
    SemanticBinding(String),
    #[error("GraphQL projection operation_key does not match the shared semantic binding")]
    SemanticOperationKeyMismatch,
    #[error("GraphQL projection stream does not match the shared semantic operation")]
    SemanticStreamMismatch,
    #[error("GraphQL semantic operation has no generated invoker identity")]
    MissingSemanticInvoker,
    #[error("GraphQL projection invoke does not match the shared semantic generated invoker")]
    SemanticInvokerMismatch,
}

impl GraphqlProjectionDescriptor {
    pub fn validate(&self) -> Result<(), GraphqlProjectionDescriptorError> {
        if self.endpoint != GRAPHQL_V1_HTTP_PATH {
            return Err(GraphqlProjectionDescriptorError::Endpoint {
                actual: self.endpoint.to_owned(),
            });
        }
        validate_operation_key(self.operation_key)
            .map_err(|_| GraphqlProjectionDescriptorError::OperationKey)?;
        if !valid_invoke_boundary(self.invoke) {
            return Err(GraphqlProjectionDescriptorError::InvokeBoundary);
        }
        if !valid_graphql_name(self.field) {
            return Err(GraphqlProjectionDescriptorError::Field);
        }

        return match (self.kind, self.stream) {
            (
                GraphqlProjectionKind::Query | GraphqlProjectionKind::Mutation,
                RpcStreamMode::Unary,
            ) => Ok(()),
            (GraphqlProjectionKind::Subscription, RpcStreamMode::ServerStream) => Ok(()),
            (GraphqlProjectionKind::Query | GraphqlProjectionKind::Mutation, _) => {
                Err(GraphqlProjectionDescriptorError::UnaryKindRequiresUnary)
            }
            (GraphqlProjectionKind::Subscription, _) => {
                Err(GraphqlProjectionDescriptorError::SubscriptionRequiresServerStream)
            }
        };
    }

    /// Prove that an authored GraphQL projection points at the same semantic
    /// operation admitted by the transport-neutral registry and parsed
    /// `#[ores_operation]` authority.
    ///
    /// GraphQL-specific `kind` and `field` remain projection metadata. The
    /// callable ID, OperationSpec, policy, request/response/error schemas,
    /// semantic digest, registry digest, stream cardinality, and generated
    /// invoker are inherited and never independently re-authored here.
    pub fn validate_against_semantic_binding(
        &self,
        binding: &OperationSemanticBinding,
        contract: &RpcOperationContract,
        authority: &SharedOperationSource,
    ) -> Result<(), GraphqlProjectionDescriptorError> {
        self.validate()?;
        binding
            .validate_against_authority(contract, authority)
            .map_err(GraphqlProjectionDescriptorError::SemanticBinding)?;

        if self.operation_key != binding.operation_key {
            return Err(GraphqlProjectionDescriptorError::SemanticOperationKeyMismatch);
        }
        if self.stream != contract.stream {
            return Err(GraphqlProjectionDescriptorError::SemanticStreamMismatch);
        }

        let expected_invoker = contract
            .source
            .invoker
            .as_deref()
            .ok_or(GraphqlProjectionDescriptorError::MissingSemanticInvoker)?;
        let actual_invoker = self
            .invoke
            .rsplit("::")
            .next()
            .ok_or(GraphqlProjectionDescriptorError::InvokeBoundary)?;
        if actual_invoker != expected_invoker {
            return Err(GraphqlProjectionDescriptorError::SemanticInvokerMismatch);
        }

        return Ok(());
    }
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
    use serde_json::json;

    use super::*;
    use crate::{
        RpcClientAudience, RpcCodecSet, RpcOperationScope, RpcOperationSource, RpcPayloadCodec,
        RpcRequestShape, RpcResponseShape, RPC_OPERATION_CONTRACT_SCHEMA_VERSION,
        RPC_OPERATION_HTTP_PATH,
    };

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

    fn semantic_contract() -> RpcOperationContract {
        return RpcOperationContract {
            schema_version: RPC_OPERATION_CONTRACT_SCHEMA_VERSION,
            operation_key: "users.get_user".to_owned(),
            namespace: vec!["users".to_owned()],
            source: RpcOperationSource {
                route_file: Some("src/routes/rest/users/get_user/route.rs".to_owned()),
                handlers_file: Some("src/routes/rest/users/get_user/handlers.rs".to_owned()),
                http_handler: Some("get".to_owned()),
                operation: Some("get_user".to_owned()),
                invoker: Some("__ores_invoke_get_user".to_owned()),
                execution_model: "shared_operation".to_owned(),
                repository: None,
                commit_sha: None,
            },
            rpc_transport_path: RPC_OPERATION_HTTP_PATH,
            http: Some(crate::RpcHttpProjection {
                method: "GET".to_owned(),
                path: "/v1/users/{user_id}".to_owned(),
            }),
            scope: RpcOperationScope::Regular,
            stream: RpcStreamMode::Unary,
            audiences: vec![RpcClientAudience::Browser, RpcClientAudience::Server],
            codecs: RpcCodecSet {
                allowed: vec![RpcPayloadCodec::Json],
                default: RpcPayloadCodec::Json,
            },
            request: RpcRequestShape {
                body_schema: Some(json!({
                    "type": "object",
                    "properties": {"user_id": {"type": "string"}},
                    "required": ["user_id"],
                    "additionalProperties": false
                })),
                ..RpcRequestShape::default()
            },
            response: RpcResponseShape {
                body_schema: Some(json!({
                    "type": "object",
                    "properties": {"display_name": {"type": "string"}},
                    "required": ["display_name"],
                    "additionalProperties": false
                })),
                ..RpcResponseShape::default()
            },
            contract_sha256: "a".repeat(64),
        };
    }

    fn semantic_authority() -> SharedOperationSource {
        return SharedOperationSource {
            rust_name: "get_user".to_owned(),
            invoke_name: "__ores_invoke_get_user".to_owned(),
            spec: Some("crate::generated::UsersGetUserSpec".to_owned()),
            key: "users.get_user".to_owned(),
            codecs: vec!["json".to_owned()],
            default_codec: "json".to_owned(),
            audiences: vec!["browser".to_owned(), "server".to_owned()],
            scope: "regular".to_owned(),
            stream: "unary".to_owned(),
            parameter_types: vec!["TypedOperationContext<State, UsersGetUserSpec>".to_owned()],
            return_type: Some("Result<User, GetUserError>".to_owned()),
        };
    }

    fn semantic_binding(
        contract: &RpcOperationContract,
        authority: &SharedOperationSource,
    ) -> OperationSemanticBinding {
        return OperationSemanticBinding::from_rpc_contract(contract, authority)
            .expect("semantic binding");
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
    fn descriptor_binds_to_exact_semantic_operation() {
        let contract = semantic_contract();
        let authority = semantic_authority();
        query()
            .validate_against_semantic_binding(
                &semantic_binding(&contract, &authority),
                &contract,
                &authority,
            )
            .expect("GraphQL projection must bind to shared semantic operation");
    }

    #[test]
    fn semantic_operation_key_drift_fails_closed() {
        let contract = semantic_contract();
        let authority = semantic_authority();
        let mut descriptor = query();
        descriptor.operation_key = "users.other_user";
        assert_eq!(
            descriptor
                .validate_against_semantic_binding(
                    &semantic_binding(&contract, &authority),
                    &contract,
                    &authority,
                )
                .unwrap_err(),
            GraphqlProjectionDescriptorError::SemanticOperationKeyMismatch
        );
    }

    #[test]
    fn semantic_stream_drift_fails_closed() {
        let mut contract = semantic_contract();
        contract.stream = RpcStreamMode::ServerStream;
        let authority = semantic_authority();
        assert!(OperationSemanticBinding::from_rpc_contract(&contract, &authority).is_err());
    }

    #[test]
    fn generated_invoker_drift_fails_closed() {
        let contract = semantic_contract();
        let authority = semantic_authority();
        let binding = semantic_binding(&contract, &authority);
        let mut descriptor = query();
        descriptor.invoke = "crate::routes::rest::users::handlers::__ores_invoke_other_user";
        assert_eq!(
            descriptor
                .validate_against_semantic_binding(&binding, &contract, &authority)
                .unwrap_err(),
            GraphqlProjectionDescriptorError::SemanticInvokerMismatch
        );
    }

    #[test]
    fn operation_spec_or_callable_authority_drift_fails_closed() {
        let contract = semantic_contract();
        let authority = semantic_authority();
        let binding = semantic_binding(&contract, &authority);

        let mut wrong_spec = authority.clone();
        wrong_spec.spec = Some("crate::generated::OtherSpec".to_owned());
        assert!(query()
            .validate_against_semantic_binding(&binding, &contract, &wrong_spec)
            .is_err());

        let mut wrong_callable = authority;
        wrong_callable.parameter_types.push("Unexpected".to_owned());
        assert!(query()
            .validate_against_semantic_binding(&binding, &contract, &wrong_callable)
            .is_err());
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
    fn descriptor_rejects_persisted_operation_key_grammar_drift() {
        let mut descriptor = query();
        for invalid in ["users", "Users.get_user", "users..get_user", "users._get_user"] {
            descriptor.operation_key = invalid;
            assert_eq!(
                descriptor.validate().unwrap_err(),
                GraphqlProjectionDescriptorError::OperationKey
            );
        }
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
