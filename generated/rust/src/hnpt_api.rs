//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "hnpt-api-server";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    CreateObservation,
    ListDecoys,
    CreateDecoy,
    TriggerDecoy,
    ListAlertDestinations,
    CreateAlertDestination,
    TestAlertDestination,
    ListDiscoveries,
    CreateQuarantineCase,
    ReleaseQuarantineCase,
    CreateOutcome,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::CreateObservation, Self::ListDecoys, Self::CreateDecoy, Self::TriggerDecoy, Self::ListAlertDestinations, Self::CreateAlertDestination, Self::TestAlertDestination, Self::ListDiscoveries, Self::CreateQuarantineCase, Self::ReleaseQuarantineCase, Self::CreateOutcome];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::CreateObservation => "create_observation",
            Self::ListDecoys => "list_decoys",
            Self::CreateDecoy => "create_decoy",
            Self::TriggerDecoy => "trigger_decoy",
            Self::ListAlertDestinations => "list_alert_destinations",
            Self::CreateAlertDestination => "create_alert_destination",
            Self::TestAlertDestination => "test_alert_destination",
            Self::ListDiscoveries => "list_discoveries",
            Self::CreateQuarantineCase => "create_quarantine_case",
            Self::ReleaseQuarantineCase => "release_quarantine_case",
            Self::CreateOutcome => "create_outcome",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "create_observation" => Some(Self::CreateObservation),
            "list_decoys" => Some(Self::ListDecoys),
            "create_decoy" => Some(Self::CreateDecoy),
            "trigger_decoy" => Some(Self::TriggerDecoy),
            "list_alert_destinations" => Some(Self::ListAlertDestinations),
            "create_alert_destination" => Some(Self::CreateAlertDestination),
            "test_alert_destination" => Some(Self::TestAlertDestination),
            "list_discoveries" => Some(Self::ListDiscoveries),
            "create_quarantine_case" => Some(Self::CreateQuarantineCase),
            "release_quarantine_case" => Some(Self::ReleaseQuarantineCase),
            "create_outcome" => Some(Self::CreateOutcome),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::CreateObservation => "/observations",
            Self::ListDecoys => "/decoys",
            Self::CreateDecoy => "/decoys",
            Self::TriggerDecoy => "/decoys/{decoyId}/triggers",
            Self::ListAlertDestinations => "/alert-destinations",
            Self::CreateAlertDestination => "/alert-destinations",
            Self::TestAlertDestination => "/alert-destinations/{alertDestinationId}/test",
            Self::ListDiscoveries => "/discoveries",
            Self::CreateQuarantineCase => "/quarantine/cases",
            Self::ReleaseQuarantineCase => "/quarantine/cases/{caseId}/release",
            Self::CreateOutcome => "/outcomes",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::CreateObservation => &["POST"],
            Self::ListDecoys => &["GET"],
            Self::CreateDecoy => &["POST"],
            Self::TriggerDecoy => &["POST"],
            Self::ListAlertDestinations => &["GET"],
            Self::CreateAlertDestination => &["POST"],
            Self::TestAlertDestination => &["POST"],
            Self::ListDiscoveries => &["GET"],
            Self::CreateQuarantineCase => &["POST"],
            Self::ReleaseQuarantineCase => &["POST"],
            Self::CreateOutcome => &["POST"],
        }
    }

    #[must_use]
    pub fn transports(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["http"],
            Self::CreateObservation => &["http"],
            Self::ListDecoys => &["http"],
            Self::CreateDecoy => &["http"],
            Self::TriggerDecoy => &["http"],
            Self::ListAlertDestinations => &["http"],
            Self::CreateAlertDestination => &["http"],
            Self::TestAlertDestination => &["http"],
            Self::ListDiscoveries => &["http"],
            Self::CreateQuarantineCase => &["http"],
            Self::ReleaseQuarantineCase => &["http"],
            Self::CreateOutcome => &["http"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateObservationRequest {
    #[serde(rename = "decoyId")]
    pub decoy_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateObservationResponse {
    pub id: String,
    pub disposition: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ListDecoysQuery {
    pub cursor: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateDecoyRequest {
    #[serde(rename = "tenantId")]
    pub tenant_id: String,
    #[serde(rename = "assetId")]
    pub asset_id: String,
    #[serde(rename = "decoyKey")]
    pub decoy_key: String,
    pub kind: String,
    pub profile: String,
    #[serde(rename = "syntheticNamespace")]
    pub synthetic_namespace: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TriggerDecoyPath {
    #[serde(rename = "decoyId")]
    pub decoy_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TriggerDecoyRequest {
    #[serde(rename = "tenantId")]
    pub tenant_id: String,
    #[serde(rename = "sensorId")]
    pub sensor_id: String,
    #[serde(rename = "eventId")]
    pub event_id: String,
    #[serde(rename = "eventTime")]
    pub event_time: String,
    pub protocol: String,
    #[serde(rename = "sourceHash")]
    pub source_hash: String,
    pub attributes: serde_json::Value,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ListAlertDestinationsQuery {
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateAlertDestinationRequest {
    #[serde(rename = "tenantId")]
    pub tenant_id: String,
    #[serde(rename = "destinationKey")]
    pub destination_key: String,
    pub kind: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "minimumSeverity")]
    pub minimum_severity: String,
    #[serde(rename = "endpointSecretRef")]
    pub endpoint_secret_ref: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TestAlertDestinationPath {
    #[serde(rename = "alertDestinationId")]
    pub alert_destination_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TestAlertDestinationRequest {
    pub mode: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ListDiscoveriesQuery {
    pub cursor: Option<String>,
    pub state: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateQuarantineCaseRequest {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ReleaseQuarantineCasePath {
    #[serde(rename = "caseId")]
    pub case_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ReleaseQuarantineCaseRequest {
    #[serde(rename = "reasonCode")]
    pub reason_code: String,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateOutcomeRequest {
    pub id: String,
}

