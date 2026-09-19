//! Fallback handler

use futures_util::future::BoxFuture;
use std::{marker::PhantomData, sync::Arc};

use crate::{
    HttpRequest, HttpResult,
    error::Error,
    http::{FromRequestParts, GenericHandler, IntoResponse, endpoints::handlers::Handler, marker},
    status,
};

#[cfg(feature = "middleware")]
use crate::middleware::{HttpContext, NextFn};

/// Trait for types that represents a fallback handler
pub trait FallbackHandler {
    /// Calls the fallback handler function for the given request
    fn call(&self, req: HttpRequest) -> BoxFuture<'_, HttpResult>;
}

/// Owns a closure that handles a 404
#[derive(Debug)]
pub struct FallbackFunc<F, Args, M = marker::Async>(pub(crate) F, PhantomData<fn(Args, M)>);

impl<F, Args, R, M> FallbackFunc<F, Args, M>
where
    F: GenericHandler<Args, M, Output = R>,
    Args: FromRequestParts + Send + 'static,
    R: IntoResponse,
{
    pub(crate) fn new(func: F) -> Self {
        Self(func, PhantomData)
    }

    /// Reads the fallback's arguments out of `req`
    #[inline]
    fn args(req: HttpRequest) -> Result<Args, Error> {
        // Nothing matched, so there is no route to read a body for; the
        // parts carry everything a fallback can act on, and unlike the
        // payload trait behind `FromRequest` this one is public, so an
        // extractor defined outside the crate works here.
        let (parts, _) = req.into_parts();
        Args::from_parts(&parts)
    }
}

impl<F, Args, R, M> FallbackHandler for FallbackFunc<F, Args, M>
where
    F: GenericHandler<Args, M, Output = R>,
    Args: FromRequestParts + Send + 'static,
    R: IntoResponse,
{
    #[inline]
    fn call(&self, req: HttpRequest) -> BoxFuture<'_, HttpResult> {
        Box::pin(async move {
            let args = Self::args(req)?;
            self.0.call(args).await.into_response()
        })
    }
}

/// A route group's fallback answers at the end of a route pipeline - the group's middleware
/// in front of it - so it is reached the way a route handler is, while reading the request
/// the way the application fallback does.
impl<F, Args, R, M> Handler for FallbackFunc<F, Args, M>
where
    F: GenericHandler<Args, M, Output = R>,
    Args: FromRequestParts + Send + 'static,
    R: IntoResponse + 'static,
    M: 'static,
{
    #[inline]
    #[cfg(not(feature = "middleware"))]
    fn call(&self, req: HttpRequest) -> BoxFuture<'_, HttpResult> {
        FallbackHandler::call(self, req)
    }

    #[cfg(feature = "middleware")]
    fn into_next(self: Arc<Self>) -> NextFn {
        Arc::new(move |ctx: HttpContext| -> BoxFuture<'static, HttpResult> {
            let (req, _, _) = ctx.into_parts();
            let this = Arc::clone(&self);
            Box::pin(async move {
                let args = Self::args(req.freeze())?;
                this.0.call(args).await.into_response()
            })
        })
    }
}

impl<F, Args, R, M> From<FallbackFunc<F, Args, M>> for PipelineFallbackHandler
where
    F: GenericHandler<Args, M, Output = R>,
    Args: FromRequestParts + Send + 'static,
    R: IntoResponse,
    M: 'static,
{
    #[inline]
    fn from(func: FallbackFunc<F, Args, M>) -> Self {
        Arc::new(func)
    }
}

/// Holds a reference to global error handler
pub(crate) type PipelineFallbackHandler = Arc<dyn FallbackHandler + Send + Sync>;

/// Default fallback handler that creates a 404 [`HttpResult`]
#[inline]
pub(crate) async fn default_fallback_handler() -> HttpResult {
    status!(404)
}

#[cfg(test)]
mod tests {
    use super::{FallbackFunc, default_fallback_handler};
    use crate::status;

    #[tokio::test]
    async fn default_fallback_handler_returns_404() {
        let response = default_fallback_handler().await;
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_create_new_fallback() {
        let fallback = || async { status!(404) };
        let handler = FallbackFunc::new(fallback);

        let response = handler.0().await;
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.status(), 404);
    }
}
