//! Fallback handler

use futures_util::future::BoxFuture;
use std::{marker::PhantomData, sync::Arc};

use crate::{
    HttpRequest, HttpResult,
    http::{FromRequestParts, GenericHandler, IntoResponse},
    status,
};

/// Trait for types that represents a fallback handler
pub trait FallbackHandler {
    /// Calls the fallback handler function for the given request
    fn call(&self, req: HttpRequest) -> BoxFuture<'_, HttpResult>;
}

/// Owns a closure that handles a 404
#[derive(Debug)]
pub struct FallbackFunc<F, Args>(pub(crate) F, PhantomData<fn(Args)>);

impl<F, Args, R> FallbackFunc<F, Args>
where
    F: GenericHandler<Args, Output = R>,
    Args: FromRequestParts + Send + 'static,
    R: IntoResponse,
{
    pub(crate) fn new(func: F) -> Self {
        Self(func, PhantomData)
    }
}

impl<F, Args, R> FallbackHandler for FallbackFunc<F, Args>
where
    F: GenericHandler<Args, Output = R>,
    Args: FromRequestParts + Send + 'static,
    R: IntoResponse,
{
    #[inline]
    fn call(&self, req: HttpRequest) -> BoxFuture<'_, HttpResult> {
        Box::pin(async move {
            // Nothing matched, so there is no route to read a body for; the
            // parts carry everything a fallback can act on, and unlike the
            // payload trait behind `FromRequest` this one is public, so an
            // extractor defined outside the crate works here.
            let (parts, _) = req.into_parts();
            let args = Args::from_parts(&parts)?;
            self.0.call(args).await.into_response()
        })
    }
}

impl<F, Args, R> From<FallbackFunc<F, Args>> for PipelineFallbackHandler
where
    F: GenericHandler<Args, Output = R>,
    Args: FromRequestParts + Send + 'static,
    R: IntoResponse,
{
    #[inline]
    fn from(func: FallbackFunc<F, Args>) -> Self {
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
