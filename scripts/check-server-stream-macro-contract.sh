#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ores-stream-compile.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

write_manifest() {
  local dir="$1"
  local name="$2"
  mkdir -p "$dir/src"
  cat >"$dir/Cargo.toml" <<EOF
[package]
name = "$name"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
ores-api-docs = { path = "$ROOT/rust", default-features = false, features = ["operation-runtime"] }
ores-api-docs-operation-macros = { path = "$ROOT/macros/operation-rust" }
serde = { version = "1", features = ["derive"] }
EOF
}

common_prefix='use ores_api_docs::{NoSection, OperationSpec, RpcPayloadCodec, RpcStreamMode, ServerStreamResult, TypedOperationContext};
use ores_api_docs_operation_macros::ores_operation;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WatchEvent { value: String }

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WatchError { code: String }

struct WatchEvents;
'

write_spec() {
  local stream="$1"
  cat <<EOF
impl OperationSpec for WatchEvents {
    type Path = NoSection;
    type Query = NoSection;
    type RequestHeaders = NoSection;
    type RequestBody = NoSection;
    type ResponseBody = WatchEvent;
    type ResponseHeaders = NoSection;
    type ResponseTrailers = NoSection;
    type Error = WatchError;

    const KEY: &'static str = "demo.events.watch_stream";
    const CODECS: &'static [RpcPayloadCodec] = &[RpcPayloadCodec::Json];
    const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Json;
    const STREAM: RpcStreamMode = RpcStreamMode::$stream;
}
EOF
}

# Positive canonical server-stream handler.
POS="$WORK/positive"
write_manifest "$POS" "stream-shape-positive"
{
  printf '%s\n' "$common_prefix"
  write_spec ServerStream
  cat <<'EOF'

#[ores_operation(
    spec = WatchEvents,
    key = "demo.events.watch_stream",
    stream = "server_stream"
)]
async fn watch_stream(
    _ctx: TypedOperationContext<(), WatchEvents>,
) -> ServerStreamResult<WatchEvents> {
    todo!()
}
EOF
} >"$POS/src/lib.rs"
cargo check --quiet --manifest-path "$POS/Cargo.toml"

# A Result<T,E> handler under server_stream must be rejected by the proc macro,
# before ores-stack generation or runtime dispatch can become involved.
BAD_SHAPE="$WORK/bad-shape"
write_manifest "$BAD_SHAPE" "stream-shape-negative"
{
  printf '%s\n' "$common_prefix"
  write_spec ServerStream
  cat <<'EOF'

#[ores_operation(
    spec = WatchEvents,
    key = "demo.events.watch_stream",
    stream = "server_stream"
)]
async fn watch_stream(
    _ctx: TypedOperationContext<(), WatchEvents>,
) -> Result<WatchEvent, WatchError> {
    todo!()
}
EOF
} >"$BAD_SHAPE/src/lib.rs"
if cargo check --quiet --manifest-path "$BAD_SHAPE/Cargo.toml" >"$BAD_SHAPE/out" 2>"$BAD_SHAPE/err"; then
  echo 'server_stream accepted unary Result<T,E> return shape' >&2
  exit 1
fi
grep -Fq '#[ores_operation(stream = "server_stream")] requires return type ServerStreamResult<OperationSpec>' "$BAD_SHAPE/err"

# Macro metadata and generated OperationSpec::STREAM must agree.
BAD_SPEC="$WORK/bad-spec"
write_manifest "$BAD_SPEC" "stream-spec-negative"
{
  printf '%s\n' "$common_prefix"
  write_spec Unary
  cat <<'EOF'

#[ores_operation(
    spec = WatchEvents,
    key = "demo.events.watch_stream",
    stream = "server_stream"
)]
async fn watch_stream(
    _ctx: TypedOperationContext<(), WatchEvents>,
) -> ServerStreamResult<WatchEvents> {
    todo!()
}
EOF
} >"$BAD_SPEC/src/lib.rs"
if cargo check --quiet --manifest-path "$BAD_SPEC/Cargo.toml" >"$BAD_SPEC/out" 2>"$BAD_SPEC/err"; then
  echo 'server_stream metadata accepted unary OperationSpec::STREAM' >&2
  exit 1
fi
grep -Fq 'metadata stream mode disagrees with OperationSpec::STREAM' "$BAD_SPEC/err"

# The inverse mismatch must also fail: a streaming spec cannot be exposed as a
# unary operation merely because the function happens to return Result<T,E>.
BAD_UNARY="$WORK/bad-unary"
write_manifest "$BAD_UNARY" "unary-spec-negative"
{
  printf '%s\n' "$common_prefix"
  write_spec ServerStream
  cat <<'EOF'

#[ores_operation(
    spec = WatchEvents,
    key = "demo.events.watch",
    stream = "unary"
)]
async fn watch(
    _ctx: TypedOperationContext<(), WatchEvents>,
) -> Result<WatchEvent, WatchError> {
    todo!()
}
EOF
} >"$BAD_UNARY/src/lib.rs"
if cargo check --quiet --manifest-path "$BAD_UNARY/Cargo.toml" >"$BAD_UNARY/out" 2>"$BAD_UNARY/err"; then
  echo 'unary metadata accepted server-stream OperationSpec::STREAM' >&2
  exit 1
fi
grep -Fq 'metadata stream mode disagrees with OperationSpec::STREAM' "$BAD_UNARY/err"

printf '%s\n' 'server-stream compile-time contract: PASS'
