use std::cell::RefCell;
use std::rc::Rc;

use ridl_runtime_rust::{
    Delivery, DirectTransport, LocalReadback, MutationQueue, OptoSyncBinding, OptoSyncTransport,
    OptoTransportError, RecordIdSource, RpcRequest, RpcTransport,
};

#[derive(Debug, Default)]
struct QueueState {
    upserts: Vec<(String, String, String)>,
    deletes: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
struct SharedQueue(Rc<RefCell<QueueState>>);

impl MutationQueue for SharedQueue {
    type Error = &'static str;

    fn queue_upsert(
        &mut self,
        table: &str,
        record_id: &str,
        payload: &str,
        _base_revision: Option<&str>,
    ) -> Result<String, Self::Error> {
        self.0.borrow_mut().upserts.push((
            table.to_string(),
            record_id.to_string(),
            payload.to_string(),
        ));
        Ok("mutation-upsert".to_string())
    }

    fn queue_delete(
        &mut self,
        table: &str,
        record_id: &str,
        _base_revision: Option<&str>,
    ) -> Result<String, Self::Error> {
        self.0
            .borrow_mut()
            .deletes
            .push((table.to_string(), record_id.to_string()));
        Ok("mutation-delete".to_string())
    }
}

#[derive(Clone, Debug)]
struct DirectSpy {
    calls: Rc<RefCell<usize>>,
    response: &'static str,
}

impl DirectTransport for DirectSpy {
    type Error = &'static str;

    fn call(&self, _request: &RpcRequest) -> Result<String, Self::Error> {
        *self.calls.borrow_mut() += 1;
        Ok(self.response.to_string())
    }
}

#[derive(Clone, Debug)]
struct ReadbackSpy {
    calls: Rc<RefCell<usize>>,
    local_json: Option<String>,
}

impl LocalReadback for ReadbackSpy {
    type Error = &'static str;

    fn local_json(
        &self,
        _table: &str,
        _record_id: &str,
    ) -> Result<Option<String>, Self::Error> {
        *self.calls.borrow_mut() += 1;
        Ok(self.local_json.clone())
    }
}

fn binding(operation: &'static str) -> OptoSyncBinding {
    OptoSyncBinding {
        table: "widgets",
        operation,
        record_id: RecordIdSource::PathParam("id"),
    }
}

fn request(delivery: Delivery, operation: &'static str, body: Option<&str>) -> RpcRequest {
    RpcRequest {
        key: "widget_operation",
        method: "POST",
        path: "/v1/widgets/widget-42".to_string(),
        path_template: "/v1/widgets/{id}",
        query: Vec::new(),
        headers: Vec::new(),
        body: body.map(str::to_string),
        delivery,
        opto_sync: Some(binding(operation)),
    }
}

#[test]
fn direct_delivery_bypasses_queue_and_local_projection_even_with_binding_metadata() {
    let queue_state = Rc::new(RefCell::new(QueueState::default()));
    let direct_calls = Rc::new(RefCell::new(0));
    let readback_calls = Rc::new(RefCell::new(0));
    let transport = OptoSyncTransport::new(
        DirectSpy {
            calls: Rc::clone(&direct_calls),
            response: r#"{"source":"authoritative"}"#,
        },
        SharedQueue(Rc::clone(&queue_state)),
        ReadbackSpy {
            calls: Rc::clone(&readback_calls),
            local_json: Some(r#"{"source":"local"}"#.to_string()),
        },
    );

    let response = transport
        .call(request(Delivery::Direct, "upsert", None))
        .expect("direct reads must stay direct");

    assert_eq!(response, r#"{"source":"authoritative"}"#);
    assert_eq!(*direct_calls.borrow(), 1);
    assert_eq!(*readback_calls.borrow(), 0);
    assert!(queue_state.borrow().upserts.is_empty());
    assert!(queue_state.borrow().deletes.is_empty());
}

#[test]
fn queued_delete_uses_tombstone_queue_path_and_returns_local_projection() {
    let queue_state = Rc::new(RefCell::new(QueueState::default()));
    let direct_calls = Rc::new(RefCell::new(0));
    let readback_calls = Rc::new(RefCell::new(0));
    let transport = OptoSyncTransport::new(
        DirectSpy {
            calls: Rc::clone(&direct_calls),
            response: "unexpected-direct",
        },
        SharedQueue(Rc::clone(&queue_state)),
        ReadbackSpy {
            calls: Rc::clone(&readback_calls),
            local_json: Some(r#"{"id":"widget-42","deleted":true}"#.to_string()),
        },
    );

    let response = transport
        .call(request(Delivery::OptoSyncQueued, "delete", None))
        .expect("queued delete should return its optimistic tombstone projection");

    assert_eq!(response, r#"{"id":"widget-42","deleted":true}"#);
    assert_eq!(*direct_calls.borrow(), 0);
    assert_eq!(*readback_calls.borrow(), 1);
    let state = queue_state.borrow();
    assert!(state.upserts.is_empty());
    assert_eq!(
        state.deletes,
        [("widgets".to_string(), "widget-42".to_string())]
    );
}

#[test]
fn queued_mutation_without_local_projection_fails_closed_after_durable_queueing() {
    let queue_state = Rc::new(RefCell::new(QueueState::default()));
    let transport = OptoSyncTransport::new(
        DirectSpy {
            calls: Rc::new(RefCell::new(0)),
            response: "unexpected-direct",
        },
        SharedQueue(Rc::clone(&queue_state)),
        ReadbackSpy {
            calls: Rc::new(RefCell::new(0)),
            local_json: None,
        },
    );

    let error = transport
        .call(request(
            Delivery::OptoSyncQueued,
            "upsert",
            Some(r#"{"name":"offline edit"}"#),
        ))
        .expect_err("a queued write must not fabricate an authoritative response");

    assert!(matches!(
        error,
        OptoTransportError::NoLocalProjection {
            table,
            record_id
        } if table == "widgets" && record_id == "widget-42"
    ));
    assert_eq!(queue_state.borrow().upserts.len(), 1);
}
