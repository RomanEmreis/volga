//! Error Handler

use futures_util::future::BoxFuture;
use hyper::Uri;
use crate::{http::IntoResponse, HttpResult, status};
use super::Error;

use std::{
    future::Future,
    sync::{Arc, Weak}
};

/// Trait for types that represents an error handler
pub trait ErrorHandler {
    fn call(&self, err: Error) -> BoxFuture<HttpResult>;
}

/// Owns a closure that handles an error
pub struct ErrorFunc<F>(pub(crate) F);

impl<F, R, Fut> ErrorHandler for ErrorFunc<F>
where
    F: Fn(Error) -> Fut + Send + Sync,
    R: IntoResponse,
    Fut: Future<Output = R> + Send,
{
    #[inline]
    fn call(&self, err: Error) -> BoxFuture<HttpResult> {
        Box::pin(async move {
            match self.0(err).await.into_response() {
                Ok(resp) => Ok(resp),
                Err(err) => default_error_handler(err).await,
            }
        })
    }
}

impl<F, R, Fut> From<ErrorFunc<F>> for PipelineErrorHandler
where
    F: Fn(Error) -> Fut + Send + Sync + 'static,
    R: IntoResponse,
    Fut: Future<Output = R> + Send
{
    #[inline]
    fn from(func: ErrorFunc<F>) -> Self {
        Arc::new(func)
    }
}

/// Holds a reference to global error handler
pub(crate) type PipelineErrorHandler = Arc<
    dyn ErrorHandler
    + Send 
    + Sync
>;

/// Weak version of [`crate::error::PipelineErrorHandler`]
pub(crate) type WeakErrorHandler = Weak<
    dyn ErrorHandler
    + Send 
    + Sync
>;

/// Default error handler that creates a [`HttpResult`] from error
#[inline]
pub(crate) async fn default_error_handler(err: Error) -> HttpResult {
    status!(err.status.as_u16(), "{:?}", err)
}

#[inline]
pub(crate) async fn call_weak_err_handler(error_handler: WeakErrorHandler, uri: &Uri, mut err: Error) -> HttpResult {
    if err.instance.is_none() {
        err.instance = Some(uri.to_string());
    }
    error_handler
        .upgrade()
        .ok_or(Error::server_error("Server Error: error handler could not be upgraded"))?
        .call(err)
        .await
}