//! Extractors for the whole HTTP request

use crate::{error::Error, HttpBody, HttpRequest};
use futures_util::future::{ok, Ready};

use hyper::{
    body::Incoming,
    http::{Extensions, request::Parts},
    Method, 
    Request, 
    Uri
};

use crate::headers::{HeaderMap, HeaderValue};
use crate::http::endpoints::args::{
    FromRawRequest,
    FromRequestParts,
    FromRequest,
    FromPayload,
    Payload,
    Source
};

impl FromRawRequest for Request<Incoming> {
    #[inline]
    async fn from_request(req: Request<Incoming>) -> Result<Self, Error> {
        Ok(req)
    }
}

impl FromRawRequest for Parts {
    #[inline]
    async fn from_request(req: Request<Incoming>) -> Result<Self, Error> {
        let (parts, _) = req.into_parts();
        Ok(parts)
    }
}

impl FromRawRequest for HttpBody {
    #[inline]
    async fn from_request(req: Request<Incoming>) -> Result<Self, Error> {
        let (_, body) = req.into_parts();
        Ok(HttpBody::incoming(body))
    }
}

impl FromRequestParts for Uri {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.uri.clone())
    }
}

impl FromRequestParts for Method {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.method.clone())
    }
}

impl FromRequestParts for Extensions {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.extensions.clone())
    }
}

impl FromRequestParts for HeaderMap<HeaderValue> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.headers.clone())
    }
}

impl FromRequest for HttpRequest {
    #[inline]
    async fn from_request(req: HttpRequest) -> Result<Self, Error> {
        Ok(req)
    }
}

impl FromPayload for HttpRequest {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        if let Payload::Full(req) = payload {
            ok(req)
        } else {
            unreachable!()
        }
    }

    fn source() -> Source {
        Source::Full
    }
}