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
use serde_json::{json, Value};

use crate::{
    project::sha256_hex, RpcClientAudience, RpcOperationContract, RpcOperationScope, RpcStreamMode,
    SharedOperationSource,
};

pub const OPERATION_SEMANTIC_BINDING_SCHEMA_VERSION: u32 = 1;
pub const OPERATION_SEMANTIC_CONTRACT_SCHEMA: &str = "ores.api-docs.operation-semantic-contract.v1";
pub const OPERATION_CALLABLE_ID_SCHEMA: &str = "ores.api-docs.operation-callable-identity.v1";
pub const OPERATION_CALLABLE_ID_PREFIX: &str = "ores-callable-v1:";

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
/// `callable_id` and `operation_spec` are derived from the parsed
/// `#[ores_operation]` authority. Callers never supply them independently.
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
    /// Build one semantic binding from the normalized contract and the parsed
    /// authored operation that produced it.
    ///
    /// This deliberately does not accept caller-provided callable/spec strings:
    /// those identities must come from `#[ores_operation]` source analysis.
    pub fn from_rpc_contract(
        contract: &RpcOperationContract,
        authority: &SharedOperationSource,
    ) -> Result<Self, String> {
        validate_shared_operation_authority(contract, authority)?;
        let operation_spec = authority
            .spec
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "operation {:?} requires #[ores_operation(spec = ...)] for semantic exposure",
                    contract.operation_key
                )
            })?
            .to_owned();
        let binding = Self {
            schema_version: OPERATION_SEMANTIC_BINDING_SCHEMA_VERSION,
            operation_key: contract.operation_key.clone(),
            callable_id: operation_callable_id(authority)?,
            operation_spec,
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
        validate_operation_key(&self.operation_key)?;
        validate_callable_id(&self.callable_id)?;
        require_non_empty("operation_spec", &self.operation_spec)?;
        require_sha256("registry_contract_sha256", &self.registry_contract_sha256)?;
        require_sha256("operation_contract_sha256", &self.operation_contract_sha256)?;
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
                return Err(
                    "operation policy audiences must be emitted in canonical sorted order"
                        .to_owned(),
                );
            }
            previous = Some(audience.as_str());
            if !audiences.insert(audience.as_str()) {
                return Err(format!(
                    "operation policy identity repeats audience {audience:?}"
                ));
            }
        }
        if self.policy.scope == "admin" && audiences.contains("browser") {
            return Err(
                "admin operations are server-only and may not expose browser audience".to_owned(),
            );
        }
        validate_type_identity(&self.types)?;

        return Ok(());
    }

    /// Recompute every semantic fact from both normalized contract and authored
    /// operation authority. Transport projections must call this before
    /// publication so copied/stale identity cannot survive independently.
    pub fn validate_against_authority(
        &self,
        contract: &RpcOperationContract,
        authority: &SharedOperationSource,
    ) -> Result<(), String> {
        validate_shared_operation_authority(contract, authority)?;
        self.validate()?;
        if self.operation_key != contract.operation_key {
            return Err(format!(
                "operation semantic binding key {:?} disagrees with contract key {:?}",
                self.operation_key, contract.operation_key
            ));
        }
        let expected_callable_id = operation_callable_id(authority)?;
        if self.callable_id != expected_callable_id {
            return Err(format!(
                "operation {:?} callable identity mismatch",
                self.operation_key
            ));
        }
        let expected_spec = authority
            .spec
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "operation {:?} has no authoritative OperationSpec",
                    self.operation_key
                )
            })?;
        if self.operation_spec != expected_spec {
            return Err(format!(
                "operation {:?} OperationSpec identity mismatch",
                self.operation_key
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

/// Validate the authored/persisted operation-key grammar exactly:
/// `^[a-z0-9]+(?:[._-][a-z0-9]+)+$`.
///
/// Dot, underscore, and hyphen are single separators. Empty segments,
/// repeated/mixed adjacent separators, uppercase characters, whitespace, and
/// keys without a separator all fail closed.
pub fn validate_operation_key(key: &str) -> Result<(), String> {
    let mut saw_separator = false;
    let mut segment_length = 0_usize;

    for byte in key.bytes() {
        if matches!(byte, b'.' | b'_' | b'-') {
            if segment_length == 0 {
                return Err(format!(
                    "operation_key {key:?} must match ^[a-z0-9]+(?:[._-][a-z0-9]+)+$"
                ));
            }
            saw_separator = true;
            segment_length = 0;
            continue;
        }
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit()) {
            return Err(format!(
                "operation_key {key:?} must match ^[a-z0-9]+(?:[._-][a-z0-9]+)+$"
            ));
        }
        segment_length += 1;
    }

    if !saw_separator || segment_length == 0 {
        return Err(format!(
            "operation_key {key:?} must match ^[a-z0-9]+(?:[._-][a-z0-9]+)+$"
        ));
    }

    return Ok(());
}

/// Stable callable ABI identity derived from authored semantic function shape,
/// never source location, source bytes, transport, or provider/build identity.
pub fn operation_callable_id(authority: &SharedOperationSource) -> Result<String, String> {
    validate_operation_key(&authority.key)?;
    require_non_empty("operation rust_name", &authority.rust_name)?;
    if authority.invoke_name != format!("__ores_invoke_{}", authority.rust_name) {
        return Err(format!(
            "operation {:?} generated invoker {:?} does not match semantic function {:?}",
            authority.key, authority.invoke_name, authority.rust_name
        ));
    }

    let value = json!({
        "schema": OPERATION_CALLABLE_ID_SCHEMA,
        "operation_key": authority.key,
        "rust_name": authority.rust_name,
        "parameter_types": authority.parameter_types,
        "return_type": authority.return_type,
    });
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| format!("serialize operation callable identity: {error}"))?;

    return Ok(format!(
        "{OPERATION_CALLABLE_ID_PREFIX}{}",
        sha256_hex(&bytes)
    ));
}

/// Deterministic digest for the semantic operation independent of REST path,
/// GraphQL field, source path, repository revision, or execution provider.
pub fn operation_semantic_contract_sha256(
    contract: &RpcOperationContract,
) -> Result<String, String> {
    contract.validate()?;
    validate_operation_key(&contract.operation_key)?;
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

fn validate_shared_operation_authority(
    contract: &RpcOperationContract,
    authority: &SharedOperationSource,
) -> Result<(), String> {
    contract.validate()?;
    validate_operation_key(&contract.operation_key)?;
    validate_operation_key(&authority.key)?;

    if authority.key != contract.operation_key {
        return Err(format!(
            "authored operation key {:?} disagrees with normalized contract key {:?}",
            authority.key, contract.operation_key
        ));
    }
    let contract_operation = contract
        .source
        .operation
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "operation {:?} normalized contract has no semantic function identity",
                contract.operation_key
            )
        })?;
    if contract_operation != authority.rust_name {
        return Err(format!(
            "operation {:?} semantic function {:?} disagrees with authored {:?}",
            contract.operation_key, contract_operation, authority.rust_name
        ));
    }
    let contract_invoker = contract
        .source
        .invoker
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "operation {:?} normalized contract has no generated invoker identity",
                contract.operation_key
            )
        })?;
    if contract_invoker != authority.invoke_name {
        return Err(format!(
            "operation {:?} generated invoker {:?} disagrees with authored analysis {:?}",
            contract.operation_key, contract_invoker, authority.invoke_name
        ));
    }
    if authority.invoke_name != format!("__ores_invoke_{}", authority.rust_name) {
        return Err(format!(
            "operation {:?} generated invoker is not derived from semantic function {:?}",
            contract.operation_key, authority.rust_name
        ));
    }
    if authority
        .spec
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err(format!(
            "operation {:?} requires an authored OperationSpec binding",
            contract.operation_key
        ));
    }
    if authority.stream != stream_name(contract.stream) {
        return Err(format!(
            "operation {:?} stream {:?} disagrees with authored {:?}",
            contract.operation_key,
            stream_name(contract.stream),
            authority.stream
        ));
    }
    let expected_scope = match contract.scope {
        RpcOperationScope::Regular => "regular",
        RpcOperationScope::Admin => "admin",
    };
    if authority.scope != expected_scope {
        return Err(format!(
            "operation {:?} scope {expected_scope:?} disagrees with authored {:?}",
            contract.operation_key, authority.scope
        ));
    }

    let mut expected_audiences = contract
        .audiences
        .iter()
        .map(|audience| audience_name(*audience).to_owned())
        .collect::<Vec<_>>();
    expected_audiences.sort();
    let mut authored_audiences = authority.audiences.clone();
    authored_audiences.sort();
    reject_duplicate_values("authored operation audience", &authored_audiences)?;
    if expected_audiences != authored_audiences {
        return Err(format!(
            "operation {:?} audiences {:?} disagree with authored {:?}",
            contract.operation_key, expected_audiences, authored_audiences
        ));
    }

    let mut expected_codecs = contract
        .codecs
        .allowed
        .iter()
        .map(|codec| codec.as_str().to_owned())
        .collect::<Vec<_>>();
    expected_codecs.sort();
    let mut authored_codecs = authority.codecs.clone();
    authored_codecs.sort();
    reject_duplicate_values("authored operation codec", &authored_codecs)?;
    if expected_codecs != authored_codecs {
        return Err(format!(
            "operation {:?} codecs {:?} disagree with authored {:?}",
            contract.operation_key, expected_codecs, authored_codecs
        ));
    }
    if contract.codecs.default.as_str() != authority.default_codec {
        return Err(format!(
            "operation {:?} default codec {:?} disagrees with authored {:?}",
            contract.operation_key,
            contract.codecs.default.as_str(),
            authority.default_codec
        ));
    }

    return Ok(());
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

fn validate_callable_id(value: &str) -> Result<(), String> {
    let digest = value.strip_prefix(OPERATION_CALLABLE_ID_PREFIX).ok_or_else(|| {
        format!(
            "callable_id must start with {OPERATION_CALLABLE_ID_PREFIX:?} and carry a SHA-256 digest"
        )
    })?;
    return require_sha256("callable_id digest", digest);
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

    fn authority() -> SharedOperationSource {
        return SharedOperationSource {
            rust_name: "get_user".to_owned(),
            invoke_name: "__ores_invoke_get_user".to_owned(),
            spec: Some("crate::generated::UsersGetUserSpec".to_owned()),
            key: "users.get_user".to_owned(),
            codecs: vec!["json".to_owned(), "messagepack".to_owned()],
            default_codec: "json".to_owned(),
            audiences: vec!["server".to_owned(), "browser".to_owned()],
            scope: "regular".to_owned(),
            stream: "unary".to_owned(),
            parameter_types: vec!["TypedOperationContext<State, UsersGetUserSpec>".to_owned()],
            return_type: Some("Result<User, GetUserError>".to_owned()),
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
            response_framing: crate::HttpResponseFraming::Single,
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
    fn binding_is_derived_from_authored_callable_and_spec_authority() {
        let contract = contract();
        let authority = authority();
        let binding =
            OperationSemanticBinding::from_rpc_contract(&contract, &authority).expect("binding");
        binding
            .validate_against_authority(&contract, &authority)
            .expect("exact binding");
        assert_eq!(binding.operation_spec, "crate::generated::UsersGetUserSpec");
        assert!(binding
            .callable_id
            .starts_with(OPERATION_CALLABLE_ID_PREFIX));
    }

    #[test]
    fn binding_fails_closed_on_stale_contract_policy_type_callable_or_spec_evidence() {
        let contract = contract();
        let authority = authority();
        let binding =
            OperationSemanticBinding::from_rpc_contract(&contract, &authority).expect("binding");

        let mut stale = binding.clone();
        stale.types.response_body_schema_sha256 = Some("b".repeat(64));
        assert!(stale
            .validate_against_authority(&contract, &authority)
            .is_err());

        let mut stale = binding.clone();
        stale.policy.scope = "admin".to_owned();
        assert!(stale
            .validate_against_authority(&contract, &authority)
            .is_err());

        let mut stale = binding.clone();
        stale.operation_contract_sha256 = "c".repeat(64);
        assert!(stale
            .validate_against_authority(&contract, &authority)
            .is_err());

        let mut stale = binding.clone();
        stale.callable_id = format!("{OPERATION_CALLABLE_ID_PREFIX}{}", "d".repeat(64));
        assert!(stale
            .validate_against_authority(&contract, &authority)
            .is_err());

        let mut stale = binding;
        stale.operation_spec = "crate::generated::OtherSpec".to_owned();
        assert!(stale
            .validate_against_authority(&contract, &authority)
            .is_err());
    }

    #[test]
    fn authored_authority_drift_fails_closed() {
        let contract = contract();

        let mut wrong_spec = authority();
        wrong_spec.spec = None;
        assert!(OperationSemanticBinding::from_rpc_contract(&contract, &wrong_spec).is_err());

        let mut wrong_function = authority();
        wrong_function.rust_name = "other_user".to_owned();
        assert!(OperationSemanticBinding::from_rpc_contract(&contract, &wrong_function).is_err());

        let mut wrong_invoker = authority();
        wrong_invoker.invoke_name = "__ores_invoke_other_user".to_owned();
        assert!(OperationSemanticBinding::from_rpc_contract(&contract, &wrong_invoker).is_err());
    }

    #[test]
    fn operation_key_grammar_matches_persisted_schema() {
        for valid in [
            "users.get_user",
            "users-get-user",
            "users_get_user",
            "v1.users.get2",
        ] {
            validate_operation_key(valid).expect("valid key");
        }
        for invalid in [
            "users",
            "Users.get_user",
            "users..get_user",
            ".users.get_user",
            "users.get_user.",
            "users._get_user",
            "users.-get_user",
            "users get_user",
        ] {
            assert!(
                validate_operation_key(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
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
