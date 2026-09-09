use std::cell::RefCell;
use std::rc::Rc;

use ridl_runtime_rust::{
    Delivery, DirectTransport, LocalReadback, MutationQueue, OptoSyncBinding, OptoSyncTransport,
    OptoTransportError, RecordIdSource, RpcRequest, RpcTransport,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoundaryFixture {
    schema_version: u32,
    profile: String,
    path_cases: Vec<PathCase>,
    request_field_cases: Vec<RequestFieldCase>,
    upsert_body_cases: Vec<UpsertBodyCase>,
}

#[derive(Debug, Deserialize)]
struct PathCase {
    name: String,
    encoded: String,
    valid: bool,
    decoded: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestFieldCase {
    name: String,
    body: String,
    valid: bool,
    decoded: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpsertBodyCase {
    name: String,
    body: String,
    valid: bool,
}

#[derive(Clone, Default)]
struct Calls {
    direct: Rc<RefCell<usize>>,
    queue: Rc<RefCell<usize>>,
    readback: Rc<RefCell<usize>>,
}

#[derive(Clone)]
struct DirectSpy(Calls);

impl DirectTransport for DirectSpy {
    type Error = &'static str;

    fn call(&self, _request: &RpcRequest) -> Result<String, Self::Error> {
        *self.0.direct.borrow_mut() += 1;
        Ok("unexpected-direct".to_string())
    }
}

#[derive(Clone)]
struct QueueSpy(Calls);

impl MutationQueue for QueueSpy {
    type Error = &'static str;

    fn queue_upsert(
        &mut self,
        _table: &str,
        _record_id: &str,
        _payload: &str,
        _base_revision: Option<&str>,
    ) -> Result<String, Self::Error> {
        *self.0.queue.borrow_mut() += 1;
        Ok("mutation-upsert".to_string())
    }

    fn queue_delete(
        &mut self,
        _table: &str,
        _record_id: &str,
        _base_revision: Option<&str>,
    ) -> Result<String, Self::Error> {
        *self.0.queue.borrow_mut() += 1;
        Ok("mutation-delete".to_string())
    }
}

#[derive(Clone)]
struct EchoReadback(Calls);

impl LocalReadback for EchoReadback {
    type Error = &'static str;

    fn local_json(
        &self,
        _table: &str,
        record_id: &str,
    ) -> Result<Option<String>, Self::Error> {
        *self.0.readback.borrow_mut() += 1;
        Ok(Some(record_id.to_string()))
    }
}

fn transport(calls: &Calls) -> OptoSyncTransport<DirectSpy, QueueSpy, EchoReadback> {
    OptoSyncTransport::new(
        DirectSpy(calls.clone()),
        QueueSpy(calls.clone()),
        EchoReadback(calls.clone()),
    )
}

fn request(
    path: String,
    body: Option<String>,
    record_id: RecordIdSource,
) -> RpcRequest {
    RpcRequest {
        key: "widget_operation",
        method: "POST",
        path,
        path_template: "/v1/widgets/{id}",
        query: Vec::new(),
        headers: Vec::new(),
        body,
        delivery: Delivery::OptoSyncQueued,
        opto_sync: Some(OptoSyncBinding {
            table: "widgets",
            operation: "upsert",
            record_id,
        }),
    }
}

fn fixture() -> BoundaryFixture {
    serde_json::from_str(include_str!(
        "../../../examples/opto-sync/record-id-boundary.conformance.json"
    ))
    .expect("Opto-Sync boundary fixture must be valid JSON")
}

#[test]
fn path_record_ids_match_the_shared_strict_percent_decoding_contract() {
    let fixture = fixture();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.profile, "ridl-opto-sync-record-id-boundary-v1");

    for case in fixture.path_cases {
        let calls = Calls::default();
        let result = transport(&calls).call(request(
            format!("/v1/widgets/{}", case.encoded),
            Some(r#"{"name":"ok"}"#.to_string()),
            RecordIdSource::PathParam("id"),
        ));

        if case.valid {
            let actual = result.unwrap_or_else(|error| panic!("{} failed: {error}", case.name));
            assert_eq!(actual, case.decoded.expect("valid case needs decoded value"));
            assert_eq!(*calls.queue.borrow(), 1, "{} must queue once", case.name);
            assert_eq!(
                *calls.readback.borrow(),
                1,
                "{} must read back once",
                case.name
            );
        } else {
            let error = result.expect_err(&format!("{} must fail closed", case.name));
            assert!(matches!(error, OptoTransportError::NotQueueable(_)));
            assert_eq!(*calls.queue.borrow(), 0, "{} reached queue", case.name);
            assert_eq!(
                *calls.readback.borrow(),
                0,
                "{} reached readback",
                case.name
            );
        }
        assert_eq!(*calls.direct.borrow(), 0, "{} went direct", case.name);
    }
}

#[test]
fn request_field_record_ids_match_the_shared_json_contract() {
    for case in fixture().request_field_cases {
        let calls = Calls::default();
        let result = transport(&calls).call(request(
            "/v1/widgets/widget-42".to_string(),
            Some(case.body),
            RecordIdSource::RequestField("id"),
        ));

        if case.valid {
            let actual = result.unwrap_or_else(|error| panic!("{} failed: {error}", case.name));
            assert_eq!(actual, case.decoded.expect("valid case needs decoded value"));
            assert_eq!(*calls.queue.borrow(), 1, "{} must queue once", case.name);
            assert_eq!(*calls.readback.borrow(), 1, "{} must read back once", case.name);
        } else {
            let error = result.expect_err(&format!("{} must fail closed", case.name));
            assert!(matches!(error, OptoTransportError::NotQueueable(_)));
            assert_eq!(*calls.queue.borrow(), 0, "{} reached queue", case.name);
            assert_eq!(*calls.readback.borrow(), 0, "{} reached readback", case.name);
        }
        assert_eq!(*calls.direct.borrow(), 0, "{} went direct", case.name);
    }
}

#[test]
fn queued_upserts_require_a_json_object_before_the_durable_queue() {
    for case in fixture().upsert_body_cases {
        let calls = Calls::default();
        let result = transport(&calls).call(request(
            "/v1/widgets/widget-42".to_string(),
            Some(case.body),
            RecordIdSource::PathParam("id"),
        ));

        if case.valid {
            assert_eq!(result.expect("valid object must queue"), "widget-42");
            assert_eq!(*calls.queue.borrow(), 1, "{} must queue once", case.name);
            assert_eq!(*calls.readback.borrow(), 1, "{} must read back once", case.name);
        } else {
            let error = result.expect_err(&format!("{} must fail closed", case.name));
            assert!(matches!(error, OptoTransportError::NotQueueable(_)));
            assert_eq!(*calls.queue.borrow(), 0, "{} reached queue", case.name);
            assert_eq!(*calls.readback.borrow(), 0, "{} reached readback", case.name);
        }
        assert_eq!(*calls.direct.borrow(), 0, "{} went direct", case.name);
    }
}
