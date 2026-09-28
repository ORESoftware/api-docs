# Lambda and RPC response contracts

A lambda/RPC operation has three independent contract dimensions. They MUST NOT be inferred from the handler body or collapsed into one setting:

1. **semantic response type** — `OperationSpec::ResponseBody`;
2. **invocation shape** — unary vs server/client/bidirectional stream through `OperationSpec::STREAM` and `#[ores_operation(stream = ...)]`;
3. **response representation / wire media** — `OperationSpec::RESPONSE_REPRESENTATION`, `OperationSpec::RESPONSE_CONTENT_TYPE`, and the negotiated RPC payload codec.

This separation is what lets one semantic operation drive a direct lambda invocation, an HTTP route, `/v1/rpc`, generated clients, API docs, and MCP/RPC docs without inventing a second handler contract.

## One source of truth

The `#[ores_operation(spec = ...)]` attribute binds an authored handler to a generated `OperationSpec`. New generated code SHOULD write `stream = "unary"` explicitly even though unary remains a compatibility default. Non-unary operations MUST declare their stream mode explicitly.

The operation spec owns the exact response body type and response representation. Do not duplicate those values as unrelated handler-local strings. The normalized operation IR / TypeSpec / JSON Schema authorities generate the spec and the client surfaces from the same contract.

### Typed JSON / MessagePack / Protobuf response

```rust
pub struct GetWidgetOperation;

impl OperationSpec for GetWidgetOperation {
    type Path = NoSection;
    type Query = NoSection;
    type RequestHeaders = NoSection;
    type RequestBody = GetWidgetRequest;
    type ResponseBody = Widget;
    type ResponseHeaders = NoSection;
    type ResponseTrailers = NoSection;
    type Error = GetWidgetError;

    const KEY: &'static str = "widgets.get";
    const CODECS: &'static [RpcPayloadCodec] = &[
        RpcPayloadCodec::Json,
        RpcPayloadCodec::Messagepack,
        RpcPayloadCodec::Protobuf,
    ];
    const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Messagepack;
    const RESPONSE_REPRESENTATION: OperationResponseRepresentation =
        OperationResponseRepresentation::Structured;
    const RESPONSE_CONTENT_TYPE: Option<&'static str> = Some("application/json");
    const STREAM: RpcStreamMode = RpcStreamMode::Unary;
}

#[ores_operation(
    spec = GetWidgetOperation,
    key = "widgets.get",
    codecs("json", "messagepack", "protobuf"),
    default_codec = "messagepack",
    audiences("browser", "server"),
    scope = "regular",
    stream = "unary"
)]
pub async fn get_widget(
    ctx: TypedOperationContext<AppState, GetWidgetOperation>,
) -> Result<Widget, GetWidgetError> {
    // ...
}
```

`Structured` describes the semantic response. `RpcPayloadCodec` describes how that typed value is serialized across RPC. Supporting MessagePack or Protobuf does not turn the operation into a different semantic response type.

### HTML response

```rust
impl OperationSpec for RenderArticleOperation {
    // request and header sections omitted
    type ResponseBody = String;

    const RESPONSE_REPRESENTATION: OperationResponseRepresentation =
        OperationResponseRepresentation::Html;
    const RESPONSE_CONTENT_TYPE: Option<&'static str> =
        Some("text/html; charset=utf-8");
    const STREAM: RpcStreamMode = RpcStreamMode::Unary;

    // remaining OperationSpec fields omitted
}

#[ores_operation(
    spec = RenderArticleOperation,
    key = "articles.render",
    codecs("json"),
    default_codec = "json",
    audiences("browser", "server"),
    scope = "regular",
    stream = "unary"
)]
pub async fn render_article(
    ctx: TypedOperationContext<AppState, RenderArticleOperation>,
) -> Result<String, RenderError> {
    // ...
}
```

A direct HTTP/lambda adapter writes that string as HTML using the declared media type. An RPC transport may still carry the typed `String` inside its negotiated RPC framing; generated clients return the typed string rather than pretending the result is a JSON object.

### Streaming response

```rust
impl OperationSpec for WatchEventsOperation {
    // request and header sections omitted
    type ResponseBody = EventChunk;

    const RESPONSE_REPRESENTATION: OperationResponseRepresentation =
        OperationResponseRepresentation::Structured;
    const STREAM: RpcStreamMode = RpcStreamMode::ServerStream;

    // remaining OperationSpec fields omitted
}

#[ores_operation(
    spec = WatchEventsOperation,
    key = "events.watch_stream",
    codecs("json", "messagepack", "protobuf"),
    default_codec = "messagepack",
    audiences("browser", "server"),
    scope = "regular",
    stream = "server_stream"
)]
pub async fn watch_events_stream(
    ctx: TypedOperationContext<AppState, WatchEventsOperation>,
) -> ServerStreamResult<WatchEventsOperation> {
    // ...
}
```

For a server stream, `ResponseBody` is **one emitted item/chunk**, never an implicitly buffered aggregate. The generated client surface must therefore distinguish:

- unary: `call(...) -> Result<ResponseBody, Error>`;
- server stream: `call_stream(...) -> Stream<Result<ResponseBody, Error>>`.

A binary stream uses `ResponseBody = Vec<u8>`, `OperationResponseRepresentation::Binary`, an explicit media type such as `application/octet-stream`, and a streaming mode. A text/HTML stream similarly declares `Text`/`Html`; each chunk has the declared representation.

## RPC codecs versus HTTP media types

The following are separate concepts:

| Contract | Examples | Purpose |
| --- | --- | --- |
| semantic representation | structured, HTML, text, binary | tells adapters/docs/clients what one response value means |
| RPC payload codec | JSON, MessagePack, Protobuf | serializes typed RPC request/response values |
| HTTP/lambda media type | `application/json`, `text/html`, `application/octet-stream` | controls direct HTTP/lambda body representation |
| stream mode | unary, server stream, client stream, bidi | controls invocation/framing API |

Generated code and documentation MUST preserve all four dimensions instead of deriving one from another.

## Code generation rules

`oresoftware/api-docs` generators should use the normalized contract to emit:

- the Rust `OperationSpec` response associated type and representation constants;
- TypeScript/Dart/Go/Gleam/Rust client result types;
- distinct unary and streaming client methods;
- OpenAPI response media types for HTTP projections;
- RPC/OpenRPC/MCP metadata describing codecs and stream mode;
- provider lambda adapters that write unary bodies or stream typed chunks without buffering a stream into a unary result.

Legacy specs may omit the new representation constants and therefore resolve to `Structured` with no fixed HTTP content type. Newly generated externally callable operations should emit them explicitly.
