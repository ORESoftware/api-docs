//! Client-side module identity for RPC publication.
//!
//! The public RPC wire key is deliberately independent from server source
//! provenance. A callable may be generated from a REST leaf or authored as an
//! RPC-native leaf; clients still address it through the same stable dotted
//! `operation_key`. Provenance is retained separately for conformance,
//! diagnostics and server-leaf routing.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcPublicationOrigin {
    /// RPC publication generated from an admitted `src/routes/rest/**` leaf.
    RestGenerated,
    /// RPC-native/custom publication authored under `src/rpc/**`.
    RpcNative,
}

impl RpcPublicationOrigin {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RestGenerated => "rest_generated",
            Self::RpcNative => "rpc_native",
        }
    }
}

/// Language-neutral operation module identity.
///
/// `module_segments` excludes the service/product prefix from the wire key so a
/// service package can expose compact imports such as `rpc/users/get_user`
/// instead of repeating its own package name. `origin` never participates in
/// the public module path; it is provenance metadata only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcClientModulePath {
    pub operation_key: String,
    pub origin: RpcPublicationOrigin,
    pub namespace: Vec<String>,
    pub operation_name: String,
    pub module_segments: Vec<String>,
}

impl RpcClientModulePath {
    pub fn from_operation_key(
        operation_key: impl Into<String>,
        origin: RpcPublicationOrigin,
    ) -> Result<Self, String> {
        let operation_key = operation_key.into();
        let mut wire = operation_key
            .split('.')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if wire.len() < 2 {
            return Err(format!(
                "RPC operation key {operation_key:?} must contain at least service.operation"
            ));
        }
        if wire.iter().any(|segment| !portable_segment(segment)) {
            return Err(format!(
                "RPC operation key {operation_key:?} must use lowercase snake_case segments"
            ));
        }

        let operation_name = wire.pop().expect("length checked");
        // The service/product prefix remains part of wire identity but is
        // redundant beneath that service's generated client package.
        wire.remove(0);
        let namespace = wire;
        let mut module_segments = namespace.clone();
        module_segments.push(operation_name.clone());

        Ok(Self {
            operation_key,
            origin,
            namespace,
            operation_name,
            module_segments,
        })
    }

    /// Namespace prefixes for selective subtree entrypoints, shallow to deep.
    ///
    /// `demo.admin.users.get_user` => `[admin]`, `[admin, users]`.
    #[must_use]
    pub fn subtree_prefixes(&self) -> Vec<Vec<String>> {
        (1..=self.namespace.len())
            .map(|length| self.namespace[..length].to_vec())
            .collect()
    }

    #[must_use]
    pub fn import_path(&self, separator: &str) -> String {
        self.module_segments.join(separator)
    }

    #[must_use]
    pub fn belongs_to_subtree(&self, subtree: &[String]) -> bool {
        subtree.len() <= self.namespace.len() && self.namespace[..subtree.len()] == *subtree
    }
}

fn portable_segment(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_does_not_change_client_module_path() {
        let generated = RpcClientModulePath::from_operation_key(
            "demo.users.get_user",
            RpcPublicationOrigin::RestGenerated,
        )
        .unwrap();
        let native = RpcClientModulePath::from_operation_key(
            "demo.users.get_user",
            RpcPublicationOrigin::RpcNative,
        )
        .unwrap();

        assert_eq!(generated.module_segments, vec!["users", "get_user"]);
        assert_eq!(generated.module_segments, native.module_segments);
        assert_ne!(generated.origin, native.origin);
    }

    #[test]
    fn subtree_prefixes_allow_small_entrypoints() {
        let path = RpcClientModulePath::from_operation_key(
            "demo.admin.users.get_user",
            RpcPublicationOrigin::RpcNative,
        )
        .unwrap();
        assert_eq!(
            path.subtree_prefixes(),
            vec![
                vec!["admin".to_owned()],
                vec!["admin".to_owned(), "users".to_owned()]
            ]
        );
        assert_eq!(path.import_path("/"), "admin/users/get_user");
    }

    #[test]
    fn subtree_membership_is_prefix_based_not_root_wide() {
        let users = RpcClientModulePath::from_operation_key(
            "demo.users.get_user",
            RpcPublicationOrigin::RestGenerated,
        )
        .unwrap();
        assert!(users.belongs_to_subtree(&["users".to_owned()]));
        assert!(!users.belongs_to_subtree(&["billing".to_owned()]));
    }

    #[test]
    fn invalid_or_nonportable_keys_fail_closed() {
        assert!(RpcClientModulePath::from_operation_key(
            "get_user",
            RpcPublicationOrigin::RpcNative
        )
        .is_err());
        assert!(RpcClientModulePath::from_operation_key(
            "demo.Users.get_user",
            RpcPublicationOrigin::RpcNative
        )
        .is_err());
    }
}
