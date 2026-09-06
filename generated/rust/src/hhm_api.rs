//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "hhm-api-server";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    ListReservations,
    CreateReservation,
    GetReservation,
    Websocket,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::ListReservations, Self::CreateReservation, Self::GetReservation, Self::Websocket];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::ListReservations => "list_reservations",
            Self::CreateReservation => "create_reservation",
            Self::GetReservation => "get_reservation",
            Self::Websocket => "websocket",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "list_reservations" => Some(Self::ListReservations),
            "create_reservation" => Some(Self::CreateReservation),
            "get_reservation" => Some(Self::GetReservation),
            "websocket" => Some(Self::Websocket),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::ListReservations => "/api/v1/reservations",
            Self::CreateReservation => "/api/v1/reservations",
            Self::GetReservation => "/api/v1/reservations/{id}",
            Self::Websocket => "/ws",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::ListReservations => &["GET"],
            Self::CreateReservation => &["POST"],
            Self::GetReservation => &["GET"],
            Self::Websocket => &["GET"],
        }
    }

    #[must_use]
    pub fn transports(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["http"],
            Self::ListReservations => &["http"],
            Self::CreateReservation => &["http"],
            Self::GetReservation => &["http"],
            Self::Websocket => &["websocket"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateReservationRequest {
    pub title: String,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateReservationResponse {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetReservationPath {
    pub id: String,
}
