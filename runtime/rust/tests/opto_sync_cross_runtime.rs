use std::cell::RefCell;
use std::rc::Rc;

use ridl_runtime_rust::{
    Delivery, DirectTransport, LocalReadback, MutationQueue, OptoSyncBinding, OptoSyncTransport,
    OptoTransportError, RecordIdSource, RpcRequest, RpcTransport,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MintedVectors {
    schema_version: u32,
    profile: String,
    algorithm: String,
    input_encoding: String,
    cases: Vec<MintedCase>,
}

#[derive(Debug, Deserialize)]
struct MintedCase {
    name: String,
    key: String,
    path: String,
    body: String,
    expected: String,
}

#[derive(Clone, Debug, Default)]
struct Calls {
    direct: Rc<RefCell<usize>>,
    queue: Rc<RefCell<usize>>,
    readback: Rc<RefCell<usize>>,
}

#[derive(Clone, Debug)]
struct DirectSpy {
    calls: Calls,
    response: &'static str,
}

impl DirectTransport for DirectSpy {
    type Error = &'static str;

    fn call(&self, _request: &RpcRequest) -> Result<String, Self::Error> {
        *self.calls.direct.borrow_mut() += 1;
        Ok(self.response.to_string())
    }
}

#[derive(Clone, Debug)]
struct QueueSpy {
    calls: Calls,
    fail: bool,
}

impl MutationQueue for QueueSpy {
    type Error = &'static str;

    fn queue_upsert(
        &mut self,
        _table: &str,
        _record_id: &str,
        _payload: &str,
        _base_revision: Option<&str>,
    ) -> Result<String, Self::Error> {
        *self.calls.queue.borrow_mut() += 1;
        if self.fail {
            Err("queue failed")
        } else {
            Ok("mutation-upsert".to_string())
        }
    }

    fn queue_delete(
        &mut self,
        _table: &str,
        _record_id: &str,
        _base_revision: Option<&str>,
    ) -> Result<String, Self::Error> {
        *self.calls.queue.borrow_mut() += 1;
        if self.fail {
            Err("queue failed")
        } else {
            Ok("mutation-delete".to_string())
        }
    }
}

#[derive(Clone, Debug)]
struct ReadbackSpy {
    calls: Calls,
    fail: bool,
    echo_record_id: bool,
    response: Option<&'static str>,
}

impl LocalReadback for ReadbackSpy {
    type Error = &'static str;

    fn local_json(
        &self,
        _table: &str,
        record_id: &str,
    ) -> Result<Option<String>, Self::Error> {
        *self.calls.readback.borrow_mut() += 1;
        if self.fail {
            return Err("readback failed");
        }
        if self.echo_record_id {
            return Ok(Some(record_id.to_string()));
        }
        Ok(self.response.map(str::to_string))
    }
}

fn request(
    delivery: Delivery,
    body: Option<&str>,
    binding: Option<OptoSyncBinding>,
) -> RpcRequest {
    RpcRequest {
        key: "widget_operation",
        method: "POST",
        path: "/v1/widgets/widget-42".to_string(),
        path_template: "/v1/widgets/{id}",
        query: Vec::new(),
        headers: Vec::new(),
        body: body.map(str::to_string),
        delivery,
        opto_sync: binding,
    }
}

fn path_binding(operation: &'static str) -> OptoSyncBinding {
    OptoSyncBinding {
        table: "widgets",
        operation,
        record_id: RecordIdSource::PathParam("id"),
    }
}

#[test]
fn rust_consumes_the_same_minted_id_vectors_as_typescript() {
    let vectors: MintedVectors = serde_json::from_str(include_str!(
        "../../../examples/opto-sync/minted-id.conformance.json"
    ))
    .expect("canonical minted-id fixture must be valid JSON");

    assert_eq!(vectors.schema_version, 1);
    assert_eq!(vectors.profile, "ridl-opto-sync-minted-record-id-v1");
    assert_eq!(vectors.algorithm, "fnv1a-64");
    assert_eq!(vectors.input_encoding, "utf-8");

    for case in vectors.cases {
        let calls = Calls::default();
        let transport = OptoSyncTransport::new(
            DirectSpy {
                calls: calls.clone(),
                response: "unexpected-direct",
            },
            QueueSpy {
                calls: calls.clone(),
                fail: false,
            },
            ReadbackSpy {
                calls: calls.clone(),
                fail: false,
                echo_record_id: true,
                response: None,
            },
        );
        let request = RpcRequest {
            key: Box::leak(case.key.clone().into_boxed_str()),
            method: "POST",
            path: case.path,
            path_template: "/v1/minted",
            query: Vec::new(),
            headers: Vec::new(),
            body: Some(case.body),
            delivery: Delivery::OptoSyncQueued,
            opto_sync: Some(OptoSyncBinding {
                table: "widgets",
                operation: "upsert",
                record_id: RecordIdSource::Minted,
            }),
        };

        let actual = transport
            .call(request)
            .unwrap_or_else(|error| panic!("{} failed: {error}", case.name));
        assert_eq!(actual, case.expected, "minted-id drift in {}", case.name);
        assert_eq!(*calls.direct.borrow(), 0, "{} went direct", case.name);
        assert_eq!(*calls.queue.borrow(), 1, "{} was not queued once", case.name);
        assert_eq!(
            *calls.readback.borrow(),
            1,
            "{} did not read back once",
            case.name
        );
    }
}

#[test]
fn queued_delivery_without_binding_stays_direct() {
    let calls = Calls::default();
    let transport = OptoSyncTransport::new(
        DirectSpy {
            calls: calls.clone(),
            response: "authoritative",
        },
        QueueSpy {
            calls: calls.clone(),
            fail: false,
        },
        ReadbackSpy {
            calls: calls.clone(),
            fail: false,
            echo_record_id: false,
            response: Some("local"),
        },
    );

    let response = transport
        .call(request(
            Delivery::OptoSyncQueued,
            Some(r#"{"name":"offline edit"}"#),
            None,
        ))
        .expect("missing Opto-Sync binding must use direct transport");

    assert_eq!(response, "authoritative");
    assert_eq!(*calls.direct.borrow(), 1);
    assert_eq!(*calls.queue.borrow(), 0);
    assert_eq!(*calls.readback.borrow(), 0);
}

#[test]
fn queue_failure_never_falls_back_to_direct_or_readback() {
    let calls = Calls::default();
    let transport = OptoSyncTransport::new(
        DirectSpy {
            calls: calls.clone(),
            response: "unexpected-direct",
        },
        QueueSpy {
            calls: calls.clone(),
            fail: true,
        },
        ReadbackSpy {
            calls: calls.clone(),
            fail: false,
            echo_record_id: false,
            response: Some("unexpected-local"),
        },
    );

    let error = transport
        .call(request(
            Delivery::OptoSyncQueued,
            Some(r#"{"name":"offline edit"}"#),
            Some(path_binding("upsert")),
        ))
        .expect_err("queue failure must fail closed");

    assert!(matches!(error, OptoTransportError::Queue("queue failed")));
    assert_eq!(*calls.queue.borrow(), 1);
    assert_eq!(*calls.direct.borrow(), 0);
    assert_eq!(*calls.readback.borrow(), 0);
}

#[test]
fn readback_failure_is_distinct_after_durable_queueing_and_never_falls_back() {
    let calls = Calls::default();
    let transport = OptoSyncTransport::new(
        DirectSpy {
            calls: calls.clone(),
            response: "unexpected-direct",
        },
        QueueSpy {
            calls: calls.clone(),
            fail: false,
        },
        ReadbackSpy {
            calls: calls.clone(),
            fail: true,
            echo_record_id: false,
            response: None,
        },
    );

    let error = transport
        .call(request(
            Delivery::OptoSyncQueued,
            Some(r#"{"name":"offline edit"}"#),
            Some(path_binding("upsert")),
        ))
        .expect_err("readback failure must remain distinguishable from queue failure");

    assert!(matches!(
        error,
        OptoTransportError::Readback("readback failed")
    ));
    assert_eq!(*calls.queue.borrow(), 1);
    assert_eq!(*calls.readback.borrow(), 1);
    assert_eq!(*calls.direct.borrow(), 0);
}
