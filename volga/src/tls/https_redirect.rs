//! HTTP to HTTPS redirection middleware

use crate::{HttpBody, HttpResult, error::Error, status, tls::request_authority};
use std::future::{Ready, ready};

use hyper::{
    Request, Response, Uri,
    http::{
        request::Parts,
        uri::{Authority, PathAndQuery, Scheme},
    },
    service::Service,
};

#[cfg(debug_assertions)]
use crate::temp_redirect;

#[cfg(not(debug_assertions))]
use crate::permanent_redirect;

/// Represents a middleware that redirects all HTTP requests to HTTPS
pub(super) struct HttpsRedirectionMiddleware {
    https_port: u16,
}

impl HttpsRedirectionMiddleware {
    pub(super) fn new(https_port: u16) -> Self {
        Self { https_port }
    }
}

impl<B> Service<Request<B>> for HttpsRedirectionMiddleware {
    type Response = Response<HttpBody>;
    type Error = Error;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    #[inline]
    fn call(&self, request: Request<B>) -> Self::Future {
        let (parts, _) = request.into_parts();
        ready(redirect(parts, self.https_port).map(Into::into))
    }
}

/// Answers a plain-HTTP request with a redirect to the same target over HTTPS
fn redirect(parts: Parts, https_port: u16) -> HttpResult {
    // With no host to send the client to there is nothing to redirect to, and RFC 9112
    // Section 3.2 answers such a request `400`: one without `Host`, with more than one, or with
    // one that is not a valid authority
    let Some(authority) =
        request_authority(&parts.uri, &parts.headers).and_then(|a| Authority::try_from(a).ok())
    else {
        return status!(400);
    };

    let mut uri_parts = parts.uri.into_parts();

    uri_parts.scheme = Some(Scheme::HTTPS);
    uri_parts.authority = Some(
        format!("{}:{https_port}", authority.host())
            .parse()
            .map_err(HttpsRedirectionError::invalid_uri)?,
    );
    if uri_parts.path_and_query.is_none() {
        uri_parts.path_and_query = Some(PathAndQuery::from_static("/"));
    }

    let uri = Uri::from_parts(uri_parts).map_err(HttpsRedirectionError::invalid_uri_parts)?;

    // Link caching can cause unstable behavior in development environments.
    // So use temporary redirects rather than permanent redirects for debug mode
    #[cfg(debug_assertions)]
    let response = temp_redirect!(uri.to_string());
    #[cfg(not(debug_assertions))]
    let response = permanent_redirect!(uri.to_string());

    response
}

struct HttpsRedirectionError;

impl HttpsRedirectionError {
    #[inline]
    fn invalid_uri(error: hyper::http::uri::InvalidUri) -> Error {
        Error::server_error(error)
    }

    #[inline]
    fn invalid_uri_parts(error: hyper::http::uri::InvalidUriParts) -> Error {
        Error::server_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::redirect;
    use hyper::{Request, StatusCode, header::HOST, header::LOCATION, http::request::Parts};

    fn parts(uri: &str, hosts: &[&str]) -> Parts {
        let mut request = Request::builder().uri(uri);
        for host in hosts {
            request = request.header(HOST, *host);
        }
        request.body(()).unwrap().into_parts().0
    }

    fn location(parts: Parts) -> String {
        let response = redirect(parts, 8443).unwrap();

        assert!(response.status().is_redirection());
        response.headers()[LOCATION].to_str().unwrap().to_owned()
    }

    #[test]
    fn it_redirects_by_the_host_header() {
        let location = location(parts("/path?a=b", &["example.com:8080"]));

        assert_eq!(location, "https://example.com:8443/path?a=b");
    }

    #[test]
    fn it_redirects_by_the_authority_of_the_request_target() {
        // An HTTP/2 request: the host is only in the URI
        let location = location(parts("http://example.com:8080/path", &[]));

        assert_eq!(location, "https://example.com:8443/path");
    }

    #[test]
    fn it_redirects_to_the_root_when_the_target_has_no_path() {
        let location = location(parts("http://example.com", &[]));

        assert_eq!(location, "https://example.com:8443/");
    }

    #[test]
    fn it_keeps_an_ipv6_host_whole() {
        assert_eq!(
            location(parts("/path", &["[::1]:8080"])),
            "https://[::1]:8443/path"
        );
        assert_eq!(
            location(parts("/path", &["[::1]"])),
            "https://[::1]:8443/path"
        );
    }

    #[test]
    fn it_answers_400_without_a_host() {
        let response = redirect(parts("/path", &[]), 8443).unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_answers_400_to_more_than_one_host() {
        let response = redirect(parts("/path", &["example.com", "example.net"]), 8443).unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_answers_400_to_a_host_that_is_not_an_authority() {
        let response = redirect(parts("/path", &["exa mple.com"]), 8443).unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
