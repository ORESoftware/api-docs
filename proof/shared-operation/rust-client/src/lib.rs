#![forbid(unsafe_code)]

/// Generated RPC client used by both the direct-client live proof and the
/// web-server -> API-server live proof. The web server consumes this module as
/// a client library; it never mounts the API server's RPC router.
pub mod generated;
