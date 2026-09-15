import {
  Routes as RpcRoutes,
  type RouteTypes as RpcRouteTypes,
} from "../../../generated/typescript/rpc_transports.js";
import {
  Routes as PmapRoutes,
  type RouteTypes as PmapRouteTypes,
} from "../../../generated/typescript/pmap_api.js";
import {
  createTypedRpcClient,
  type RpcTransportInvoke,
} from "./typed-rpc.js";

declare const invoke: RpcTransportInvoke;

const tcp = createTypedRpcClient<typeof RpcRoutes, RpcRouteTypes, "tcp">({
  routes: RpcRoutes,
  transport: "tcp",
  invoke,
});

// `get_item` declares TCP and carries its generated path/header surface.
tcp.call("get_item", {
  path: { id: "42" },
  headers: { "x-request-id": "req-1" },
});

// @ts-expect-error healthz is HTTP-only in the generated route map.
tcp.call("healthz", {});

// @ts-expect-error get_item requires the generated path id.
tcp.call("get_item", { headers: { "x-request-id": "req-1" } });

// @ts-expect-error the generated header surface requires x-request-id.
tcp.call("get_item", { path: { id: "42" }, headers: {} });

// @ts-expect-error GET is the only method binding for get_item.
tcp.call("get_item", {
  path: { id: "42" },
  headers: { "x-request-id": "req-1" },
  method: "POST",
});

const http = createTypedRpcClient<typeof PmapRoutes, PmapRouteTypes, "http">({
  routes: PmapRoutes,
  transport: "http",
  invoke,
});

const counsel: Promise<{
  round_table: Record<string, unknown>;
  providers_configured: Array<string>;
}> = http.call("AskCounsel", {
  body: { matter_id: "matter-1", scope: "review" },
});

void counsel;

// @ts-expect-error request body is selected by the operation key.
http.call("AskCounsel", { body: { matter_id: 123 } });
