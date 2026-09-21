//! Deterministic transport-specific ABI identity for separately deployable API leaves.
//!
//! This identity is deliberately **not** a source-content hash. `ores-stack` owns
//! filesystem/Cargo dependency hashing. This module only captures ABI/projection
//! facts that must participate in a leaf build key so dev cannot accidentally
//! reuse a REST executable for RPC/GraphQL (or vice versa).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const TRANSPORT_LEAF_BUILD_IDENTITY_SCHEMA: &str =
    "ores.api-docs.transport-leaf-build-identity.v1";
pub const TRANSPORT_LEAF_ABI_VERSION: &str = "ores.api-docs.transport-leaf-abi.v1";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportLeafKind {
    Rest,
    Rpc,
    Graphql,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportLeafStreamMode {
    Unary,
    ServerStream,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphqlLeafKind {
    Query,
    Mutation,
    Subscription,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportLeafBuildIdentity {
    pub schema: String,
    pub abi_version: String,
    pub transport: TransportLeafKind,
    pub operation_key: String,
    pub callable_id: String,
    pub projection_id: String,
    pub stream_mode: TransportLeafStreamMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql_kind: Option<GraphqlLeafKind>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capabilities: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum TransportLeafBuildIdentityError {
    #[error("transport leaf operation_key must not be empty")]
    EmptyOperationKey,
    #[error("transport leaf callable_id must not be empty")]
    EmptyCallableId,
    #[error("transport leaf projection_id must not be empty")]
    EmptyProjectionId,
    #[error("graphql_kind is only valid for graphql transport leaves")]
    GraphqlKindOnNonGraphql,
    #[error("graphql transport leaves require graphql_kind")]
    MissingGraphqlKind,
    #[error("graphql query/mutation leaves must use unary stream mode")]
    GraphqlUnaryKindRequiresUnary,
    #[error("graphql subscription leaves must use server_stream mode")]
    GraphqlSubscriptionRequiresServerStream,
}

impl TransportLeafBuildIdentity {
    pub fn new(
        transport: TransportLeafKind,
        operation_key: impl Into<String>,
        callable_id: impl Into<String>,
        projection_id: impl Into<String>,
        stream_mode: TransportLeafStreamMode,
        graphql_kind: Option<GraphqlLeafKind>,
    ) -> Result<Self, TransportLeafBuildIdentityError> {
        let identity = Self {
            schema: TRANSPORT_LEAF_BUILD_IDENTITY_SCHEMA.to_owned(),
            abi_version: TRANSPORT_LEAF_ABI_VERSION.to_owned(),
            transport,
            operation_key: operation_key.into(),
            callable_id: callable_id.into(),
            projection_id: projection_id.into(),
            stream_mode,
            graphql_kind,
            capabilities: BTreeMap::new(),
        };
        identity.validate()?;
        Ok(identity)
    }

    pub fn with_capability(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.capabilities.insert(key.into(), value.into());
        self
    }

    pub fn validate(&self) -> Result<(), TransportLeafBuildIdentityError> {
        if self.operation_key.is_empty() {
            return Err(TransportLeafBuildIdentityError::EmptyOperationKey);
        }
        if self.callable_id.is_empty() {
            return Err(TransportLeafBuildIdentityError::EmptyCallableId);
        }
        if self.projection_id.is_empty() {
            return Err(TransportLeafBuildIdentityError::EmptyProjectionId);
        }

        match (self.transport, self.graphql_kind, self.stream_mode) {
            (TransportLeafKind::Graphql, None, _) => {
                Err(TransportLeafBuildIdentityError::MissingGraphqlKind)
            }
            (
                TransportLeafKind::Graphql,
                Some(GraphqlLeafKind::Subscription),
                TransportLeafStreamMode::Unary,
            ) => Err(TransportLeafBuildIdentityError::GraphqlSubscriptionRequiresServerStream),
            (
                TransportLeafKind::Graphql,
                Some(GraphqlLeafKind::Query | GraphqlLeafKind::Mutation),
                TransportLeafStreamMode::ServerStream,
            ) => Err(TransportLeafBuildIdentityError::GraphqlUnaryKindRequiresUnary),
            (TransportLeafKind::Rest | TransportLeafKind::Rpc, Some(_), _) => {
                Err(TransportLeafBuildIdentityError::GraphqlKindOnNonGraphql)
            }
            _ => Ok(()),
        }
    }

    /// Stable JSON bytes suitable as an input to `ores-stack`'s leaf build hash.
    ///
    /// Field order is the struct declaration order and `capabilities` uses
    /// `BTreeMap`, so semantically identical identities serialize identically.
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rest() -> TransportLeafBuildIdentity {
        TransportLeafBuildIdentity::new(
            TransportLeafKind::Rest,
            "users.create",
            "callable-users-create-2",
            "POST /v1/users",
            TransportLeafStreamMode::Unary,
            None,
        )
        .unwrap()
    }

    #[test]
    fn same_semantic_operation_has_distinct_transport_identities() {
        let rest = rest();
        let rpc = TransportLeafBuildIdentity::new(
            TransportLeafKind::Rpc,
            "users.create",
            "callable-users-create-2",
            "/v1/rpc:users.create",
            TransportLeafStreamMode::Unary,
            None,
        )
        .unwrap();
        let graphql = TransportLeafBuildIdentity::new(
            TransportLeafKind::Graphql,
            "users.create",
            "callable-users-create-2",
            "/v1/graphql:mutation:createUser",
            TransportLeafStreamMode::Unary,
            Some(GraphqlLeafKind::Mutation),
        )
        .unwrap();

        assert_ne!(
            rest.canonical_json().unwrap(),
            rpc.canonical_json().unwrap()
        );
        assert_ne!(
            rest.canonical_json().unwrap(),
            graphql.canonical_json().unwrap()
        );
        assert_ne!(
            rpc.canonical_json().unwrap(),
            graphql.canonical_json().unwrap()
        );
    }

    #[test]
    fn stream_mode_changes_identity() {
        let unary = rest();
        let mut streaming = unary.clone();
        streaming.stream_mode = TransportLeafStreamMode::ServerStream;
        assert_ne!(
            unary.canonical_json().unwrap(),
            streaming.canonical_json().unwrap()
        );
    }

    #[test]
    fn graphql_stream_shape_is_fail_closed() {
        assert_eq!(
            TransportLeafBuildIdentity::new(
                TransportLeafKind::Graphql,
                "events.watch",
                "callable-events-watch-1",
                "/v1/graphql:subscription:watchEvents",
                TransportLeafStreamMode::Unary,
                Some(GraphqlLeafKind::Subscription),
            )
            .unwrap_err(),
            TransportLeafBuildIdentityError::GraphqlSubscriptionRequiresServerStream
        );
        assert_eq!(
            TransportLeafBuildIdentity::new(
                TransportLeafKind::Graphql,
                "users.find",
                "callable-users-find-1",
                "/v1/graphql:query:user",
                TransportLeafStreamMode::ServerStream,
                Some(GraphqlLeafKind::Query),
            )
            .unwrap_err(),
            TransportLeafBuildIdentityError::GraphqlUnaryKindRequiresUnary
        );
    }

    #[test]
    fn capability_order_does_not_change_canonical_identity() {
        let a = rest()
            .with_capability("zeta", "1")
            .with_capability("alpha", "2");
        let b = rest()
            .with_capability("alpha", "2")
            .with_capability("zeta", "1");
        assert_eq!(a.canonical_json().unwrap(), b.canonical_json().unwrap());
    }

    #[test]
    fn source_bytes_are_intentionally_not_part_of_abi_identity() {
        let json = rest().canonical_json().unwrap();
        assert!(!json.contains("source_sha"));
        assert!(!json.contains("source_path"));
        assert!(!json.contains("implementation"));
    }
}
