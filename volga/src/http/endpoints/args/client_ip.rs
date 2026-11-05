//! Extractors for client IP address

use crate::error::Error;
use crate::http::{FromRequestHead, RequestHead, endpoints::args::sealed::PathAccess};
use std::fmt::Display;
use std::{net::SocketAddr, ops::Deref};

/// Wraps the client's [`SocketAddr`]
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, ClientIp, ok};
///
/// async fn handle(ip: ClientIp) -> HttpResult {
///     ok!("Client IP: {ip}")
/// }
/// ```
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ClientIp(pub(crate) SocketAddr);

impl Display for ClientIp {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for ClientIp {
    type Target = SocketAddr;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ClientIp {
    /// Unwraps the inner [`SocketAddr`]
    #[inline]
    pub fn into_inner(self) -> SocketAddr {
        self.0
    }
}

/// Extracts `ClientIp` from the head of a request
impl FromRequestHead for ClientIp {
    #[inline]
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        head.scope()
            .map(|s| s.client_ip)
            .ok_or_else(|| Error::server_error("Client IP: missing"))
    }

    // Reads no path parameter, so a positional extractor next to it takes its own
    const PATH_ACCESS: PathAccess = PathAccess::Ignored;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::endpoints::args::{FromPayload, FromRequestRef, Payload};
    use crate::http::request_scope::HttpRequestScope;
    use crate::{HttpBody, HttpRequest};
    use hyper::{Request, http::request::Parts};

    fn make_scope(ip: ClientIp) -> HttpRequestScope {
        HttpRequestScope {
            client_ip: ip,
            ..HttpRequestScope::default()
        }
    }

    fn parts() -> Parts {
        Request::get("/").body(()).unwrap().into_parts().0
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let ip = ClientIp(SocketAddr::from(([0, 0, 0, 0], 8080)));
        let (parts, scope) = (parts(), make_scope(ip));

        let client_ip =
            ClientIp::from_payload(Payload::Parts(RequestHead::with_scope(&parts, &scope)))
                .await
                .unwrap();

        assert_eq!(client_ip, ip);
    }

    #[test]
    fn it_gets_from_the_scope() {
        let ip = ClientIp(SocketAddr::from(([0, 0, 0, 0], 8080)));
        let (parts, scope) = (parts(), make_scope(ip));

        let client_ip = ClientIp::from_request(&RequestHead::with_scope(&parts, &scope)).unwrap();

        assert_eq!(client_ip, ip);
    }

    #[test]
    fn it_gets_err_without_a_scope() {
        let parts = parts();

        let client_ip = ClientIp::from_request(&RequestHead::from_parts(&parts));

        assert!(client_ip.is_err());
    }

    #[test]
    fn it_gets_from_request_ref() {
        let ip = ClientIp(SocketAddr::from(([0, 0, 0, 0], 8080)));
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        let req = HttpRequest::from_parts(parts, body).with_scope(make_scope(ip));

        let client_ip = req.extract::<ClientIp>().unwrap();

        assert_eq!(client_ip, ip);
    }
}
