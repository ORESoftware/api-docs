//! Transport-neutral semantic identity bound to REST/RPC/GraphQL projections.
//!
//! A semantic operation is authored once. REST, canonical `/v1/rpc`, GraphQL,
//! and direct Lambda execution may expose that operation differently, but none
//! of those projections is allowed to redefine callable, contract, policy, or
//! wire-type identity.
//!
//! This module deliberately excludes source paths, source bytes, build hashes,
//! provider identity, and transport projection IDs from semantic identity.
//! Those facts belong to provenance/build receipts or
//! [`crate::TransportLeafBuildIdentity`].

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    RpcClientAudience, RpcOperationContract, RpcOperationScope, RpcStreamMode,
    project::sha256_hex,
};

pub const OPERATION_SEMANTIC_BINDING_SCHEMA_VERSION: u32 = 1;
pub const OPERATION_SEMANTIC_CONTRACT_SCHEMA: &str =
    "ores.api-docs.operation-semantic-contract.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OperationPolicyIdentity {
    pub scope: String,
    pub audiences: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct OperationTypeIdentity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_schema_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_schema_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_header_schema_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body_schema_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_header_schema_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_trailer_schema_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body_schema_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_schema_sha256: Option<String>,
}

/// Semantic binding copied into or referenced by every admitted transport
/// exposure.
///
/// `callable_id` is supplied by the callable-identity authority. It is not
/// derived here because source path, implementation bytes, transport, build
/// revision, and dependency closure must never accidentally enter that stable
/// public identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OperationSemanticBinding {
    pub schema_version: u32,
    pub operation_key: String,
    pub callable_id: String,
    /// Generated `OperationSpec` identity/path used for static type binding.
    /// This is a binding locator, not part of callable-id derivation.
    pub operation_spec: String,
    /// Digest for the complete normalized registry/catalog authority.
    pub registry_contract_sha256: String,
    /// Digest for this operation's transport-neutral semantic contract only.
    pub operation_contract_sha256: String,
    pub policy: OperationPolicyIdentity,
    pub types: OperationTypeIdentity,
}

impl OperationSemanticBinding {
    pub fn from_rpc_contract(
        contract: &RpcOperationContract,
        callable_id: impl Into<String>,
        operation_spec: impl Into<String>,
    ) -> Result<Self, String> {
        contract.validate()?;
        let binding = Self {
            schema_version: OPERATION_SEMANTIC_BINDING_SCHEMA_VERSION,
            operation_key: contract.operation_key.clone(),
            callable_id: callable_id.into(),
            operation_spec: operation_spec.into(),
            registry_contract_sha256: contract.contract_sha256.clone(),
            operation_contract_sha256: operation_semantic_contract_sha256(contract)?,
            policy: operation_policy_identity(contract)?,
            types: operation_type_identity(contract),
        };
        binding.validate()?;
        return Ok(binding);
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != OPERATION_SEMANTIC_BINDING_SCHEMA_VERSION {
            return Err(format!(
                "operation semantic binding schema_version must be {OPERATION_SEMANTIC_BINDING_SCHEMA_VERSION}, got {}",
                self.schema_version
            ));
        }
        require_non_empty("operation_key", &self.operation_key)?;
        require_non_empty("callable_id", &self.callable_id)?;
        require_non_empty("operation_spec", &self.operation_spec)?;
        require_sha256(
            "registry_contract_sha256",
            &self.registry_contract_sha256,
        )?;
        require_sha256(
            "operation_contract_sha256",
            &self.operation_contract_sha256,
        )?;
        if !matches!(self.policy.scope.as_str(), "regular" | "admin") {
            return Err(format!(
                "operation policy scope must be regular or admin, got {:?}",
                self.policy.scope
            ));
        }
        if self.policy.audiences.is_empty() {
            return Err("operation policy identity requires at least one audience".to_owned());
        }
        let mut audiences = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for audience in &self.policy.audiences {
            if !matches!(audience.as_str(), "browser" | "server") {
                return Err(format!(
                    "operation policy audience must be browser or server, got {audience:?}"
                ));
            }
            if previous.is_some_and(|value| value > audience.as_str()) {
                return Err("operation policy audiences must be emitted in canonical sorted order".to_owned());
            }
            previous = Some(audience.as_str());
            if !audiences.insert(audience.as_str()) {
                return Err(format!(
                    "operation policy identity repeats audience {audience:?}"
                ));
            }
        }
        if self.policy.scope == "admin" && audiences.contains("browser") {
            return Err("admin operations are server-only and may not expose browser audience".to_owned());
        }
        validate_type_identity(&self.types)?;
        return Ok(());
    }

    /// Recompute every semantic fact available from the normalized operation
    /// contract. Callers should run this before publishing REST/RPC/GraphQL
    /// exposure evidence so stale copied metadata fails closed.
    pub fn validate_against_contract(
        &self,
        contract: &RpcOperationContract,
    ) -> Result<(), String> {
        contract.validate()?;
        self.validate()?;
        if self.operation_key != contract.operation_key {
            return Err(format!(
                "operation semantic binding key {:?} disagrees with contract key {:?}",
                self.operation_key, contract.operation_key
            ));
        }
        if self.registry_contract_sha256 != contract.contract_sha256 {
            return Err(format!(
                "operation {:?} registry contract digest mismatch",
                self.operation_key
            ));
        }
        let expected_contract = operation_semantic_contract_sha256(contract)?;
        if self.operation_contract_sha256 != expected_contract {
            return Err(format!(
                "operation {:?} semantic contract digest mismatch",
                self.operation_key
            ));
        }
        let expected_policy = operation_policy_identity(contract)?;
        if self.policy != expected_policy {
            return Err(format!(
                "operation {:?} policy identity mismatch",
                self.operation_key
            ));
        }
        let expected_types = operation_type_identity(contract);
        if self.types != expected_types {
            return Err(format!(
                "operation {:?} wire-type identity mismatch",
                self.operation_key
            ));
        }
        return Ok(());
    }
}

/// Deterministic digest for the semantic operation independent of REST path,
/// GraphQL field, source path, repository revision, or execution provider.
pub fn operation_semantic_contract_sha256(
    contract: &RpcOperationContract,
) -> Result<String, String> {
    contract.validate()?;
    let policy = operation_policy_identity(contract)?;
    let mut codecs = contract
        .codecs
        .allowed
        .iter()
        .map(|codec| codec.as_str())
        .collect::<Vec<_>>();
    codecs.sort_unstable();
    reject_duplicate_values("RPC codec", &codecs)?;

    let value = json!({
        "schema": OPERATION_SEMANTIC_CONTRACT_SCHEMA,
        "operation_key": contract.operation_key,
        "scope": policy.scope,
        "audiences": policy.audiences,
        "stream": stream_name(contract.stream),
        "codecs": {
            "allowed": codecs,
            "default": contract.codecs.default.as_str(),
        },
        "request": contract.request,
        "response": contract.response,
    });
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| format!("serialize operation semantic contract: {error}"))?;
    return Ok(sha256_hex(&bytes));
}

#[must_use]
pub fn operation_type_identity(contract: &RpcOperationContract) -> OperationTypeIdentity {
    return OperationTypeIdentity {
        path_schema_sha256: schema_digest(contract.request.path_schema.as_ref()),
        query_schema_sha256: schema_digest(contract.request.query_schema.as_ref()),
        request_header_schema_sha256: schema_digest(contract.request.header_schema.as_ref()),
        request_body_schema_sha256: schema_digest(contract.request.body_schema.as_ref()),
        response_header_schema_sha256: schema_digest(contract.response.header_schema.as_ref()),
        response_trailer_schema_sha256: schema_digest(contract.response.trailer_schema.as_ref()),
        response_body_schema_sha256: schema_digest(contract.response.body_schema.as_ref()),
        error_schema_sha256: schema_digest(contract.response.error_schema.as_ref()),
    };
}

fn operation_policy_identity(
    contract: &RpcOperationContract,
) -> Result<OperationPolicyIdentity, String> {
    let scope = match contract.scope {
        RpcOperationScope::Regular => "regular",
        RpcOperationScope::Admin => "admin",
    }
    .to_owned();
    let mut audiences = contract
        .audiences
        .iter()
        .map(|audience| audience_name(*audience).to_owned())
        .collect::<Vec<_>>();
    audiences.sort();
    reject_duplicate_values("operation audience", &audiences)?;
    if audiences.is_empty() {
        return Err(format!(
            "operation {:?} must admit at least one client audience",
            contract.operation_key
        ));
    }
    if scope == "admin" && audiences.iter().any(|audience| audience == "browser") {
        return Err(format!(
            "operation {:?} is admin-scoped and must be server-only",
            contract.operation_key
        ));
    }
    return Ok(OperationPolicyIdentity { scope, audiences });
}

fn audience_name(audience: RpcClientAudience) -> &'static str {
    return match audience {
        RpcClientAudience::Browser => "browser",
        RpcClientAudience::Server => "server",
    };
}

fn stream_name(stream: RpcStreamMode) -> &'static str {
    return stream.as_str();
}

fn schema_digest(schema: Option<&Value>) -> Option<String> {
    return schema.map(|schema| {
        let bytes = serde_json::to_vec(schema).expect("JSON Schema value is JSON serializable");
        sha256_hex(&bytes)
    });
}

fn validate_type_identity(types: &OperationTypeIdentity) -> Result<(), String> {
    for (name, digest) in [
        ("path_schema_sha256", &types.path_schema_sha256),
        ("query_schema_sha256", &types.query_schema_sha256),
        (
            "request_header_schema_sha256",
            &types.request_header_schema_sha256,
        ),
        (
            "request_body_schema_sha256",
            &types.request_body_schema_sha256,
        ),
        (
            "response_header_schema_sha256",
            &types.response_header_schema_sha256,
        ),
        (
            "response_trailer_schema_sha256",
            &types.response_trailer_schema_sha256,
        ),
        (
            "response_body_schema_sha256",
            &types.response_body_schema_sha256,
        ),
        ("error_schema_sha256", &types.error_schema_sha256),
    ] {
        if let Some(digest) = digest {
            require_sha256(name, digest)?;
        }
    }
    return Ok(());
}

fn require_non_empty(name: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    return Ok(());
}

fn require_sha256(name: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(format!(
            "{name} must be a lowercase 64-character SHA-256 hex digest"
        ));
    }
    return Ok(());
}

fn reject_duplicate_values<T>(name: &str, values: &[T]) -> Result<(), String>
where
    T: Ord + std::fmt::Debug,
{
    for pair in values.windows(2) {
        if pair[0] == pair[1] {
            return Err(format!("{name} repeats value {:?}", pair[0]));
        }
    }
    return Ok(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RpcCodecSet, RpcOperationSource, RpcPayloadCodec, RpcRequestShape, RpcResponseShape,
        RPC_OPERATION_CONTRACT_SCHEMA_VERSION, RPC_OPERATION_HTTP_PATH,
    };

    fn contract() -> RpcOperationContract {
        return RpcOperationContract {
            schema_version: RPC_OPERATION_CONTRACT_SCHEMA_VERSION,
            operation_key: "users.get_user".to_owned(),
            namespace: vec!["users".to_owned()],
            source: RpcOperationSource {
                route_file: None,
                handlers_file: Some("src/rpc/users/get_user/funcs.rs".to_owned()),
                http_handler: None,
                operation: Some("get_user".to_owned()),
                invoker: Some("__ores_invoke_get_user".to_owned()),
                execution_model: "shared_operation".to_owned(),
                repository: None,
                commit_sha: None,
            },
            rpc_transport_path: RPC_OPERATION_HTTP_PATH,
            http: None,
            scope: RpcOperationScope::Regular,
            stream: RpcStreamMode::Unary,
            audiences: vec![RpcClientAudience::Server, RpcClientAudience::Browser],
            codecs: RpcCodecSet {
                allowed: vec![RpcPayloadCodec::Json, RpcPayloadCodec::Messagepack],
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
                error_schema: Some(json!({
                    "type": "object",
                    "properties": {"code": {"type": "string"}},
                    "required": ["code"],
                    "additionalProperties": false
                })),
                ..RpcResponseShape::default()
            },
            contract_sha256: "a".repeat(64),
        };
    }

    #[test]
    fn transport_and_source_moves_do_not_change_semantic_contract_identity() {
        let left = contract();
        let mut right = contract();
        right.source.handlers_file = Some("src/rpc/users/v2/get_user/funcs.rs".to_owned());
        right.source.repository = Some("moved/repository".to_owned());
        right.source.commit_sha = Some("deadbeef".to_owned());
        right.http = Some(crate::RpcHttpProjection {
            method: "GET".to_owned(),
            path: "/v2/users/{user_id}".to_owned(),
        });
        right.source.route_file = Some("src/routes/rest/users/get_user/route.rs".to_owned());
        right.source.http_handler = Some("get".to_owned());

        assert_eq!(
            operation_semantic_contract_sha256(&left).expect("left digest"),
            operation_semantic_contract_sha256(&right).expect("right digest")
        );
    }

    #[test]
    fn policy_and_wire_type_changes_change_semantic_identity() {
        let baseline = contract();
        let baseline_digest =
            operation_semantic_contract_sha256(&baseline).expect("baseline digest");

        let mut policy_changed = contract();
        policy_changed.scope = RpcOperationScope::Admin;
        policy_changed.audiences = vec![RpcClientAudience::Server];
        assert_ne!(
            baseline_digest,
            operation_semantic_contract_sha256(&policy_changed).expect("policy digest")
        );

        let mut type_changed = contract();
        type_changed.response.body_schema = Some(json!({"type": "string"}));
        assert_ne!(
            baseline_digest,
            operation_semantic_contract_sha256(&type_changed).expect("type digest")
        );
        assert_ne!(
            operation_type_identity(&baseline).response_body_schema_sha256,
            operation_type_identity(&type_changed).response_body_schema_sha256
        );
    }

    #[test]
    fn admin_browser_exposure_fails_closed() {
        let mut contract = contract();
        contract.scope = RpcOperationScope::Admin;
        assert!(operation_semantic_contract_sha256(&contract).is_err());
    }

    #[test]
    fn binding_fails_closed_on_stale_contract_policy_or_type_evidence() {
        let contract = contract();
        let binding = OperationSemanticBinding::from_rpc_contract(
            &contract,
            "users_get_user_a1_0123456789abcdef0123",
            "crate::generated::UsersGetUserSpec",
        )
        .expect("binding");
        binding
            .validate_against_contract(&contract)
            .expect("exact binding");

        let mut stale = binding.clone();
        stale.types.response_body_schema_sha256 = Some("b".repeat(64));
        assert!(stale.validate_against_contract(&contract).is_err());

        let mut stale = binding.clone();
        stale.policy.scope = "admin".to_owned();
        assert!(stale.validate_against_contract(&contract).is_err());

        let mut stale = binding;
        stale.operation_contract_sha256 = "c".repeat(64);
        assert!(stale.validate_against_contract(&contract).is_err());
    }

    #[test]
    fn malformed_public_identity_is_rejected() {
        let contract = contract();
        assert!(OperationSemanticBinding::from_rpc_contract(
            &contract,
            "",
            "crate::generated::UsersGetUserSpec"
        )
        .is_err());

        let mut malformed = contract;
        malformed.contract_sha256 = "ABC".to_owned();
        assert!(OperationSemanticBinding::from_rpc_contract(
            &malformed,
            "users_get_user_a1_0123456789abcdef0123",
            "crate::generated::UsersGetUserSpec"
        )
        .is_err());
    }

    #[test]
    fn audience_order_is_not_semantic() {
        let left = contract();
        let mut right = contract();
        right.audiences.reverse();
        assert_eq!(
            operation_semantic_contract_sha256(&left).expect("left"),
            operation_semantic_contract_sha256(&right).expect("right")
        );
    }
}
