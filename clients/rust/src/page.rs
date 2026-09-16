use std::{any::Any, collections::BTreeMap, fmt, future::Future, pin::Pin, sync::Arc};

/// Type-erased state carried through the framework-neutral page ABI.
///
/// `api-docs` must not depend on a product's concrete `AppState`, RPC client,
/// ORM pool, or build-snapshot type. The generated Axum adapter stores its
/// concrete state here and page code recovers it with `ctx.state::<AppState>()`.
/// This keeps the shared page signature stable while preserving typed access at
/// the product boundary.
#[derive(Clone, Default)]
pub struct PageState(Option<Arc<dyn Any + Send + Sync>>);

impl PageState {
    #[must_use]
    pub fn new<T>(state: T) -> Self
    where
        T: Any + Send + Sync,
    {
        Self(Some(Arc::new(state)))
    }

    #[must_use]
    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.0.as_ref()?.clone().downcast::<T>().ok()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

impl fmt::Debug for PageState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageState")
            .field("present", &self.0.is_some())
            .finish()
    }
}

/// Runtime context passed to every filesystem page.
///
/// Route parameters are populated only from the validated filesystem route
/// pattern. `state` contains the concrete web-server state supplied by the
/// generated framework adapter. SSR pages may use it to reach the long-lived
/// sibling-API RPC pool and/or read-only sibling `*-orm-core` handles.
#[derive(Debug, Clone, Default)]
pub struct PageContext {
    pub route_params: BTreeMap<String, String>,
    pub request_path: String,
    pub state: PageState,
}

impl PageContext {
    #[must_use]
    pub fn new(route_params: BTreeMap<String, String>, request_path: impl Into<String>) -> Self {
        Self {
            route_params,
            request_path: request_path.into(),
            state: PageState::default(),
        }
    }

    #[must_use]
    pub fn with_state<T>(
        route_params: BTreeMap<String, String>,
        request_path: impl Into<String>,
        state: T,
    ) -> Self
    where
        T: Any + Send + Sync,
    {
        Self {
            route_params,
            request_path: request_path.into(),
            state: PageState::new(state),
        }
    }

    #[must_use]
    pub fn state<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.state.get::<T>()
    }
}

/// Build-time context passed to sibling `gen.rs`.
///
/// The optional typed state is for deterministic adapters such as a pinned API
/// snapshot or read-only build database. Release enumeration must still name a
/// stable `source_version`; a state object is not permission to read mutable
/// live data silently.
#[derive(Debug, Clone, Default)]
pub struct PrerenderContext {
    /// Digest of the normalized filesystem route manifest.
    pub route_manifest_sha256: String,
    /// Optional sibling `api-docs` contract digest when enumeration reads API data.
    pub rpc_contract_sha256: Option<String>,
    /// Stable remote snapshot/version. Release prerendering must not consume an
    /// unversioned mutable source.
    pub source_version: Option<String>,
    pub state: PageState,
}

impl PrerenderContext {
    #[must_use]
    pub fn state<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.state.get::<T>()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrerenderPath {
    pub route_params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRenderer {
    Mash,
    Leptos,
    Dioxus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDelivery {
    SsrOnly,
    ClientOnly,
    SsrAndHydrate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRenderMode {
    Dynamic,
    StaticOnly,
    StaticWithFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevalidationPolicy {
    Never,
    AfterSeconds(u64),
    OnDemand(&'static str),
}

/// Per-page runtime/build contract emitted by `#[ores_page]`.
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

    pub const MASH_SSR: Self = Self::new(PageRenderer::Mash, PageDelivery::SsrOnly);
}

/// Documentation and feature metadata emitted by `#[ores_page]`.
///
/// This is intentionally static data: `ores-stack docs` and generated route
/// manifests can describe a page without executing application code. `database`
/// is either `none` or `read_only`; page rendering may call a sibling
/// `*-orm-core` directly, but the rendering surface must not request its
/// read-write capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageMetadata {
    pub title: Option<&'static str>,
    pub summary: Option<&'static str>,
    pub auth: &'static str,
    pub stability: &'static str,
    pub database: &'static str,
    pub features: &'static [&'static str],
    pub data_sources: &'static [&'static str],
    pub tags: &'static [&'static str],
}

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
pub type PageFuture = Pin<Box<dyn Future<Output = PageResult> + Send + 'static>>;
pub type GenerateStaticParamsFuture =
    Pin<Box<dyn Future<Output = PrerenderResult> + Send + 'static>>;

pub type PageFn = fn(PageContext) -> PageFuture;
pub type GenerateStaticParamsFn = fn(PrerenderContext) -> GenerateStaticParamsFuture;
pub type PageAssetsFn = fn() -> PageAssets;
pub type PrerenderFn = GenerateStaticParamsFn;
pub type PageConfigFn = fn() -> PageConfig;
