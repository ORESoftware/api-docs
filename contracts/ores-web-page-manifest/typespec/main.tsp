import "@typespec/json-schema";
using TypeSpec.JsonSchema;

namespace OresWebPageManifest;

// Independent authored authority for the deterministic browser-page manifest.
// This file must not be generated from the authored JSON Schema peer.

@id("WebPageManifestEntryV1")
model WebPageManifestEntryV1 {
  @minLength(1)
  @pattern("^/")
  canonicalUrl: string;

  @minLength(1)
  sourcePageRs: string;

  @minLength(1)
  sourceGenRs?: string;

  @pattern("^[0-9a-f]{64}$")
  sourceSha256: string;

  axumPaths: string[];
  renderer: "mash" | "leptos" | "dioxus";
  delivery: "ssr_only" | "client_only" | "ssr_hydrate";
  renderMode: "dynamic" | "static_only" | "static_with_fallback";

  // This value crosses the JSON/JavaScript tooling boundary in generated page
  // manifests, so its wire domain is the exact interoperable integer range,
  // not the larger Rust storage type. Both bounds are exactly representable by
  // the TypeSpec compiler/JSON tooling; the Rust projection must fail closed if
  // an authored u64 value exceeds this safe-integer contract.
  @minValue(1)
  @maxValue(9007199254740991)
  revalidateSecs?: safeint;
  onDemand?: string;
  title?: string;
  summary?: string;
  auth: "public" | "optional_session" | "session" | "admin";
  stability: string;
  database: "none" | "read_only";
  features: string[];
  dataSources: string[];
  tags: string[];

  // Outbound dependencies only. A page never owns an RPC operation identity.
  rpcDependencies: string[];
}

@id("WebPageManifestV1")
model WebPageManifestV1 {
  schema: "ores.web.page-manifest/v1";
  @minLength(1)
  generatedBy: string;
  routeRoot: "src/pages";
  @pattern("^[0-9a-f]{64}$")
  manifestSha256: string;
  pages: WebPageManifestEntryV1[];
}
