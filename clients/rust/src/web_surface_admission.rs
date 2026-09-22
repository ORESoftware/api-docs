use crate::{PageAdmissionRejection, PageRequestContext, PageState};
use std::{fmt, future::Future, pin::Pin};

/// Framework-owned browser-web surface whose bytes are served outside the
/// normal filesystem-page renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSpecialSurfaceKind {
    Static,
    Docs,
}

/// Trusted build-plan access class for a classified special surface.
///
/// This value must come from admitted manifest/build metadata, never from a
/// request header, cookie, query parameter, or provider-specific hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSpecialSurfaceAccess {
    /// Bytes may be served after the explicit built-in public admission path.
    Public,
    /// Product-owned admission must authorize the request before bytes are read.
    Private,
    /// Admin/session admission must authorize the request before docs are read.
    Admin,
}

/// Provider-neutral input to product-owned admission for static/docs surfaces.
///
/// `normalized_path` is already percent-decoded and normalized exactly once by
/// the trusted ingress before surface classification. Runtime/provider glue may
/// transport this value but must not reinterpret product auth/session policy.
#[derive(Clone)]
pub struct WebSpecialSurfaceAdmissionInput {
    pub surface: WebSpecialSurfaceKind,
    pub access: WebSpecialSurfaceAccess,
    pub normalized_path: String,
    pub request: PageRequestContext,
    pub state: PageState,
}

impl fmt::Debug for WebSpecialSurfaceAdmissionInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebSpecialSurfaceAdmissionInput")
            .field("surface", &self.surface)
            .field("access", &self.access)
            .field("normalized_path", &self.normalized_path)
            .field("request", &self.request)
            .field("state", &self.state)
            .finish()
    }
}

/// Evidence returned only after the selected special surface has passed its
/// required admission boundary. Content lookup/streaming may begin after this
/// value exists; a miss remains terminal inside the already-selected surface.
#[derive(Clone)]
pub struct AdmittedWebSpecialSurface {
    pub surface: WebSpecialSurfaceKind,
    pub access: WebSpecialSurfaceAccess,
    pub normalized_path: String,
    pub state: PageState,
}

impl fmt::Debug for AdmittedWebSpecialSurface {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdmittedWebSpecialSurface")
            .field("surface", &self.surface)
            .field("access", &self.access)
            .field("normalized_path", &self.normalized_path)
            .field("state", &self.state)
            .finish()
    }
}

impl WebSpecialSurfaceAdmissionInput {
    /// Admit only an explicitly public special surface.
    ///
    /// Private static and admin docs fail closed when a product admission hook
    /// is missing; provider/runtime glue must never silently downgrade them.
    pub fn admit_public(self) -> Result<AdmittedWebSpecialSurface, PageAdmissionRejection> {
        if self.access != WebSpecialSurfaceAccess::Public {
            return Err(PageAdmissionRejection::text(
                500,
                "special-surface authentication middleware is not configured",
            ));
        }
        Ok(AdmittedWebSpecialSurface {
            surface: self.surface,
            access: self.access,
            normalized_path: self.normalized_path,
            state: self.state,
        })
    }
}

pub type WebSpecialSurfaceAdmissionResult =
    Result<AdmittedWebSpecialSurface, PageAdmissionRejection>;
pub type WebSpecialSurfaceAdmissionFuture =
    Pin<Box<dyn Future<Output = WebSpecialSurfaceAdmissionResult> + Send + 'static>>;

/// Stable product admission ABI shared by standalone, PPR and provider hosts.
///
/// Product code may inspect its normal application/session state and the
/// normalized request evidence. Provider glue must not substitute its own auth
/// interpretation for this function.
pub type WebSpecialSurfaceAdmissionFn =
    fn(WebSpecialSurfaceAdmissionInput) -> WebSpecialSurfaceAdmissionFuture;

#[must_use]
pub fn admit_public_web_special_surface(
    input: WebSpecialSurfaceAdmissionInput,
) -> WebSpecialSurfaceAdmissionFuture {
    Box::pin(async move { input.admit_public() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageRequestMethod;
    use std::collections::BTreeMap;

    fn input(
        surface: WebSpecialSurfaceKind,
        access: WebSpecialSurfaceAccess,
    ) -> WebSpecialSurfaceAdmissionInput {
        WebSpecialSurfaceAdmissionInput {
            surface,
            access,
            normalized_path: match surface {
                WebSpecialSurfaceKind::Static => "/static/private/report.pdf",
                WebSpecialSurfaceKind::Docs => "/_/docs/admin/reference",
            }
            .to_owned(),
            request: PageRequestContext {
                method: PageRequestMethod::Get,
                raw_path: "/raw-request-evidence".to_owned(),
                raw_query: None,
                headers: BTreeMap::from([(
                    "authorization".to_owned(),
                    vec!["Bearer sentinel-secret".to_owned()],
                )]),
                cookies: vec!["session=sentinel-cookie".to_owned()],
            },
            state: PageState::default(),
        }
    }

    #[test]
    fn public_admission_is_explicit() {
        let admitted = input(
            WebSpecialSurfaceKind::Static,
            WebSpecialSurfaceAccess::Public,
        )
        .admit_public()
        .expect("public static admission");
        assert_eq!(admitted.surface, WebSpecialSurfaceKind::Static);
        assert_eq!(admitted.access, WebSpecialSurfaceAccess::Public);
    }

    #[test]
    fn missing_product_hook_cannot_downgrade_private_or_admin() {
        for (surface, access) in [
            (
                WebSpecialSurfaceKind::Static,
                WebSpecialSurfaceAccess::Private,
            ),
            (WebSpecialSurfaceKind::Docs, WebSpecialSurfaceAccess::Admin),
        ] {
            let rejection = input(surface, access)
                .admit_public()
                .expect_err("non-public special surface must fail closed");
            assert_eq!(rejection.status, 500);
        }
    }

    #[test]
    fn debug_redacts_request_header_and_cookie_values() {
        let debug = format!(
            "{:?}",
            input(
                WebSpecialSurfaceKind::Static,
                WebSpecialSurfaceAccess::Private,
            )
        );
        assert!(debug.contains("authorization"));
        assert!(!debug.contains("sentinel-secret"));
        assert!(!debug.contains("sentinel-cookie"));
    }

    #[test]
    fn admitted_path_is_trusted_normalized_path_not_raw_request_path() {
        let admitted = input(
            WebSpecialSurfaceKind::Docs,
            WebSpecialSurfaceAccess::Public,
        )
        .admit_public()
        .unwrap();
        assert_eq!(admitted.normalized_path, "/_/docs/admin/reference");
        assert_ne!(admitted.normalized_path, "/raw-request-evidence");
    }
}
