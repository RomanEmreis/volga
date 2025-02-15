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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use http_body_util::BodyExt;
    use crate::{http::Uri, status};
    use super::{
        Error,
        ErrorFunc,
        PipelineErrorHandler,
        WeakErrorHandler,
        default_error_handler, 
        call_weak_err_handler
    };

    #[tokio::test]
    async fn default_error_handler_returns_server_error_status_code() {
        let error = Error::server_error("Some error");
        let response = default_error_handler(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), 500);
        assert_eq!(String::from_utf8_lossy(body), "\"Error { status: 500, instance: None, inner: \\\"Some error\\\" }\"");
    }

    #[tokio::test]
    async fn default_error_handler_returns_client_error_status_code() {
        let error = Error::client_error("Some error");
        let response = default_error_handler(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 400);
        assert_eq!(String::from_utf8_lossy(body), "\"Error { status: 400, instance: None, inner: \\\"Some error\\\" }\"");
    }

    #[tokio::test]
    async fn it_create_new_error_handler() {
        let fallback = |_: Error| async { status!(403) };
        let handler = ErrorFunc(fallback);

        let error = Error::server_error("Some error");
        let response = handler.0(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 403);
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_call_weak_error_handler() {
        let fallback = |_: Error| async { status!(403) };
        let handler = ErrorFunc(fallback);
        let handler = PipelineErrorHandler::from(handler);
        let weak_handler: WeakErrorHandler = Arc::downgrade(&handler);

        let error = Error::server_error("Some error");
        let uri = "/foo/bar?baz".parse::<Uri>().unwrap();
        let response = call_weak_err_handler(weak_handler, &uri, error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 403);
        assert_eq!(body.len(), 0);
    }
}