//! Error Handling tools

use futures_util::future::BoxFuture;
use super::{
    App, 
    http::IntoResponse, 
    HttpResult,
    status
};

use std::{
    future::Future,
    io::{Error, ErrorKind::InvalidInput},
    sync::Arc
};

pub(super) type PipelineErrorHandler = Arc<dyn ErrorHandler>;

#[inline]
pub(crate) async fn default_error_handler(err: Error) -> HttpResult {
    if err.kind() == InvalidInput {
        status!(400, err.to_string())
    } else {
        status!(500, err.to_string())
    } 
}

pub trait ErrorHandler: Send + Sync + 'static {
    fn call(&self, err: Error) -> BoxFuture<HttpResult>;
}

pub struct ErrorFunc<F>(pub(super) F);

impl<F, R, Fut> ErrorHandler for ErrorFunc<F>
where
    F: Fn(Error) -> Fut + Send + Sync + 'static,
    R: IntoResponse + 'static,
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
    R: IntoResponse + 'static,
    Fut: Future<Output = R> + Send
{
    #[inline]
    fn from(func: ErrorFunc<F>) -> Self {
        Arc::new(func)
    }
}

impl App {
    /// Adds a global error handler
    pub fn map_err<F, R, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Error) -> Fut + Send + Sync + 'static,
        R: IntoResponse + 'static,
        Fut: Future<Output = R> + Send,
    {
        self.pipeline
            .set_error_handler(ErrorFunc(handler).into());
        self
    }
}