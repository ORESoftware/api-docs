use std::collections::BTreeMap;

/// Runtime/build context passed to every filesystem page.
///
/// Route parameters are populated only from the validated filesystem route
/// pattern. Query/body data belongs to the normal HTTP layer and is not a route
/// discriminator.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageContext {
    pub route_params: BTreeMap<String, String>,
    pub request_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrerenderContext {
    /// Digest of the normalized filesystem route manifest.
    pub route_manifest_sha256: String,
    /// Optional sibling `api-docs` contract digest when enumeration reads API data.
    pub rpc_contract_sha256: Option<String>,
    /// Stable remote snapshot/version. Release prerendering must not consume an
    /// unversioned mutable source.
    pub source_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrerenderPath {
    pub route_params: BTreeMap<String, String>,
}

/// Which Rust renderer owns a browser route. This is deliberately per-page so
/// MASH/Maud+HTMX, Leptos, and Dioxus can coexist in one web server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRenderer {
    Mash,
    Leptos,
    Dioxus,
}

/// Browser delivery mode is independent from the renderer. A Leptos or Dioxus
/// route can be SSR-only, CSR-only, or SSR + hydration; MASH normally uses
/// `SsrOnly`, but may opt into a small Rust-WASM client or HTMX-enhanced client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDelivery {
    SsrOnly,
    ClientOnly,
    SsrAndHydrate,
}

/// Next-style rendering policy expressed without coupling pages to a specific
/// Rust UI framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRenderMode {
    /// Render every request at runtime.
    Dynamic,
    /// Only paths returned by sibling `gen.rs::generate_static_params` exist in production.
    StaticOnly,
    /// Pre-render known paths and SSR/cache unknown paths on first request.
    StaticWithFallback,
}

/// Cache regeneration policy for pre-rendered pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevalidationPolicy {
    /// Immutable until the next deployment/build.
    Never,
    /// Regenerate after the cached document reaches this age.
    AfterSeconds(u64),
    /// Regenerate only when an authenticated invalidation event names this tag.
    OnDemand(&'static str),
}

/// Per-page build/runtime contract. Generated router glue reads this value; page
/// implementations do not need to know how the final Axum/Leptos/Dioxus router
/// is assembled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageConfig {
    pub renderer: PageRenderer,
    pub delivery: PageDelivery,
    pub render_mode: PageRenderMode,
    pub revalidate: RevalidationPolicy,
}

impl PageConfig {
    pub const fn new(renderer: PageRenderer, delivery: PageDelivery) -> Self {
        Self {
            renderer,
            delivery,
            render_mode: PageRenderMode::Dynamic,
            revalidate: RevalidationPolicy::Never,
        }
    }

    pub const fn static_only(renderer: PageRenderer, delivery: PageDelivery) -> Self {
        Self {
            renderer,
            delivery,
            render_mode: PageRenderMode::StaticOnly,
            revalidate: RevalidationPolicy::Never,
        }
    }

    pub const fn static_with_fallback(
        renderer: PageRenderer,
        delivery: PageDelivery,
        revalidate: RevalidationPolicy,
    ) -> Self {
        Self {
            renderer,
            delivery,
            render_mode: PageRenderMode::StaticWithFallback,
            revalidate,
        }
    }

    /// Migration-friendly default for existing Maud/Axum pages.
    pub const MASH_SSR: Self = Self::new(PageRenderer::Mash, PageDelivery::SsrOnly);
}

/// Framework-neutral SSR result. MASH/Maud, Leptos, and Dioxus adapters render
/// their native view into this boundary. The browser receives only the assets
/// explicitly attached to the matched page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDocument {
    pub html: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

impl PageDocument {
    pub fn html(html: impl Into<String>) -> Self {
        Self {
            html: html.into(),
            status: 200,
            headers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageClientKind {
    None,
    RustWasm,
}

/// Authored asset inputs adjacent to one `page.rs`.
///
/// The bundler fingerprints/copies these into a route-scoped generated output;
/// paths here are repository-relative source inputs, never public URLs. CSS and
/// WASM are emitted per route so unrelated framework/runtime code is not sent to
/// the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageAssets {
    pub client: PageClientKind,
    pub client_entry: Option<&'static str>,
    pub static_inputs: &'static [&'static str],
}

impl PageAssets {
    pub const NONE: Self = Self {
        client: PageClientKind::None,
        client_entry: None,
        static_inputs: &[],
    };

    pub const fn rust_wasm(
        client_entry: &'static str,
        static_inputs: &'static [&'static str],
    ) -> Self {
        Self {
            client: PageClientKind::RustWasm,
            client_entry: Some(client_entry),
            static_inputs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageError {
    InvalidInput(String),
    Render(String),
    Prerender(String),
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(value) => write!(f, "invalid page input: {value}"),
            Self::Render(value) => write!(f, "page render failed: {value}"),
            Self::Prerender(value) => write!(f, "page prerender failed: {value}"),
        }
    }
}

impl std::error::Error for PageError {}

pub type PageResult = Result<PageDocument, PageError>;
pub type PrerenderResult = Result<Vec<PrerenderPath>, PageError>;

/// Exact free-function signatures generated glue validates.
///
/// `page.rs` exports `page`, `config`, and `assets`. Optional sibling `gen.rs`
/// exports `generate_static_params`; keeping enumeration out of `page.rs` makes
/// build-time data discovery an explicit separate concern.
pub type PageFn = fn(PageContext) -> PageResult;
pub type PrerenderFn = fn(&PrerenderContext) -> PrerenderResult;
pub type GenerateStaticParamsFn = PrerenderFn;
pub type PageConfigFn = fn() -> PageConfig;
pub type PageAssetsFn = fn() -> PageAssets;
