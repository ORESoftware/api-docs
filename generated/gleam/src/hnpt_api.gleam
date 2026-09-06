//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "hnpt-api-server"

pub type RouteKey {
  Healthz
  CreateObservation
  ListDecoys
  CreateDecoy
  TriggerDecoy
  ListAlertDestinations
  CreateAlertDestination
  TestAlertDestination
  ListDiscoveries
  CreateQuarantineCase
  ReleaseQuarantineCase
  CreateOutcome
}

pub fn all() -> List(RouteKey) {
  [Healthz, CreateObservation, ListDecoys, CreateDecoy, TriggerDecoy, ListAlertDestinations, CreateAlertDestination, TestAlertDestination, ListDiscoveries, CreateQuarantineCase, ReleaseQuarantineCase, CreateOutcome]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    CreateObservation -> "create_observation"
    ListDecoys -> "list_decoys"
    CreateDecoy -> "create_decoy"
    TriggerDecoy -> "trigger_decoy"
    ListAlertDestinations -> "list_alert_destinations"
    CreateAlertDestination -> "create_alert_destination"
    TestAlertDestination -> "test_alert_destination"
    ListDiscoveries -> "list_discoveries"
    CreateQuarantineCase -> "create_quarantine_case"
    ReleaseQuarantineCase -> "release_quarantine_case"
    CreateOutcome -> "create_outcome"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "create_observation" -> Ok(CreateObservation)
    "list_decoys" -> Ok(ListDecoys)
    "create_decoy" -> Ok(CreateDecoy)
    "trigger_decoy" -> Ok(TriggerDecoy)
    "list_alert_destinations" -> Ok(ListAlertDestinations)
    "create_alert_destination" -> Ok(CreateAlertDestination)
    "test_alert_destination" -> Ok(TestAlertDestination)
    "list_discoveries" -> Ok(ListDiscoveries)
    "create_quarantine_case" -> Ok(CreateQuarantineCase)
    "release_quarantine_case" -> Ok(ReleaseQuarantineCase)
    "create_outcome" -> Ok(CreateOutcome)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    CreateObservation -> "/observations"
    ListDecoys -> "/decoys"
    CreateDecoy -> "/decoys"
    TriggerDecoy -> "/decoys/{decoyId}/triggers"
    ListAlertDestinations -> "/alert-destinations"
    CreateAlertDestination -> "/alert-destinations"
    TestAlertDestination -> "/alert-destinations/{alertDestinationId}/test"
    ListDiscoveries -> "/discoveries"
    CreateQuarantineCase -> "/quarantine/cases"
    ReleaseQuarantineCase -> "/quarantine/cases/{caseId}/release"
    CreateOutcome -> "/outcomes"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    CreateObservation -> ["POST"]
    ListDecoys -> ["GET"]
    CreateDecoy -> ["POST"]
    TriggerDecoy -> ["POST"]
    ListAlertDestinations -> ["GET"]
    CreateAlertDestination -> ["POST"]
    TestAlertDestination -> ["POST"]
    ListDiscoveries -> ["GET"]
    CreateQuarantineCase -> ["POST"]
    ReleaseQuarantineCase -> ["POST"]
    CreateOutcome -> ["POST"]
  }
}

pub fn transports(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["http"]
    CreateObservation -> ["http"]
    ListDecoys -> ["http"]
    CreateDecoy -> ["http"]
    TriggerDecoy -> ["http"]
    ListAlertDestinations -> ["http"]
    CreateAlertDestination -> ["http"]
    TestAlertDestination -> ["http"]
    ListDiscoveries -> ["http"]
    CreateQuarantineCase -> ["http"]
    ReleaseQuarantineCase -> ["http"]
    CreateOutcome -> ["http"]
  }
}
