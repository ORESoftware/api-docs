use crate::{PageContext, PageState};
use std::{collections::BTreeMap, fmt, future::Future, pin::Pin};

/// HTTP method admitted by the filesystem-page surface.
///
/// The page runtime intentionally supports only the safe read methods that the
/// generated page router supports. API mutation semantics remain in `src/routes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRequestMethod {
    Get,
    Head,
}

/// Provider-neutral, bounded HTTP request information supplied to product-owned
/// page admission middleware.
///
/// Provider adapters are responsible for size/shape validation before building
/// this value. Header and cookie values are intentionally omitted from `Debug`
/// because they can contain bearer tokens, session cookies, CSRF material, or
/// other credentials.
#[derive(Clone, PartialEq, Eq)]
pub struct PageRequestContext {
    pub method: PageRequestMethod,
    pub raw_path: String,
    pub raw_query: Option<String>,
    pub headers: BTreeMap<String, Vec<String>>,
    pub cookies: Vec<String>,
}

impl fmt::Debug for PageRequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageRequestContext")
            .field("method", &self.method)
            .field("raw_path", &self.raw_path)
            .field("raw_query_present", &self.raw_query.is_some())
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("cookie_count", &self.cookies.len())
            .finish()
    }
}

/// Input to the product-owned page admission function.
///
/// `auth` is the authored `#[ores_page(auth = "...")]` requirement. The ORES
/// runtime transports it but does not interpret product session/role semantics.
/// `state` is the exact type-erased application state used by normal page
/// execution, allowing the product hook to recover its concrete `AppState` and
/// call the same session/auth services as the standalone server.
#[derive(Clone)]
pub struct PageAdmissionInput {
    pub auth: String,
    pub route_params: BTreeMap<String, String>,
    pub request: PageRequestContext,
    pub state: PageState,
}

impl fmt::Debug for PageAdmissionInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageAdmissionInput")
            .field("auth", &self.auth)
            .field("route_params", &self.route_params)
            .field("request", &self.request)
            .field("state", &self.state)
            .finish()
    }
}

impl PageAdmissionInput {
    /// Admit a public page without product-specific authentication.
    ///
    /// This helper deliberately rejects any non-public requirement so a missing
    /// product admission hook cannot silently downgrade a session/admin page.
    pub fn admit_public(self) -> Result<PageContext, PageAdmissionRejection> {
        if self.auth != "public" {
            return Err(PageAdmissionRejection::text(
                500,
                "page authentication middleware is not configured",
            ));
        }
        let mut context = PageContext::new(self.route_params, self.request.raw_path);
        context.state = self.state;
        Ok(context)
    }
}

/// Provider-neutral HTTP rejection returned before page execution.
///
/// Repeated headers are represented as repeated `(name, value)` entries so
/// `set-cookie` is never comma-folded by this ABI.
#[derive(Clone, PartialEq, Eq)]
pub struct PageAdmissionRejection {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl fmt::Debug for PageAdmissionRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageAdmissionRejection")
            .field("status", &self.status)
            .field(
                "header_names",
                &self
                    .headers
                    .iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
            )
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

impl PageAdmissionRejection {
    #[must_use]
    pub fn text(status: u16, message: &'static str) -> Self {
        Self {
            status,
            headers: vec![
                (
                    "content-type".to_owned(),
                    "text/plain; charset=utf-8".to_owned(),
                ),
                ("cache-control".to_owned(), "no-store".to_owned()),
            ],
            body: message.as_bytes().to_vec(),
        }
    }
}

pub type PageAdmissionResult = Result<PageContext, PageAdmissionRejection>;
pub type PageAdmissionFuture = Pin<Box<dyn Future<Output = PageAdmissionResult> + Send + 'static>>;

/// Stable product admission ABI used by both standalone page routing and cloud
/// provider hosts. Provider runtimes must never substitute their own session or
/// application-role interpretation for this function.
pub type PageAdmissionFn = fn(PageAdmissionInput) -> PageAdmissionFuture;

/// Built-in admission function for an explicitly public page.
#[must_use]
pub fn admit_public_page(input: PageAdmissionInput) -> PageAdmissionFuture {
    Box::pin(async move { input.admit_public() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(auth: &str) -> PageAdmissionInput {
        PageAdmissionInput {
            auth: auth.to_owned(),
            route_params: BTreeMap::new(),
            request: PageRequestContext {
                method: PageRequestMethod::Get,
                raw_path: "/readiness".to_owned(),
                raw_query: None,
                headers: BTreeMap::from([
                    (
                        "authorization".to_owned(),
                        vec!["Bearer sentinel-secret".to_owned()],
                    ),
                    (
                        "cookie".to_owned(),
                        vec!["session=sentinel-cookie".to_owned()],
                    ),
                ]),
                cookies: vec!["session=sentinel-cookie".to_owned()],
            },
            state: PageState::default(),
        }
    }

    #[test]
    fn debug_redacts_header_cookie_and_body_values() {
        let debug = format!("{:?}", input("session"));
        assert!(debug.contains("authorization"));
        assert!(debug.contains("cookie"));
        assert!(!debug.contains("sentinel-secret"));
        assert!(!debug.contains("sentinel-cookie"));

        let rejection = PageAdmissionRejection {
            status: 401,
            headers: vec![("set-cookie".to_owned(), "secret=1".to_owned())],
            body: b"body-secret".to_vec(),
        };
        let debug = format!("{rejection:?}");
        assert!(debug.contains("set-cookie"));
        assert!(!debug.contains("secret=1"));
        assert!(!debug.contains("body-secret"));
    }

    #[test]
    fn public_default_fails_closed_for_non_public_requirement() {
        assert!(input("public").admit_public().is_ok());
        let rejection = input("session").admit_public().unwrap_err();
        assert_eq!(rejection.status, 500);
    }
}
