//! Extractors for client IP address

use std::{ops::Deref, net::SocketAddr};
use std::fmt::Display;
use futures_util::future::{ready, Ready};
use hyper::http::{request::Parts, Extensions};
use crate::{
    http::{FromRequestParts, FromRequestRef, endpoints::args::{FromPayload, Payload, Source}},
    error::Error,
    HttpRequest,
};

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

impl TryFrom<&Extensions> for ClientIp {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        extensions.get::<ClientIp>()
            .cloned()
            .ok_or_else(|| Error::server_error("Client IP: missing"))
    }
}

impl TryFrom<&Parts> for ClientIp {
    type Error = Error;
    
    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Self::Error> {
        ClientIp::try_from(&parts.extensions)
    }
}

/// Extracts `ClientIp` from request parts
impl FromRequestParts for ClientIp {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        parts.try_into()
    }
}

/// Extracts `ClientIp` from request
impl FromRequestRef for ClientIp {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions().try_into()
    }
}

/// Extracts `ClientIp` from request payload
impl FromPayload for ClientIp {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(parts.try_into())
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Request, http::Extensions};
    use crate::http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload};
    use crate::{HttpBody, HttpRequest};
    use super::*;

    #[tokio::test]
    async fn it_reads_from_payload() {
        let ip = ClientIp(SocketAddr::from(([0, 0, 0, 0], 8080)));
        let req = Request::get("/")
            .extension(ip)
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let client_ip = ClientIp::from_payload(Payload::Parts(&parts)).await.unwrap();

        assert_eq!(client_ip, ip);
    }

    #[test]
    fn it_gets_from_extensions() {
        let ip = ClientIp(SocketAddr::from(([0, 0, 0, 0], 8080)));
        let mut extensions = Extensions::new();
        extensions.insert(ip);

        let client_ip = ClientIp::try_from(&extensions).unwrap();

        assert_eq!(client_ip, ip);
    }

    #[test]
    fn it_gets_err_from_extensions_if_missing() {
        let extensions = Extensions::new();

        let client_ip = ClientIp::try_from(&extensions);

        assert!(client_ip.is_err());
    }

    #[test]
    fn it_gets_from_request_parts() {
        let ip = ClientIp(SocketAddr::from(([0, 0, 0, 0], 8080)));
        let req = Request::get("/")
            .extension(ip)
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let client_ip = ClientIp::from_parts(&parts).unwrap();

        assert_eq!(client_ip, ip);
    }

    #[test]
    fn it_gets_from_request_ref() {
        let ip = ClientIp(SocketAddr::from(([0, 0, 0, 0], 8080)));
        let req = Request::get("/")
            .extension(ip)
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let client_ip = ClientIp::from_request(&req).unwrap();

        assert_eq!(client_ip, ip);
    }
}