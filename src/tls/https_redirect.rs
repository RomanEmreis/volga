//! HTTP to HTTPS redirection middleware

use crate::{HttpResponse, permanent_redirect, status};
use futures_util::future::BoxFuture;
use std::io::{Error, ErrorKind::Other};

use hyper::{
    header::HOST, 
    http::uri::Scheme, 
    body::Incoming, 
    Request, 
    service::Service, 
    Uri
};

/// Represents a middleware that redirects all HTTP requests to HTTPS
pub(super) struct HttpsRedirectionMiddleware {
    https_port: u16
}

impl HttpsRedirectionMiddleware {
    pub(super) fn new(https_port: u16) -> Self {
        Self { https_port }
    }
}

impl Service<Request<Incoming>> for HttpsRedirectionMiddleware {
    type Response = HttpResponse;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    #[inline]
    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let https_port = self.https_port;
        Box::pin(async move {
            let (parts, _) = request.into_parts();
            let mut uri_parts = parts.uri.into_parts();
            
            uri_parts.scheme = Some(Scheme::HTTPS);
            if uri_parts.path_and_query.is_none() {
                uri_parts.path_and_query = Some("/".parse().unwrap());
            }
            
            if let Some(host) = parts.headers
                .get(&HOST)
                .and_then(|host| host.to_str().ok()) {

                let (host, _) = host
                    .rsplit_once(':')
                    .unwrap_or((host, ""));

                uri_parts.authority = Some(format!("{host}:{https_port}")
                    .parse()
                    .map_err(HttpsRedirectionError::invalid_uri)?
                );
                
                let uri = Uri::from_parts(uri_parts)
                    .map_err(HttpsRedirectionError::invalid_uri_parts)?;
                
                permanent_redirect!(uri.to_string())
            } else {
                status!(404)
            }
        })
    }
}

struct HttpsRedirectionError;

impl HttpsRedirectionError {
    #[inline]
    fn invalid_uri(error: hyper::http::uri::InvalidUri) -> Error {
        Error::new(Other, error)
    }
    
    #[inline]
    fn invalid_uri_parts(error: hyper::http::uri::InvalidUriParts) -> Error {
        Error::new(Other, error)
    }
}