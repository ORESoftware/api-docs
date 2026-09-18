use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use ores_api_docs::{
    OperationPolicy, OperationPolicyFuture, OperationPolicyOutcome, OperationPolicyPermit,
    OperationPolicyRejection, OperationPolicyRequest, RpcTelemetrySink,
};
use serde::Serialize;
use serde_json::Value;

use crate::model::User;

#[derive(Default)]
pub struct ProofCounters {
    pub http_adapter_hits: AtomicUsize,
    pub rpc_adapter_hits: AtomicUsize,
    pub operation_hits: AtomicUsize,
    pub policy_before_hits: AtomicUsize,
    pub policy_after_hits: AtomicUsize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CounterSnapshot {
    pub http_adapter_hits: usize,
    pub rpc_adapter_hits: usize,
    pub operation_hits: usize,
    pub policy_before_hits: usize,
    pub policy_after_hits: usize,
}

impl ProofCounters {
    pub fn snapshot(&self) -> CounterSnapshot {
        CounterSnapshot {
            http_adapter_hits: self.http_adapter_hits.load(Ordering::SeqCst),
            rpc_adapter_hits: self.rpc_adapter_hits.load(Ordering::SeqCst),
            operation_hits: self.operation_hits.load(Ordering::SeqCst),
            policy_before_hits: self.policy_before_hits.load(Ordering::SeqCst),
            policy_after_hits: self.policy_after_hits.load(Ordering::SeqCst),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub users: Arc<Mutex<HashMap<String, User>>>,
    pub counters: Arc<ProofCounters>,
    pub policy: Arc<ProofPolicy>,
    /// Application-owned ores-otel adapter, or `None`.
    ///
    /// The proof server runs without one, which is the point: the generated
    /// dispatch path must behave identically either way, so the live proof in
    /// CI exercises the `None` branch of every emit.
    pub telemetry: Option<Arc<dyn RpcTelemetrySink>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::with_telemetry(None)
    }

    pub fn with_telemetry(telemetry: Option<Arc<dyn RpcTelemetrySink>>) -> Self {
        let counters = Arc::new(ProofCounters::default());
        let policy = Arc::new(ProofPolicy {
            counters: counters.clone(),
        });
        let mut users = HashMap::new();
        users.insert(
            "seed-user".into(),
            User {
                id: "seed-user".into(),
                display_name: "Seed User".into(),
            },
        );
        Self {
            users: Arc::new(Mutex::new(users)),
            counters,
            policy,
            telemetry,
        }
    }
}

pub struct ProofPolicy {
    counters: Arc<ProofCounters>,
}

impl OperationPolicy for ProofPolicy {
    fn before<'a>(
        &'a self,
        request: OperationPolicyRequest<'a>,
    ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>> {
        self.counters
            .policy_before_hits
            .fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let mut values = BTreeMap::new();
            values.insert(
                "operation_key".into(),
                Value::String(request.operation.key.to_owned()),
            );
            Ok(OperationPolicyPermit { values })
        })
    }

    fn after<'a>(&'a self, _outcome: OperationPolicyOutcome<'a>) -> OperationPolicyFuture<'a, ()> {
        self.counters
            .policy_after_hits
            .fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
}
