//! Route-map API documentation.
//!
//! The interchange contract is a JSON object whose **keys are operations** and
//! whose **values are HTTP routes**. Languages may author those keys as
//! annotations, param types, return types, function types, or a mix; they
//! serialize to the same map.
//!
//! The map is projected into several standards closely (not one of them
//! perfectly): OpenAPI 3.1, JSON Schema 2020-12, Connect JSON unary, OpenRPC
//! 1.3, and JSON Hyper-Schema links. Every projection is checked with JSON
//! Schema.

pub mod binding;
pub mod call;
pub mod catalog;
pub mod headers;
pub mod html;
pub mod infer;
pub mod map;
pub mod opto_sync;
pub mod paths;
pub mod project;
pub mod schema;
pub mod telemetry;
pub mod template;

#[cfg(feature = "axum")]
pub mod axum_router;

pub use binding::{RouteBinding, RpcHttp, RpcMethod, RpcTransport, UnaryFn};
pub use call::{
    encode_length_prefixed, split_length_prefixed, RpcCall, RpcReceipt, Transport, MAX_FRAME_BYTES,
};
pub use catalog::Catalog;
pub use map::{OptoSyncQueue, RouteEntry, RouteMap};
pub use opto_sync::{RouteMapEnvelope, SCOPE as OPTO_SYNC_SCOPE};
pub use telemetry::{TelemetryAttributes, RPC_SYSTEM};
pub use template::{expand_path, path_template_vars, QueryValue};

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const GENERATED_BY: &str = "ores-api-docs";

#[cfg(test)]
#[path = "../../generated/rust/src/pmap_api.rs"]
mod generated_pmap_api;

#[cfg(test)]
#[path = "../../generated/rust/src/canonical_api.rs"]
mod generated_canonical_api;

#[cfg(test)]
#[path = "../../generated/rust/src/chptr_api.rs"]
mod generated_chptr_api;

#[cfg(test)]
#[path = "../../generated/rust/src/cliptown_api.rs"]
mod generated_cliptown_api;

#[cfg(test)]
#[path = "../../generated/rust/src/gha_indie_worker.rs"]
mod generated_gha_indie_worker;

#[cfg(test)]
#[path = "../../generated/rust/src/hhm_api.rs"]
mod generated_hhm_api;

#[cfg(test)]
#[path = "../../generated/rust/src/hnpt_api.rs"]
mod generated_hnpt_api;

#[cfg(test)]
#[path = "../../generated/rust/src/rpc_transports.rs"]
mod generated_rpc_transports;

#[cfg(test)]
mod generated_key_objects {
    #[test]
    fn pmap_frontend_uses_keys_not_paths() {
        use crate::generated_pmap_api::RouteKey;
        assert_eq!(
            RouteKey::parse("get_matter").unwrap().path(),
            "/v1/matters/{id}"
        );
        assert_eq!(RouteKey::CheckFieldSanity.as_str(), "CheckFieldSanity");
        assert!(RouteKey::ALL.len() >= 10);
    }

    #[test]
    fn canonical_and_chapter_maps_generate() {
        use crate::generated_canonical_api::RouteKey as Canonical;
        use crate::generated_chptr_api::RouteKey as Chapter;
        assert_eq!(
            Canonical::parse("create_quote").unwrap().path(),
            "/api/v1/quotes"
        );
        assert_eq!(
            Chapter::parse("get_chapter").unwrap().path(),
            "/v1/chapters/{chapterId}"
        );
    }

    #[test]
    fn cliptown_gha_hhm_hnpt_maps_generate() {
        use crate::generated_cliptown_api::RouteKey as Clip;
        use crate::generated_gha_indie_worker::RouteKey as Gha;
        use crate::generated_hhm_api::RouteKey as Hhm;
        use crate::generated_hnpt_api::RouteKey as Hnpt;
        assert_eq!(
            Clip::parse("app_vault_sync_push").unwrap().path(),
            "/v1/app-vault/{appId}/sync/push"
        );
        assert_eq!(
            Gha::parse("get_build_logs").unwrap().path(),
            "/builds/{job_id}/logs"
        );
        assert_eq!(
            Hhm::parse("get_reservation").unwrap().path(),
            "/api/v1/reservations/{id}"
        );
        assert_eq!(
            Hnpt::parse("trigger_decoy").unwrap().path(),
            "/decoys/{decoyId}/triggers"
        );
    }

    #[test]
    fn generated_transports_compile() {
        use crate::generated_rpc_transports::RouteKey;
        assert_eq!(
            RouteKey::parse("get_item").unwrap().transports(),
            &["http", "tcp", "websocket"]
        );
        assert_eq!(
            RouteKey::parse("tcp_ping").unwrap().transports(),
            &["tcp"]
        );
    }
}
