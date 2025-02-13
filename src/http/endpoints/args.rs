//! Extractors for HTTP request parts and body

use std::future::Future;
use hyper::{
    body::Incoming,
    http::request::Parts,
    Request
};

use crate::{
    http::endpoints::route::PathArguments,
    HttpBody, 
    HttpRequest, 
    error::Error
};

#[cfg(feature = "di")]
use crate::di::Container;

pub mod path;
pub mod query;
pub mod json;
pub mod file;
pub mod cancellation_token;
pub mod request;
pub mod form;

#[cfg(feature = "multipart")]
pub mod multipart;
#[cfg(feature = "static-files")]
pub mod host_env;

/// Holds the payload for extractors
pub(crate) enum Payload<'a> {
    None,
    Request(HttpRequest),
    Body(HttpBody),
    Full(&'a Parts, HttpBody),
    Parts(&'a Parts),
    Path(&'a (String, String)),
    #[cfg(feature = "di")]
    Dc(&'a Container)
}

/// Describes a data source for extractors to read from
pub(crate) enum Source {
    None,
    Request,
    Full,
    Parts,
    Path,
    Body,
    #[cfg(feature = "di")]
    Dc
}

/// Specifies extractors to read data from HTTP request
pub trait FromRequest: Sized {
    fn from_request(req: HttpRequest) -> impl Future<Output = Result<Self, Error>> + Send;
}

/// Specifies extractors to read data from raw HTTP request
pub trait FromRawRequest: Sized {
    fn from_request(req: Request<Incoming>) -> impl Future<Output = Result<Self, Error>> + Send;
}

/// Specifies extractors to read data from borrowed HTTP request
pub trait FromRequestRef: Sized {
    fn from_request(req: &HttpRequest) -> Result<Self, Error>;
}

/// Specifies extractors to read data from HTTP request parts
pub trait FromRequestParts: Sized {
    fn from_parts(parts: &Parts) -> Result<Self, Error>;
}

/// Specifies extractor to read data from HTTP request
/// depending on payload's [`Source`]
pub(crate) trait FromPayload: Send + Sized {
    type Future: Future<Output = Result<Self, Error>> + Send;
    
    /// Extracts data from give [`Payload`]
    fn from_payload(payload: Payload) -> Self::Future;

    /// Returns a [`Source`] where payload should be extracted from
    fn source() -> Source {
        Source::None
    }
}

impl FromRequest for () {
    #[inline]
    async fn from_request(_: HttpRequest) -> Result<Self, Error> {
        Ok(())
    }
}

impl FromRawRequest for () {
    #[inline]
    async fn from_request(_: Request<Incoming>) -> Result<Self, Error> {
        Ok(())
    }
}

macro_rules! define_generic_from_request {
    ($($T: ident),*) => {
        impl<$($T: FromPayload),+> FromRequest for ($($T,)+) {
            #[inline]
            async fn from_request(req: HttpRequest) -> Result<Self, Error> {
                #[cfg(feature = "di")]
                let (parts, body, container) = req.into_parts();
                #[cfg(not(feature = "di"))]
                let (parts, body) = req.into_parts();
                
                let params = parts.extensions.get::<PathArguments>()
                    .map(|params| &params[..])
                    .unwrap_or(&[]);
                
                let mut body = Some(body);
                let mut iter = params.iter();
                let tuple = (
                    $(
                    $T::from_payload(match $T::source() {
                        Source::None => Payload::None,
                        Source::Parts => Payload::Parts(&parts),
                        #[cfg(feature = "di")]
                        Source::Dc => Payload::Dc(&container),
                        Source::Path => match iter.next() {
                            Some(param) => Payload::Path(&param),
                            None => Payload::None
                        },
                        Source::Body => match body.take() {
                            Some(body) => Payload::Body(body),
                            None => Payload::None
                        },
                        Source::Full => match body.take() {
                            Some(body) => Payload::Full(&parts, body),
                            None => Payload::None
                        },
                        Source::Request => match body.take() {
                            Some(body) => {
                                #[cfg(feature = "di")]
                                let req = Payload::Request(HttpRequest::from_parts(parts.clone(), body, container.clone()));
                                #[cfg(not(feature = "di"))]
                                let req = Payload::Request(HttpRequest::from_parts(parts.clone(), body));
                                req
                            },
                            None => Payload::None
                        },
                    }).await?,
                    )*    
                );
                Ok(tuple)
            }
        }
    }
}

macro_rules! define_generic_from_raw_request {
    ($($T: ident),*) => {
        impl<$($T: FromRequestParts),+> FromRawRequest for ($($T,)+) {
            #[inline]
            async fn from_request(req: Request<Incoming>) -> Result<Self, Error> {
                let (parts, _) = req.into_parts();
                let tuple = (
                    $(
                    $T::from_parts(&parts)?,
                    )*    
                );
                Ok(tuple)
            }
        }
    }
}

define_generic_from_request! { T1 }
define_generic_from_request! { T1, T2 }
define_generic_from_request! { T1, T2, T3 }
define_generic_from_request! { T1, T2, T3, T4 }
define_generic_from_request! { T1, T2, T3, T4, T5 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7, T8 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7, T8, T9 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7, T8, T9, T10 }

define_generic_from_raw_request! { T1 }
define_generic_from_raw_request! { T1, T2 }
define_generic_from_raw_request! { T1, T2, T3 }
define_generic_from_raw_request! { T1, T2, T3, T4 }