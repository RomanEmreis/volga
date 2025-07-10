//! Error Handler

use futures_util::future::BoxFuture;
use hyper::http::request::Parts;
use crate::{http::{IntoResponse, MapErrHandler, FromRequestParts}, HttpResult, status};
use super::Error;

use std::sync::{Arc, Weak};

/// Trait for types that represents an error handler
pub trait ErrorHandler {
    fn call(&self, parts: &Parts, err: Error) -> BoxFuture<HttpResult>;
}

/// Owns a closure that handles an error
pub struct ErrorFunc<F, R, Args>
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequestParts + Send + Sync
{
    func: F,
    _marker: std::marker::PhantomData<Args>,
}

impl<F, R, Args> ErrorFunc<F, R, Args>
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequestParts + Send + Sync
{
    pub(crate) fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<F, R, Args> ErrorHandler for ErrorFunc<F, R, Args>
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequestParts + Send + Sync + 'static,
{
    #[inline]
    fn call(&self, parts: &Parts, err: Error) -> BoxFuture<HttpResult> {
        let Ok(args) = Args::from_parts(parts) else { 
            return Box::pin(async move { Err(err) });
        };
        Box::pin(async move {
            match self.func.call(err, args).await.into_response() {
                Ok(resp) => Ok(resp),
                Err(err) => default_error_handler(err).await,
            }
            //match Args::from_parts(&parts) { 
            //    Err(err) => err.into_response(),
            //    Ok(args) => match self.func.call(err, args).await.into_response() {
            //        Ok(resp) => Ok(resp),
            //        Err(err) => default_error_handler(err).await,
            //    }
            //}
        })
    }
}

impl<F, R, Args> From<ErrorFunc<F, R, Args>> for PipelineErrorHandler
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequestParts + Send + Sync + 'static,
{
    #[inline]
    fn from(func: ErrorFunc<F, R, Args>) -> Self {
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
    status!(err.status.as_u16(), "{}", err.to_string())
}

#[inline]
pub(crate) async fn call_weak_err_handler(error_handler: WeakErrorHandler, parts: &Parts, mut err: Error) -> HttpResult {
    if err.instance.is_none() {
        err.instance = Some(parts.uri.to_string());
    }
    error_handler
        .upgrade()
        .ok_or(Error::server_error("Server Error: error handler could not be upgraded"))?
        .call(parts, err)
        .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use http_body_util::BodyExt;
    use hyper::Request;
    use crate::{error::ErrorHandler, status};
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
        assert_eq!(String::from_utf8_lossy(body), "\"Some error\"");
    }

    #[tokio::test]
    async fn default_error_handler_returns_client_error_status_code() {
        let error = Error::client_error("Some error");
        let response = default_error_handler(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 400);
        assert_eq!(String::from_utf8_lossy(body), "\"Some error\"");
    }

    #[tokio::test]
    async fn it_create_new_error_handler() {
        let fallback = |_: Error| async { status!(403) };
        let handler = ErrorFunc::new(fallback);

        let error = Error::server_error("Some error");

        let req = Request::get("/foo/bar?baz").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let response = handler.call(&parts, error).await;
        
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 403);
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_call_weak_error_handler() {
        let fallback = |_: Error| async { status!(403) };
        let handler = ErrorFunc::new(fallback);
        let handler = PipelineErrorHandler::from(handler);
        let weak_handler: WeakErrorHandler = Arc::downgrade(&handler);

        let error = Error::server_error("Some error");

        let req = Request::get("/foo/bar?baz").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let response = call_weak_err_handler(weak_handler, &parts, error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 403);
        assert_eq!(body.len(), 0);
    }
}