//! Loopback e2e: the same `get_item` call/receipt JSON on HTTP, TCP NDJSON,
//! and WebSocket text frames. Also covers opto-sync map envelopes and
//! ores-otel-shaped telemetry fields — without depending on those crates.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Empty, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use ores_api_docs::{
    expand_path, Catalog, RpcCall, RpcHttp, RpcMethod, RpcReceipt, RpcTransport, RouteMap,
    RouteMapEnvelope, TelemetryAttributes, Transport, OPTO_SYNC_SCOPE, RPC_SYSTEM,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message as WsMessage;

#[path = "../../generated/rust/src/rpc_transports.rs"]
mod rpc_routes;

use rpc_routes::RouteKey;

const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const SPAN_ID: &str = "00f067aa0ba902b7";
const ITEM_ID: &str = "item-42";

fn route_map() -> &'static RouteMap {
    static MAP: OnceLock<RouteMap> = OnceLock::new();
    MAP.get_or_init(|| {
        RouteMap::from_json_str(include_str!("../../examples/rpc-transports.route-map.json"))
            .expect("rpc-transports example")
    })
}

/// Compile-time alignment with generated `RouteKey::GetItem`.
struct GetItem;

impl RpcMethod for GetItem {
    const KEY: &'static str = "get_item";
    const PATH: &'static str = "/v1/items/{id}";
    const METHODS: &'static [&'static str] = &["GET"];
    type Params = ();
    type Output = Value;
}

impl RpcHttp for GetItem {
    type PathParams = rpc_routes::GetItemPath;
    type Query = ();
}

impl RpcTransport for GetItem {
    const TRANSPORTS: &'static [&'static str] = &["http", "tcp", "websocket"];
}

fn handle_call(call: RpcCall, wire: Transport) -> RpcReceipt {
    call.validate().expect("inbound call schema");
    if let Some(claimed) = call.transport {
        if claimed != wire {
            let mut rec = RpcReceipt::error(
                call.id,
                call.key,
                400,
                json!({ "code": "transport_mismatch" }),
            );
            rec.transport = Some(wire);
            rec.trace_id = call.trace_id;
            rec.span_id = call.span_id;
            rec.validate().expect("mismatch receipt");
            return rec;
        }
    }

    let Some(key) = RouteKey::parse(&call.key) else {
        let mut rec = RpcReceipt::error(call.id, call.key, 404, json!({ "code": "unknown_key" }));
        rec.transport = Some(wire);
        rec.trace_id = call.trace_id;
        rec.span_id = call.span_id;
        rec.validate().expect("unknown-key receipt");
        return rec;
    };

    if !key.transports().contains(&wire.as_str()) {
        let mut rec = RpcReceipt::error(
            call.id,
            call.key,
            405,
            json!({ "code": "transport_not_allowed" }),
        );
        rec.transport = Some(wire);
        rec.trace_id = call.trace_id;
        rec.span_id = call.span_id;
        rec.validate().expect("405 receipt");
        return rec;
    }

    let body = match key {
        RouteKey::Healthz => json!({ "status": "ok" }),
        RouteKey::GetItem => {
            let id = call
                .path
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                let mut rec = RpcReceipt::error(
                    call.id,
                    call.key,
                    400,
                    json!({ "code": "missing_path_id" }),
                );
                rec.transport = Some(wire);
                rec.trace_id = call.trace_id;
                rec.span_id = call.span_id;
                rec.validate().expect("missing id receipt");
                return rec;
            }
            json!({ "id": id, "name": format!("item-{id}") })
        }
        RouteKey::TcpPing => json!({ "pong": true }),
        RouteKey::Websocket => {
            let mut rec = RpcReceipt::error(
                call.id,
                call.key,
                400,
                json!({ "code": "upgrade_only" }),
            );
            rec.transport = Some(wire);
            rec.trace_id = call.trace_id;
            rec.span_id = call.span_id;
            rec.validate().expect("upgrade-only receipt");
            return rec;
        }
    };

    let mut rec = RpcReceipt::ok(call.id, call.key, Some(body));
    rec.transport = Some(wire);
    rec.trace_id = call.trace_id;
    rec.span_id = call.span_id;
    rec.validate().expect("ok receipt");
    rec
}

fn product_router() -> Router {
    let catalog = Catalog::from_map(route_map().clone()).expect("catalog");
    Router::new()
        .route(RouteKey::Healthz.path(), get(healthz_http))
        .route("/v1/items/{id}", get(get_item_http))
        .route("/rpc", post(rpc_http))
        .route(RouteKey::Websocket.path(), get(ws_upgrade))
        .merge(ores_api_docs::axum_router::router(catalog))
}

async fn healthz_http() -> impl IntoResponse {
    let mut call = RpcCall::new("http-healthz", RouteKey::Healthz.as_str());
    call.transport = Some(Transport::Http);
    let rec = handle_call(call, Transport::Http);
    json_receipt(rec)
}

async fn rpc_http(Json(call): Json<RpcCall>) -> impl IntoResponse {
    let rec = handle_call(call, Transport::Http);
    json_receipt(rec)
}

async fn get_item_http(Path(id): Path<String>) -> impl IntoResponse {
    let mut call = RpcCall::new("http-get-item", GetItem::KEY);
    call.transport = Some(Transport::Http);
    call.path = json!({ "id": id });
    call.trace_id = Some(TRACE_ID.into());
    call.span_id = Some(SPAN_ID.into());
    let rec = handle_call(call, Transport::Http);
    json_receipt(rec)
}

fn json_receipt(rec: RpcReceipt) -> (StatusCode, Json<RpcReceipt>) {
    let status = StatusCode::from_u16(rec.status.unwrap_or(200)).unwrap_or(StatusCode::OK);
    (status, Json(rec))
}

async fn ws_upgrade(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                let call: RpcCall = match serde_json::from_str(text.as_str()) {
                    Ok(c) => c,
                    Err(_) => break,
                };
                let rec = handle_call(call, Transport::Websocket);
                let payload = serde_json::to_string(&rec).expect("receipt json");
                if socket.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn spawn_http() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("http bind");
    let addr = listener.local_addr().expect("http addr");
    tokio::spawn(async move {
        axum::serve(listener, product_router())
            .await
            .expect("http serve");
    });
    wait_port(addr).await;
    addr
}

async fn spawn_tcp() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("tcp bind");
    let addr = listener.local_addr().expect("tcp addr");
    let map = Arc::new(route_map().clone());
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let map = Arc::clone(&map);
            tokio::spawn(async move {
                serve_ndjson(stream, map).await;
            });
        }
    });
    wait_port(addr).await;
    addr
}

async fn serve_ndjson(mut stream: TcpStream, _map: Arc<RouteMap>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    loop {
        let n = match stream.read(&mut tmp).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        buf.extend_from_slice(&tmp[..n]);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let Ok(text) = std::str::from_utf8(&line) else {
                return;
            };
            let call = match RpcCall::from_ndjson(text) {
                Ok(c) => c,
                Err(_) => return,
            };
            let rec = handle_call(call, Transport::Tcp);
            let out = rec.to_ndjson().expect("receipt ndjson");
            if stream.write_all(out.as_bytes()).await.is_err() {
                return;
            }
        }
    }
}

async fn wait_port(addr: SocketAddr) {
    for _ in 0..50 {
        if TcpStream::connect(addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("nothing accepted on {addr}");
}

async fn http_get(addr: SocketAddr, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let client = Client::builder(TokioExecutor::new()).build_http();
    let uri: hyper::Uri = format!("http://{addr}{path}").parse().expect("uri");
    let req = hyper::Request::builder()
        .uri(uri)
        .header("accept", "application/json")
        .body(Empty::<Bytes>::new())
        .expect("http request");
    let res = client.request(req).await.unwrap_or_else(|e| panic!("GET {path}: {e}"));
    let status = res.status().as_u16();
    let headers = res
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let body = res
        .into_body()
        .collect()
        .await
        .expect("http body")
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

async fn http_post_json(addr: SocketAddr, path: &str, json: &[u8]) -> (u16, Vec<u8>) {
    let client = Client::builder(TokioExecutor::new()).build_http();
    let uri: hyper::Uri = format!("http://{addr}{path}").parse().expect("uri");
    let req = hyper::Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .body(Full::new(Bytes::copy_from_slice(json)))
        .expect("http post");
    let res = client
        .request(req)
        .await
        .unwrap_or_else(|e| panic!("POST {path}: {e}"));
    let status = res.status().as_u16();
    let body = res
        .into_body()
        .collect()
        .await
        .expect("http body")
        .to_bytes()
        .to_vec();
    (status, body)
}

async fn tcp_exchange(addr: SocketAddr, call: &RpcCall) -> RpcReceipt {
    let mut stream = TcpStream::connect(addr).await.expect("tcp connect");
    stream
        .write_all(call.to_ndjson().unwrap().as_bytes())
        .await
        .expect("tcp write");
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    while !buf.contains(&b'\n') {
        let n = stream.read(&mut tmp).await.expect("tcp read");
        assert!(n > 0, "tcp closed before receipt for {}", call.id);
        buf.extend_from_slice(&tmp[..n]);
    }
    RpcReceipt::from_ndjson(std::str::from_utf8(&buf).unwrap()).expect("tcp receipt")
}

fn receipt_from_http_body(body: &[u8]) -> RpcReceipt {
    let rec: RpcReceipt = serde_json::from_slice(body).expect("receipt json");
    rec.validate().expect("receipt schema");
    rec
}

fn get_item_call(id: &str, transport: Transport, call_id: &str) -> RpcCall {
    let mut call = RpcCall::new(call_id, GetItem::KEY);
    call.transport = Some(transport);
    call.path = json!({ "id": id });
    call.trace_id = Some(TRACE_ID.into());
    call.span_id = Some(SPAN_ID.into());
    call.validate().expect("call schema");
    call
}

fn assert_get_item_ok(rec: &RpcReceipt, transport: Transport) {
    rec.validate().expect("receipt");
    assert!(rec.ok, "{rec:?}");
    assert_eq!(rec.key, GetItem::KEY);
    assert_eq!(rec.transport, Some(transport));
    assert_eq!(rec.status, Some(200));
    assert_eq!(rec.body.as_ref().unwrap()["id"], ITEM_ID);
    assert_eq!(
        rec.body.as_ref().unwrap()["name"],
        format!("item-{ITEM_ID}")
    );
    assert_eq!(rec.trace_id.as_deref(), Some(TRACE_ID));
    assert_eq!(rec.span_id.as_deref(), Some(SPAN_ID));
}

fn otel_shaped_log(rec: &RpcReceipt) -> Value {
    let mut attrs =
        TelemetryAttributes::start(rpc_routes::SERVICE, rec.key.clone(), rec.transport.unwrap());
    attrs.rpc_ok = Some(rec.ok);
    attrs.http_status_code = rec.status;
    attrs.validate().expect("telemetry schema");
    let fields = attrs.to_fields().expect("fields");
    json!({
        "fields": fields,
        "traceId": rec.trace_id,
        "spanId": rec.span_id,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_item_same_json_on_http_tcp_and_websocket() {
    assert_eq!(GetItem::TRANSPORTS, RouteKey::GetItem.transports());
    assert_eq!(GetItem::PATH, RouteKey::GetItem.path());
    assert_eq!(GetItem::METHODS, RouteKey::GetItem.methods());

    let http_addr = spawn_http().await;
    let tcp_addr = spawn_tcp().await;

    let mut params = BTreeMap::new();
    params.insert("id".into(), ITEM_ID.into());
    let http_path = expand_path(GetItem::PATH, &params).expect("expand");
    assert_eq!(http_path, format!("/v1/items/{ITEM_ID}"));

    let (http_status, http_headers, http_body) = http_get(http_addr, &http_path).await;
    assert_eq!(http_status, 200);
    let ctype = http_headers
        .iter()
        .find(|(k, _)| k == "content-type")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    assert!(ctype.contains("application/json"), "{ctype}");
    let http_rec = receipt_from_http_body(&http_body);
    assert_get_item_ok(&http_rec, Transport::Http);

    let mut tcp = TcpStream::connect(tcp_addr).await.expect("tcp connect");
    let tcp_call = get_item_call(ITEM_ID, Transport::Tcp, "tcp-get-item");
    tcp.write_all(tcp_call.to_ndjson().unwrap().as_bytes())
        .await
        .expect("tcp write");
    let ping = {
        let mut c = RpcCall::new("tcp-ping", RouteKey::TcpPing.as_str());
        c.transport = Some(Transport::Tcp);
        c
    };
    tcp.write_all(ping.to_ndjson().unwrap().as_bytes())
        .await
        .expect("tcp ping write");
    let mut tcp_buf = Vec::new();
    let mut tmp = [0u8; 4096];
    while tcp_buf.iter().filter(|&&b| b == b'\n').count() < 2 {
        let n = tcp.read(&mut tmp).await.expect("tcp read");
        assert!(n > 0, "tcp closed before two receipts");
        tcp_buf.extend_from_slice(&tmp[..n]);
    }
    let lines: Vec<&str> = std::str::from_utf8(&tcp_buf)
        .unwrap()
        .split('\n')
        .filter(|l| !l.is_empty())
        .collect();
    let tcp_rec = RpcReceipt::from_ndjson(lines[0]).expect("tcp receipt");
    assert_get_item_ok(&tcp_rec, Transport::Tcp);
    let ping_rec = RpcReceipt::from_ndjson(lines[1]).expect("ping receipt");
    assert!(ping_rec.ok);
    assert_eq!(ping_rec.key, "tcp_ping");
    assert_eq!(ping_rec.body.as_ref().unwrap()["pong"], true);

    let ws_url = format!("ws://{http_addr}{}", RouteKey::Websocket.path());
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");
    let ws_call = get_item_call(ITEM_ID, Transport::Websocket, "ws-get-item");
    ws.send(WsMessage::Text(
        serde_json::to_string(&ws_call).unwrap().into(),
    ))
    .await
    .expect("ws send");
    let ws_msg = ws.next().await.expect("ws frame").expect("ws ok");
    let WsMessage::Text(ws_text) = ws_msg else {
        panic!("expected text frame, got {ws_msg:?}");
    };
    let ws_rec: RpcReceipt = serde_json::from_str(ws_text.as_str()).expect("ws receipt json");
    ws_rec.validate().expect("ws receipt schema");
    assert_get_item_ok(&ws_rec, Transport::Websocket);

    assert_eq!(http_rec.body, tcp_rec.body);
    assert_eq!(tcp_rec.body, ws_rec.body);

    for rec in [&http_rec, &tcp_rec, &ws_rec] {
        let log = otel_shaped_log(rec);
        assert_eq!(log["fields"]["rpc.system"], RPC_SYSTEM);
        assert_eq!(log["fields"]["rpc.service"], rpc_routes::SERVICE);
        assert_eq!(log["fields"]["rpc.method"], "get_item");
        assert_eq!(log["fields"]["rpc.ok"], true);
        assert_eq!(log["fields"]["http.status_code"], 200);
        assert_eq!(log["traceId"], TRACE_ID);
        assert_eq!(log["spanId"], SPAN_ID);
        let fields = log["fields"].as_object().unwrap();
        assert!(!fields.contains_key("body"));
        assert!(!fields.contains_key("authorization"));
        assert!(fields.len() <= 256);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transport_guards_unknown_keys_and_docs_aliases() {
    let http_addr = spawn_http().await;
    let tcp_addr = spawn_tcp().await;

    let (st, _, body) = http_get(http_addr, RouteKey::Healthz.path()).await;
    assert_eq!(st, 200);
    let rec = receipt_from_http_body(&body);
    assert!(rec.ok);
    assert_eq!(rec.key, "healthz");

    let (st, _, _) = http_get(http_addr, RouteKey::TcpPing.path()).await;
    assert_eq!(st, 404, "tcp-only key must not be an HTTP route");

    let (st, headers, body) = http_get(http_addr, "/api/docs.json").await;
    assert_eq!(st, 200);
    assert_eq!(
        headers
            .iter()
            .find(|(k, _)| k == "cache-control")
            .map(|(_, v)| v.as_str()),
        Some("no-store")
    );
    let catalog: Value = serde_json::from_slice(&body).expect("catalog json");
    assert_eq!(catalog["map"]["get_item"]["path"], GetItem::PATH);
    assert_eq!(
        catalog["map"]["get_item"]["transports"],
        json!(["http", "tcp", "websocket"])
    );

    let mut tcp = TcpStream::connect(tcp_addr).await.unwrap();
    let mut health = RpcCall::new("tcp-healthz", "healthz");
    health.transport = Some(Transport::Tcp);
    tcp.write_all(health.to_ndjson().unwrap().as_bytes())
        .await
        .unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    while !buf.contains(&b'\n') {
        let n = tcp.read(&mut tmp).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&tmp[..n]);
    }
    let rec = RpcReceipt::from_ndjson(std::str::from_utf8(&buf).unwrap()).unwrap();
    assert!(!rec.ok);
    assert_eq!(rec.status, Some(405));
    assert_eq!(rec.error.as_ref().unwrap()["code"], "transport_not_allowed");

    let mut tcp = TcpStream::connect(tcp_addr).await.unwrap();
    let mystery = RpcCall::new("nope", "not_a_key");
    tcp.write_all(mystery.to_ndjson().unwrap().as_bytes())
        .await
        .unwrap();
    buf.clear();
    while !buf.contains(&b'\n') {
        let n = tcp.read(&mut tmp).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&tmp[..n]);
    }
    let rec = RpcReceipt::from_ndjson(std::str::from_utf8(&buf).unwrap()).unwrap();
    assert!(!rec.ok);
    assert_eq!(rec.status, Some(404));

    let mut mismatch = get_item_call(ITEM_ID, Transport::Http, "wrong-wire");
    mismatch.transport = Some(Transport::Http);
    let mut tcp = TcpStream::connect(tcp_addr).await.unwrap();
    tcp.write_all(mismatch.to_ndjson().unwrap().as_bytes())
        .await
        .unwrap();
    buf.clear();
    while !buf.contains(&b'\n') {
        let n = tcp.read(&mut tmp).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&tmp[..n]);
    }
    let rec = RpcReceipt::from_ndjson(std::str::from_utf8(&buf).unwrap()).unwrap();
    assert!(!rec.ok);
    assert_eq!(rec.error.as_ref().unwrap()["code"], "transport_mismatch");
}

#[test]
fn opto_sync_carries_the_map_not_the_calls() {
    let env = RouteMapEnvelope::wrap(route_map(), "1689940800123456789").unwrap();
    assert_eq!(env.scope, OPTO_SYNC_SCOPE);
    assert_eq!(env.record_id, rpc_routes::SERVICE);
    let back = env.into_map().unwrap();
    let entry = back.lookup(GetItem::KEY).unwrap();
    assert_eq!(entry.path, GetItem::PATH);
    assert_eq!(entry.transports, GetItem::TRANSPORTS);

    let call = get_item_call(ITEM_ID, Transport::Tcp, "must-not-be-payload");
    let call_json = serde_json::to_value(&call).unwrap();
    assert!(call_json.get("schema_version").is_none());
    assert!(call_json.get("map").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_json_body_is_the_same_call_frame_as_tcp_and_ws() {
    let http_addr = spawn_http().await;
    let tcp_addr = spawn_tcp().await;

    let mut http_call = get_item_call(ITEM_ID, Transport::Http, "rpc-http-body");
    let bytes = serde_json::to_vec(&http_call).unwrap();
    let (st, body) = http_post_json(http_addr, "/rpc", &bytes).await;
    assert_eq!(st, 200);
    let http_rec = receipt_from_http_body(&body);
    assert_get_item_ok(&http_rec, Transport::Http);
    assert_eq!(http_rec.id, "rpc-http-body");

    http_call.transport = None;
    http_call.id = "rpc-http-inferred".into();
    http_call.validate().unwrap();
    let (st, body) = http_post_json(http_addr, "/rpc", &serde_json::to_vec(&http_call).unwrap()).await;
    assert_eq!(st, 200);
    let inferred = receipt_from_http_body(&body);
    assert!(inferred.ok);
    assert_eq!(inferred.transport, Some(Transport::Http));
    assert_eq!(inferred.id, "rpc-http-inferred");

    let tcp_only = {
        let mut c = RpcCall::new("http-tcp-ping", RouteKey::TcpPing.as_str());
        c.transport = Some(Transport::Tcp);
        c
    };
    let (st, body) = http_post_json(http_addr, "/rpc", &serde_json::to_vec(&tcp_only).unwrap()).await;
    assert_eq!(st, 400);
    let rec = receipt_from_http_body(&body);
    assert_eq!(rec.error.as_ref().unwrap()["code"], "transport_mismatch");

    let mut omit = RpcCall::new("tcp-inferred", GetItem::KEY);
    omit.path = json!({ "id": ITEM_ID });
    omit.trace_id = Some(TRACE_ID.into());
    omit.span_id = Some(SPAN_ID.into());
    omit.validate().unwrap();
    let tcp_rec = tcp_exchange(tcp_addr, &omit).await;
    assert_get_item_ok(&tcp_rec, Transport::Tcp);
    assert_eq!(tcp_rec.id, "tcp-inferred");
    assert_eq!(http_rec.body, tcp_rec.body);

    let mut slash = get_item_call("a/b", Transport::Http, "slash-id");
    slash.path = json!({ "id": "a/b" });
    let (st, body) = http_post_json(http_addr, "/rpc", &serde_json::to_vec(&slash).unwrap()).await;
    assert_eq!(st, 200);
    let slash_rec = receipt_from_http_body(&body);
    assert_eq!(slash_rec.body.as_ref().unwrap()["id"], "a/b");

    let missing = RpcCall::new("no-path", GetItem::KEY);
    let (st, body) =
        http_post_json(http_addr, "/rpc", &serde_json::to_vec(&missing).unwrap()).await;
    assert_eq!(st, 400);
    let rec = receipt_from_http_body(&body);
    assert_eq!(rec.error.as_ref().unwrap()["code"], "missing_path_id");
    assert_eq!(rec.id, "no-path");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_tcp_and_pipelined_websocket_keep_correlation_ids() {
    let http_addr = spawn_http().await;
    let tcp_addr = spawn_tcp().await;

    let left = {
        let mut c = get_item_call("alpha", Transport::Tcp, "tcp-a");
        c.path = json!({ "id": "alpha" });
        c
    };
    let right = {
        let mut c = get_item_call("beta", Transport::Tcp, "tcp-b");
        c.path = json!({ "id": "beta" });
        c
    };
    let (a, b) = tokio::join!(tcp_exchange(tcp_addr, &left), tcp_exchange(tcp_addr, &right));
    assert_eq!(a.id, "tcp-a");
    assert_eq!(a.body.as_ref().unwrap()["id"], "alpha");
    assert_eq!(b.id, "tcp-b");
    assert_eq!(b.body.as_ref().unwrap()["id"], "beta");

    let ws_url = format!("ws://{http_addr}{}", RouteKey::Websocket.path());
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");
    for (call_id, item) in [("ws-1", "one"), ("ws-2", "two")] {
        let mut call = get_item_call(item, Transport::Websocket, call_id);
        call.path = json!({ "id": item });
        ws.send(WsMessage::Text(
            serde_json::to_string(&call).unwrap().into(),
        ))
        .await
        .expect("ws send");
        let msg = ws.next().await.expect("ws frame").expect("ws ok");
        let WsMessage::Text(text) = msg else {
            panic!("expected text, got {msg:?}");
        };
        let rec: RpcReceipt = serde_json::from_str(text.as_str()).unwrap();
        rec.validate().unwrap();
        assert_eq!(rec.id, call_id);
        assert_eq!(rec.key, "get_item");
        assert_eq!(rec.body.as_ref().unwrap()["id"], item);
        assert!(rec.ok);
    }

    let mut upgrade = RpcCall::new("ws-upgrade", RouteKey::Websocket.as_str());
    upgrade.transport = Some(Transport::Websocket);
    ws.send(WsMessage::Text(
        serde_json::to_string(&upgrade).unwrap().into(),
    ))
    .await
    .unwrap();
    let msg = ws.next().await.expect("ws frame").expect("ws ok");
    let WsMessage::Text(text) = msg else {
        panic!("expected text");
    };
    let rec: RpcReceipt = serde_json::from_str(text.as_str()).unwrap();
    assert!(!rec.ok);
    assert_eq!(rec.error.as_ref().unwrap()["code"], "upgrade_only");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_ndjson_and_encoded_http_path() {
    let http_addr = spawn_http().await;
    let tcp_addr = spawn_tcp().await;

    let mut stream = TcpStream::connect(tcp_addr).await.unwrap();
    stream.write_all(b"{not-json\n").await.unwrap();
    let mut buf = Vec::new();
    let n = tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut buf))
        .await
        .expect("malformed ndjson should close")
        .expect("read");
    assert_eq!(n, 0, "server must not emit a receipt for invalid JSON: {buf:?}");

    let mut slash = BTreeMap::new();
    slash.insert("id".into(), "a/b".into());
    assert_eq!(
        expand_path(GetItem::PATH, &slash).unwrap(),
        "/v1/items/a%2Fb"
    );

    let mut spaced = BTreeMap::new();
    spaced.insert("id".into(), "item 42".into());
    let path = expand_path(GetItem::PATH, &spaced).unwrap();
    assert_eq!(path, "/v1/items/item%2042");
    let (st, _, body) = http_get(http_addr, &path).await;
    assert_eq!(st, 200);
    let rec = receipt_from_http_body(&body);
    assert!(rec.ok);
    assert_eq!(rec.body.as_ref().unwrap()["id"], "item 42");

    let crlf = get_item_call(ITEM_ID, Transport::Tcp, "tcp-crlf");
    let mut line = crlf.to_ndjson().unwrap();
    line.pop();
    line.push_str("\r\n");
    let mut stream = TcpStream::connect(tcp_addr).await.unwrap();
    stream.write_all(line.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    while !buf.contains(&b'\n') {
        let n = stream.read(&mut tmp).await.unwrap();
        assert!(n > 0, "crlf ndjson should still produce a receipt");
        buf.extend_from_slice(&tmp[..n]);
    }
    let rec = RpcReceipt::from_ndjson(std::str::from_utf8(&buf).unwrap()).unwrap();
    assert_get_item_ok(&rec, Transport::Tcp);
    assert_eq!(rec.id, "tcp-crlf");
}
