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

/// Next-style rendering policy expressed without coupling pages to a specific
/// Rust UI framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRenderMode {
    /// Render every request on the server.
    Dynamic,
    /// Only paths returned by `generate_static_params` exist in production.
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
/// implementations do not need to know whether the adapter is Axum, Leptos, or
/// Dioxus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageConfig {
    pub render_mode: PageRenderMode,
    pub revalidate: RevalidationPolicy,
}

impl PageConfig {
    pub const DYNAMIC: Self = Self {
        render_mode: PageRenderMode::Dynamic,
        revalidate: RevalidationPolicy::Never,
    };

    pub const STATIC_ONLY: Self = Self {
        render_mode: PageRenderMode::StaticOnly,
        revalidate: RevalidationPolicy::Never,
    };

    pub const fn static_with_fallback(revalidate: RevalidationPolicy) -> Self {
        Self {
            render_mode: PageRenderMode::StaticWithFallback,
            revalidate,
        }
    }
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
/// paths here are repository-relative source inputs, never public URLs.
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

/// Exact free-function signatures required in every `src/pages/**/page.rs`.
/// Generated compile glue assigns each discovered function to these aliases, so
/// wrong/missing exports fail normal Rust compilation.
pub type PageFn = fn(PageContext) -> PageResult;
pub type PrerenderFn = fn(&PrerenderContext) -> PrerenderResult;
/// Next.js `generateStaticParams` equivalent for Rust pages.
pub type GenerateStaticParamsFn = PrerenderFn;
pub type PageConfigFn = fn() -> PageConfig;
pub type PageAssetsFn = fn() -> PageAssets;
