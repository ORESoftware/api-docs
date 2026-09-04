//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "example-rpc";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    GetItem,
    Websocket,
    TcpPing,
    NatsPing,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::GetItem, Self::Websocket, Self::TcpPing, Self::NatsPing];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::GetItem => "get_item",
            Self::Websocket => "websocket",
            Self::TcpPing => "tcp_ping",
            Self::NatsPing => "nats_ping",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "get_item" => Some(Self::GetItem),
            "websocket" => Some(Self::Websocket),
            "tcp_ping" => Some(Self::TcpPing),
            "nats_ping" => Some(Self::NatsPing),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::GetItem => "/v1/items/{id}",
            Self::Websocket => "/ws",
            Self::TcpPing => "/rpc/ping",
            Self::NatsPing => "/rpc/nats-ping",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::GetItem => &["GET"],
            Self::Websocket => &["GET"],
            Self::TcpPing => &["POST"],
            Self::NatsPing => &["POST"],
        }
    }

    #[must_use]
    pub fn transports(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["http"],
            Self::GetItem => &["http", "tcp", "websocket"],
            Self::Websocket => &["websocket"],
            Self::TcpPing => &["tcp"],
            Self::NatsPing => &["nats"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetItemPath {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetItemHeaders {
    #[serde(rename = "x-request-id")]
    pub x_request_id: String,
    #[serde(rename = "if-none-match")]
    pub if_none_match: Option<String>,
}
