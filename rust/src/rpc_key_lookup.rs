//! Canonical RPC identity lookup.
//!
//! Route maps retain an authored map key for compatibility and may additionally
//! declare a stable dotted `rpc_key` used on the wire. Semantic route-map
//! admission already guarantees `rpc_key` uniqueness; this helper makes every
//! RPC transport resolve that canonical identity the same way without changing
//! ordinary `RouteMap::lookup` semantics.

use crate::{RouteEntry, RouteMap};

#[must_use]
pub fn lookup_rpc_route<'a>(route_map: &'a RouteMap, key: &str) -> Option<&'a RouteEntry> {
    route_map.lookup(key).or_else(|| {
        route_map
            .map
            .values()
            .find(|entry| entry.rpc_key.as_deref() == Some(key))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_canonical_rpc_key_and_legacy_map_key() {
        let map = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"demo",
              "map":{
                "find_user_by_id":{
                  "path":"/v1/users/{id}",
                  "methods":["GET"],
                  "rpc_key":"demo.users.find_user"
                }
              }
            }"#,
        )
        .expect("route map");

        assert_eq!(
            lookup_rpc_route(&map, "demo.users.find_user").map(|route| route.path.as_str()),
            Some("/v1/users/{id}")
        );
        assert_eq!(
            lookup_rpc_route(&map, "find_user_by_id").map(|route| route.path.as_str()),
            Some("/v1/users/{id}")
        );
    }
}
