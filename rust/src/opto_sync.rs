//! opto-sync document envelope for a route map.
//!
//! This crate **uses** opto-sync as a distribution mechanism. The opto-sync
//! repositories must **not** depend on ores-api-docs. Consumers pass these
//! envelopes through `opto-sync-client::reconcile` (or the TS/Dart clients)
//! with the defaults below.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::map::RouteMap;
use crate::schema::{validate_opto_sync_envelope, SchemaError};

/// Collection / scope name. Stable across languages.
pub const SCOPE: &str = "ores.api-docs.route-map";
pub const KIND: &str = "ores.api-docs.route-map";

/// Match opto-sync-client defaults so route-map replicas converge.
pub const ARRAY_MATCH_KEYS: &str = "id";
pub const LWW_KEYS: &str = "updatedAt,syncedAt";
pub const FWW_KEYS: &str = "";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteMapEnvelope {
    pub id: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub record_id: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "syncedAt", default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<String>,
    pub payload: Value,
}

impl RouteMapEnvelope {
    pub fn wrap(map: &RouteMap, updated_at: impl Into<String>) -> Result<Self, SchemaError> {
        let record_id = map.service.clone();
        let payload = serde_json::to_value(map).map_err(|e| SchemaError::Instance {
            name: "opto-sync-envelope",
            detail: e.to_string(),
        })?;
        let env = Self {
            id: record_id.clone(),
            scope: SCOPE.into(),
            kind: Some(KIND.into()),
            record_id,
            updated_at: updated_at.into(),
            synced_at: None,
            payload,
        };
        validate_opto_sync_envelope(&serde_json::to_value(&env).map_err(|e| {
            SchemaError::Instance {
                name: "opto-sync-envelope",
                detail: e.to_string(),
            }
        })?)?;
        Ok(env)
    }

    pub fn into_map(self) -> Result<RouteMap, crate::map::MapError> {
        if self.scope != SCOPE {
            return Err(crate::map::MapError::Semantic(format!(
                "opto-sync scope {} != {SCOPE}",
                self.scope
            )));
        }
        RouteMap::from_value(self.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_example() {
        let map = RouteMap::from_json_str(include_str!("../../examples/pmap-api.route-map.json"))
            .unwrap();
        let env = RouteMapEnvelope::wrap(&map, "1689940800123456789").unwrap();
        assert_eq!(env.scope, SCOPE);
        assert_eq!(env.id, "pmap-api-server");
        let back = env.into_map().unwrap();
        assert_eq!(back.lookup("healthz").unwrap().path, "/healthz");
        assert!(back.lookup("get_matter").unwrap().query_schema.is_some());
    }
}
