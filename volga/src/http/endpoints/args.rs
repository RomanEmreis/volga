//! Extractors for HTTP request parts and body

use std::future::Future;
use hyper::{
    body::Incoming,
    http::request::Parts,
    Request
};

use crate::{
    error::Error,
    http::endpoints::route::{PathArg, PathArgs},
    HttpBody,
    HttpRequest
};

pub mod path;
pub mod query;
pub mod json;
pub mod file;
pub mod cancellation_token;
pub mod request;
pub mod form;
pub mod sse;
pub mod option;
pub mod result;
pub mod vec;

#[cfg(feature = "multipart")]
pub mod multipart;
#[cfg(feature = "static-files")]
pub mod host_env;

/// Holds the payload for extractors
pub(crate) enum Payload<'a> {
    None,
    Request(Box<HttpRequest>),
    Full(&'a Parts, HttpBody),
    Parts(&'a Parts),
    Path(PathArg),
    Body(HttpBody),
    PathArgs(PathArgs)
}

/// Describes a data source for extractors to read from
#[derive(Debug, PartialEq)]
pub(crate) enum Source {
    None,
    Request,
    Full,
    Parts,
    Path,
    Body,
    PathArgs
}

/// Specifies extractors to read data from HTTP request
pub trait FromRequest: Sized {
    fn from_request(req: HttpRequest) -> impl Future<Output = Result<Self, Error>> + Send;
}

/// Specifies extractors to read data from raw HTTP request
pub trait FromRawRequest: Sized {
    fn from_request(req: Request<Incoming>) -> impl Future<Output = Result<Self, Error>> + Send;
}

/// Specifies extractors to read data from a borrowed HTTP request
pub trait FromRequestRef: Sized {
    fn from_request(req: &HttpRequest) -> Result<Self, Error>;
}

/// Specifies extractors to read data from HTTP request parts
pub trait FromRequestParts: Sized {
    fn from_parts(parts: &Parts) -> Result<Self, Error>;
}

/// Specifies extractor to read data from an HTTP request 
/// depending on payload's [`Source`]
pub(crate) trait FromPayload: Send + Sized {
    type Future: Future<Output = Result<Self, Error>> + Send;
    
    /// Extracts data from give [`Payload`]
    fn from_payload(payload: Payload) -> Self::Future;

    /// Returns a [`Source`] where the payload should be extracted from
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

impl FromRequestRef for () {
    #[inline]
    fn from_request(_: &HttpRequest) -> Result<Self, Error> {
        Ok(())
    }
}

impl FromRequestParts for () {
    #[inline]
    fn from_parts(_: &Parts) -> Result<Self, Error> {
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
        impl<$($T: FromRequestRef),+> FromRequestRef for ($($T,)+) {
            #[inline]
            fn from_request(req: &HttpRequest) -> Result<Self, Error> {
                let tuple = (
                    $(
                    $T::from_request(req)?,
                    )*    
                );
                Ok(tuple)
            }
        }
        impl<$($T: FromRequestParts),+> FromRequestParts for ($($T,)+) {
            #[inline]
            fn from_parts(parts: &Parts) -> Result<Self, Error> {
                let tuple = (
                    $(
                    $T::from_parts(parts)?,
                    )*    
                );
                Ok(tuple)
            }
        }
        impl<$($T: FromPayload),+> FromRequest for ($($T,)+) {
            #[inline]
            async fn from_request(req: HttpRequest) -> Result<Self, Error> {
                let (mut parts, body) = req.into_parts();
                
                let params = parts.extensions
                    .remove::<PathArgs>()
                    .unwrap_or_default();

                let mut body = Some(body);
                let mut iter = params.into_iter();
                let tuple = (
                    $(
                    $T::from_payload(match $T::source() {
                        Source::None => Payload::None,
                        Source::Parts => Payload::Parts(&parts),
                        Source::PathArgs => Payload::PathArgs(std::mem::replace(&mut iter, default_args())
                            .collect::<PathArgs>()),
                        Source::Path => match iter.next() {
                            Some(param) => Payload::Path(param),
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
                            Some(body) => Payload::Request(Box::new(HttpRequest::from_parts(parts.clone(), body))),
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

fn default_args() -> smallvec::IntoIter<[PathArg; 4]> {
    PathArgs::new().into_iter()
}

#[cfg(test)]
mod tests {
    use futures_util::future::{ok, Ready};
    use crate::error::Error;
    use super::{FromPayload, Payload, Source};
    
    struct TestNone;
    
    impl FromPayload for TestNone {
        type Future = Ready<Result<TestNone, Error>>;

        fn from_payload(_: Payload) -> Self::Future {
            ok(TestNone)
        }
    }
    
    #[tokio::test]
    async fn it_reads_none_from_payload() {
        let extraction = TestNone::from_payload(Payload::None).await;
        assert!(extraction.is_ok());
        assert_eq!(TestNone::source(), Source::None);
    }
}