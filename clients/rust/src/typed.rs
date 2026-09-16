use ores_api_docs::RpcMethod;
use std::future::Future;

/// Network adapter used by generated/browser-facing clients.
///
/// `api-docs` owns operation identity and associated request/output types, but
/// intentionally does not own sockets. Web servers provide an HTTP/TCP/etc.
/// adapter appropriate to their trust realm and receive fully typed calls.
pub trait TypedApiTransport {
    type Error;

    fn call<M>(
        &self,
        params: M::Params,
    ) -> impl Future<Output = Result<M::Output, Self::Error>> + Send
    where
        M: RpcMethod,
        M::Params: Send,
        M::Output: Send;
}

/// Small typed facade suitable for `src/pages/**/page.rs` generated bindings.
/// It contains no server router and cannot publish RPC methods.
#[derive(Debug, Clone)]
pub struct TypedApiClient<T> {
    transport: T,
}

impl<T> TypedApiClient<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn into_transport(self) -> T {
        self.transport
    }
}

impl<T: TypedApiTransport> TypedApiClient<T> {
    pub async fn call<M>(&self, params: M::Params) -> Result<M::Output, T::Error>
    where
        M: RpcMethod,
        M::Params: Send,
        M::Output: Send,
    {
        self.transport.call::<M>(params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct GetHealth;
    impl RpcMethod for GetHealth {
        const KEY: &'static str = "get_health";
        const PATH: &'static str = "/healthz";
        const METHODS: &'static [&'static str] = &["GET"];
        type Params = ();
        type Output = &'static str;
    }

    #[derive(Clone)]
    struct InProcess;
    impl TypedApiTransport for InProcess {
        type Error = std::convert::Infallible;

        async fn call<M>(&self, _params: M::Params) -> Result<M::Output, Self::Error>
        where
            M: RpcMethod,
            M::Params: Send,
            M::Output: Send,
        {
            // Compile-only generic transport fixture. Real transports deserialize
            // into M::Output after validating the generated contract.
            unreachable!("compile-only transport fixture")
        }
    }

    #[test]
    fn client_is_transport_only_and_has_no_router() {
        let client = TypedApiClient::new(InProcess);
        let _ = client.transport();
        let _ = GetHealth::KEY;
    }
}
