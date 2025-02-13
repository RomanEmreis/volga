//! Extractors for the whole Hosting Environment request

use crate::{app::HostEnv, error::Error};
use futures_util::future::{ready, Ready};
use hyper::http::{request::Parts};
use crate::http::endpoints::args::{
    FromRequestParts,
    FromPayload,
    Payload,
    Source
};

impl FromRequestParts for HostEnv {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        match parts.extensions.get::<HostEnv>() { 
            Some(env) => Ok(env.clone()),
            None => Err(Error::server_error("hosting environment is not specified"))
        }
    }
}

impl FromPayload for HostEnv {
    type Future = Ready<Result<Self, Error>>;
    
    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(HostEnv::from_parts(parts))
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}