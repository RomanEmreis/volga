//! Extractors for the whole HTTP request

use crate::{error::Error, HttpRequest};
use futures_util::future::{ok, Ready};

use hyper::{
    http::{Extensions, request::Parts},
    Method, 
    Uri
};

use crate::headers::{HeaderMap, HeaderValue};
use crate::http::endpoints::args::{
    FromRequestParts,
    FromPayload,
    Payload,
    Source
};

impl FromRequestParts for Parts {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.clone())
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