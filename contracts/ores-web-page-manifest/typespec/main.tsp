import "@typespec/json-schema";
using TypeSpec.JsonSchema;

namespace OresWebPageManifest;

// Independent authored authority for the deterministic browser-page source/docs
// manifest. This file must not be generated from the authored JSON Schema peer.

@id("WebPageSourceDigestV1")
model WebPageSourceDigestV1 {
  @minLength(1)
  source: string;

  @pattern("^[0-9a-f]{64}$")
  sha256: string;
}

@id("WebPageManifestEntryV1")
model WebPageManifestEntryV1 {
  @minLength(1)
  @pattern("^/")
  canonicalUrl: string;

  @minLength(1)
  sourcePageRs: string;

  @pattern("^[0-9a-f]{64}$")
  pageSourceSha256: string;

  // Ordered root-to-leaf inherited layout.rs source identities.
  layoutSources: WebPageSourceDigestV1[];

  // Digest of page.rs + ordered layoutSources only. gen.rs and finalized
  // CSS/WASM artifacts deliberately have separate identities.
  @pattern("^[0-9a-f]{64}$")
  renderSha256: string;

  @minLength(1)
  sourceGenRs?: string;

  @pattern("^[0-9a-f]{64}$")
  sourceGenSha256?: string;

  axumPaths: string[];
  renderer: "mash" | "leptos" | "dioxus";
  delivery: "ssr_only" | "client_only" | "ssr_hydrate";
  renderMode: "dynamic" | "static_only" | "static_with_fallback";

  // JSON/JavaScript tooling must round-trip this value exactly.
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

  // Digest of the normalized pages array, including render/gen source
  // identities and route/docs metadata. Finalized CSS/WASM artifacts remain in
  // the page build manifest and are not falsely represented here.
  @pattern("^[0-9a-f]{64}$")
  manifestSha256: string;

  pages: WebPageManifestEntryV1[];
}
