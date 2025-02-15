//! Fallback handler

use std::{marker::PhantomData, sync::Arc};
use futures_util::future::BoxFuture;
use hyper::{Request, body::Incoming};

use crate::{
    error::handler::default_error_handler,
    http::{GenericHandler, FromRawRequest, IntoResponse},
    HttpResult,
    status
};

/// Trait for types that represents a fallback handler
pub trait FallbackHandler {
    fn call(&self, req: Request<Incoming>) -> BoxFuture<HttpResult>;
}

/// Owns a closure that handles a 404
pub struct FallbackFunc<F, Args>(pub(crate) F, PhantomData<Args>);

impl<F, Args, R> FallbackFunc<F, Args>
where
    F: GenericHandler<Args, Output = R>,
    Args: FromRawRequest + Send + Sync + 'static,
    R: IntoResponse
{
    pub(crate) fn new(func: F) -> Self {
        Self(func, PhantomData)
    }
}

impl<F, Args, R> FallbackHandler for FallbackFunc<F, Args>
where
    F: GenericHandler<Args, Output = R>,
    Args: FromRawRequest + Send + Sync + 'static,
    R: IntoResponse
{
    #[inline]
    fn call(&self, req: Request<Incoming>) -> BoxFuture<HttpResult> {
        Box::pin(async move {
            let args = match Args::from_request(req).await {
                Ok(args) => args,
                Err(err) => return default_error_handler(err).await,
            };
            match self.0.call(args).await.into_response() {
                Ok(resp) => Ok(resp),
                Err(err) => default_error_handler(err).await,
            }
        })
    }
}

impl<F, Args, R> From<FallbackFunc<F, Args>> for PipelineFallbackHandler
where
    F: GenericHandler<Args, Output = R>,
    Args: FromRawRequest + Send + Sync + 'static,
    R: IntoResponse
{
    #[inline]
    fn from(func: FallbackFunc<F, Args>) -> Self {
        Arc::new(func)
    }
}

/// Holds a reference to global error handler
pub(crate) type PipelineFallbackHandler = Arc<
    dyn FallbackHandler
    + Send
    + Sync
>;

/// Default fallback handler that creates a 404 [`HttpResult`]
#[inline]
pub(crate) async fn default_fallback_handler() -> HttpResult {
    status!(404)
}

#[cfg(test)]
mod tests {
    use super::{default_fallback_handler, FallbackFunc};
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