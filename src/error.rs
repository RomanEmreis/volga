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
    sync::{Arc, Weak}
};
use std::io::ErrorKind;
#[cfg(feature = "problem-details")]
pub use self::problem::Problem;

#[cfg(feature = "problem-details")]
pub mod problem;

/// Holds a reference to global error handler
pub(super) type PipelineErrorHandler = Arc<dyn ErrorHandler + Send + Sync>;

/// Weak version of [`PipelineErrorHandler`]
pub(super) type WeakErrorHandler = Weak<dyn ErrorHandler + Send + Sync>;

/// Default error handler that creates a [`HttpResult`] from [`std::io::Error`]
#[inline]
pub(crate) async fn default_error_handler(err: Error) -> HttpResult {
    if err.kind() == InvalidInput {
        status!(400, err.to_string())
    } else {
        status!(500, err.to_string())
    } 
}

#[inline]
pub(crate) async fn call_weak_err_handler(error_handler: WeakErrorHandler, err: Error) -> HttpResult {
    error_handler
        .upgrade()
        .ok_or(Error::new(ErrorKind::Other, "Server Error: error handler could not be upgraded"))?
        .call(err)
        .await
}

/// Trait for types that represents an error handler
pub trait ErrorHandler {
    fn call(&self, err: Error) -> BoxFuture<HttpResult>;
}

/// Owns a closure that handles an error
pub struct ErrorFunc<F>(pub(super) F);

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

impl App {
    /// Adds a global error handler
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, status};
    /// use std::io::Error;
    /// 
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    ///  let mut app = App::new();
    ///  
    ///  app.map_err(|error: Error| async move {
    ///     status!(500, { "error_message:": error.to_string() })
    ///  });
    /// # app.run().await
    /// # }
    /// ```
    pub fn map_err<F, R, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Error) -> Fut + Send + Sync + 'static,
        R: IntoResponse,
        Fut: Future<Output = R> + Send
    {
        self.pipeline
            .set_error_handler(ErrorFunc(handler).into());
        self
    }
}