use ores_api_docs::{NoSection, OperationSpec, RpcPayloadCodec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationEnvelope<T> {
    pub result: T,
    #[serde(rename = "traceIds")]
    pub trace_ids: Vec<String>,
}

impl<T> OperationEnvelope<T> {
    pub fn new(result: T, trace_id: &'static str) -> Self {
        Self {
            result,
            trace_ids: vec![trace_id.to_owned()],
        }
    }

    pub fn prepend_trace_id(&mut self, trace_id: &'static str) {
        self.trace_ids.insert(0, trace_id.to_owned());
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateUserHeaders {
    #[serde(rename = "x-ores-tenant")]
    pub x_ores_tenant: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateUserRequest {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindUserPath {
    pub user_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindUserQuery {
    pub include_disabled: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindUserHeaders {
    #[serde(rename = "if-none-match")]
    pub if_none_match: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateUserPath {
    pub user_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateUserHeaders {
    #[serde(rename = "idempotency-key")]
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateUserRequest {
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofError {
    pub code: String,
    pub message: String,
}

impl ProofError {
    pub fn not_found(user_id: &str) -> Self {
        Self {
            code: "user_not_found".into(),
            message: format!("user {user_id} was not found"),
        }
    }
}

pub struct CreateUserOperation;
impl OperationSpec for CreateUserOperation {
    type Path = NoSection;
    type Query = NoSection;
    type RequestHeaders = CreateUserHeaders;
    type RequestBody = CreateUserRequest;
    type ResponseBody = OperationEnvelope<User>;
    type ResponseHeaders = NoSection;
    type ResponseTrailers = NoSection;
    type Error = ProofError;

    const KEY: &'static str = "demo.users.create_user";
    const CODECS: &'static [RpcPayloadCodec] = &[RpcPayloadCodec::Json];
    const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Json;
}

pub struct FindUserOperation;
impl OperationSpec for FindUserOperation {
    type Path = FindUserPath;
    type Query = FindUserQuery;
    type RequestHeaders = FindUserHeaders;
    type RequestBody = NoSection;
    type ResponseBody = OperationEnvelope<User>;
    type ResponseHeaders = NoSection;
    type ResponseTrailers = NoSection;
    type Error = ProofError;

    const KEY: &'static str = "demo.users.find_user_by_id";
    const CODECS: &'static [RpcPayloadCodec] = &[RpcPayloadCodec::Json];
    const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Json;
}

pub struct UpdateUserOperation;
impl OperationSpec for UpdateUserOperation {
    type Path = UpdateUserPath;
    type Query = NoSection;
    type RequestHeaders = UpdateUserHeaders;
    type RequestBody = UpdateUserRequest;
    type ResponseBody = OperationEnvelope<User>;
    type ResponseHeaders = NoSection;
    type ResponseTrailers = NoSection;
    type Error = ProofError;

    const KEY: &'static str = "demo.users.update_user";
    const CODECS: &'static [RpcPayloadCodec] = &[RpcPayloadCodec::Json];
    const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Json;
}
