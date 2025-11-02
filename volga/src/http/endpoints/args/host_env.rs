//! Extractors for the whole Hosting Environment request

use crate::{app::HostEnv, error::Error, HttpRequest};
use futures_util::future::{ready, Ready};
use hyper::http::{request::Parts, Extensions};
use crate::http::endpoints::args::{
    FromRequestParts,
    FromRequestRef,
    FromPayload,
    Payload,
    Source
};

impl TryFrom<&Extensions> for HostEnv {
    type Error = Error;
    
    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        match extensions.get::<HostEnv>() {
            Some(env) => Ok(env.clone()),
            None => Err(Error::server_error("Server Error: hosting environment is not specified"))
        }
    }
}

impl FromRequestRef for HostEnv {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions().try_into()
    }
}

impl FromRequestParts for HostEnv {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        HostEnv::try_from(&parts.extensions)
    }
}

impl FromPayload for HostEnv {
    type Future = Ready<Result<Self, Error>>;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(HostEnv::from_parts(parts))
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}