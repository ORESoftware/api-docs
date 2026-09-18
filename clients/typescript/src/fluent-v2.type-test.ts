// Compile-time proof of the type-state. Every `@ts-expect-error` below is an
// assertion that the marked line does NOT compile: if the narrowing regresses
// and the call becomes legal, tsc fails with "unused '@ts-expect-error'".

import type {
  RpcStreamCallBuilder,
  RpcUnaryCallBuilder,
} from "./options.generated.js";
import type { RpcOutcome, RpcRequestPlan, RpcStreamHandle } from "./fluent-types.js";

declare const unary: RpcUnaryCallBuilder<{ id: string }>;
declare const streaming: RpcStreamCallBuilder<{ n: number }>;

// --- the two surfaces are disjoint -----------------------------------------

// @ts-expect-error a unary chain has no stream() terminal
unary.stream();

// @ts-expect-error a streaming chain has no makeCall() terminal
streaming.makeCall();

// @ts-expect-error unary-only shaping is absent from the streaming surface
streaming.withFallback({ n: 0 });

// @ts-expect-error stream-only shaping is absent from the unary surface
unary.withBackpressure("buffer");

// @ts-expect-error debounce is unary-only; streams use debounceEach
streaming.debounce(50);

// @ts-expect-error dryRun is unary-only
streaming.dryRun();

// --- serialization is a one-way door ---------------------------------------

const json = unary.useJson();
// @ts-expect-error the serialization group is spent
json.useProtobuf();
// @ts-expect-error the serialization group is spent
json.useJson();
// @ts-expect-error the serialization group is spent
json.useSerialStrategy("protobuf");
// @ts-expect-error the serialization group is spent
json.useMessagePack();

const viaValue = unary.useSerialStrategy("message_pack");
// @ts-expect-error selecting by value spends the same group
viaValue.useJson();

// --- the other exclusive groups behave the same ----------------------------

const anonymous = unary.omitAuth();
// @ts-expect-error a per-call credential contradicts omitAuth
anonymous.withBearerToken("t");
// @ts-expect-error omitAuth cannot be spent twice
anonymous.omitAuth();

const v4 = unary.forceIpv4();
// @ts-expect-error the address family is already pinned
v4.forceIpv6();

const throttled = unary.throttle(200);
// @ts-expect-error throttle and debounce are contradictory
throttled.debounce(200);

const sampled = streaming.sampleEach(100);
// @ts-expect-error inbound rate shaping is already chosen
sampled.throttleEach(100);
// @ts-expect-error inbound rate shaping is already chosen
sampled.debounceEach(100);

// --- what must still compile ------------------------------------------------

// Spending one group leaves the others reachable, in any order.
const configured = unary
  .useProtobuf()
  .omitAuth()
  .forceIpv6()
  .debounce(30)
  .withTimeout(2_000)
  .withRetries(3)
  .withRetryBackoff(100, 2)
  .addPathField("user_id", "user-42")
  .withTraceId("4bf92f3577b34da6a3ce929d0e0e4736")
  .queuePriority(3)
  .compress("gzip")
  .onRetry((attempt: number, cause: unknown) => void [attempt, cause]);

const outcome: Promise<RpcOutcome<{ id: string }>> = configured.makeCall();
const value: Promise<{ id: string }> = configured.makeCallOrThrow();
const plan: RpcRequestPlan = configured.toPlan();

const opened: Promise<RpcStreamHandle<{ n: number }>> = streaming
  .useMessagePack()
  .withBackpressure("drop_oldest")
  .withStreamBuffer(256)
  .withStreamIdleTimeout(30_000)
  .sampleEach(100)
  .stream();

// Repeatable options stay repeatable.
const repeated = unary.addHeader("x-a", "1").addHeader("x-b", "2").addQueryField("q", "v");

// Enum parameters are constrained to their catalog variants.
// @ts-expect-error "yaml" is not a declared serialization strategy
unary.useSerialStrategy("yaml");
// @ts-expect-error 9 is not a declared queue priority
unary.queuePriority(9);
// @ts-expect-error "lz4" is not a declared compression
unary.compress("lz4");

export type _Checks = [
  typeof outcome,
  typeof value,
  typeof plan,
  typeof opened,
  typeof repeated,
];
