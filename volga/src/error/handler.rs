//! Error Handler

use super::Error;
use crate::{
    HttpResult,
    http::{FromRequestParts, IntoResponse, MapErrHandler},
    status,
};
use futures_util::future::BoxFuture;
use hyper::Uri;
use hyper::http::request::Parts;

use std::sync::Arc;

/// Trait for types that represent a global error handler.
///
/// Instead of receiving full request [`Parts`] at error time (which would
/// require cloning them on every request), an `ErrorHandler` pre-extracts
/// whatever arguments it needs *before* the parts are consumed and returns
/// them as a type-erased [`ErasedErrorArgs`]. The extracted args are dropped
/// on the happy path and invoked only if the handler returns an error.
pub trait ErrorHandler: Send + Sync {
    /// Extracts handler arguments from the request parts before they are consumed.
    ///
    /// Called on every matched request. The returned [`ErasedErrorArgs`] is
    /// dropped if no error occurs, or invoked with the error if one does.
    fn extract(&self, parts: &Parts) -> Box<dyn ErasedErrorArgs + Send>;

    /// Returns whether this handler requires argument extraction from [`Parts`].
    ///
    /// When `false`, [`extract_error_args`] skips the `Box<dyn ErasedErrorArgs>`
    /// allocation entirely and stores only the request URI. Overridden to `false`
    /// by [`DefaultErrorHandler`] to eliminate a per-request heap allocation on
    /// the happy path.
    #[inline]
    fn needs_parts_extraction(&self) -> bool {
        true
    }
}

/// Type-erased, pre-extracted error handler arguments.
///
/// Produced by [`ErrorHandler::extract`] before request parts are consumed.
/// Stores the handler closure and its already-extracted arguments so that
/// `parts.clone()` is never needed on the hot path.
pub trait ErasedErrorArgs: Send {
    /// Invokes the error handler with the given error.
    fn call(self: Box<Self>, err: Error) -> BoxFuture<'static, HttpResult>;
}

/// Pre-extracted error handler slot.
///
/// Avoids a `Box<dyn ErasedErrorArgs>` allocation on the happy path when the
/// default (no custom `map_err`) handler is in use. Only the request [`Uri`]
/// is stored in that case; the full pre-extraction path is taken only when a
/// user-configured handler is present.
pub(crate) enum ErrorArgsSlot {
    /// Default handler: only the request URI is stored — no box allocation.
    Uri(Uri),
    /// Custom handler: args were pre-extracted from parts before consumption.
    Custom(Box<dyn ErasedErrorArgs + Send>),
}

impl ErrorArgsSlot {
    /// Invokes the appropriate error handler.
    #[inline]
    pub(crate) async fn call(self, err: Error) -> HttpResult {
        match self {
            Self::Uri(uri) => {
                let mut err = err;
                if err.instance.is_none() {
                    err.instance = Some(uri.to_string());
                }
                default_error_handler(err).await
            }
            Self::Custom(args) => args.call(err).await,
        }
    }
}

/// The built-in error handler used when no custom [`crate::App::map_err`] handler is configured.
///
/// Sets [`needs_parts_extraction`](ErrorHandler::needs_parts_extraction) to `false`,
/// which allows [`extract_error_args`] to skip the per-request
/// `Box<dyn ErasedErrorArgs>` allocation on the happy path.
#[derive(Debug)]
pub(crate) struct DefaultErrorHandler;

impl ErrorHandler for DefaultErrorHandler {
    #[inline]
    fn extract(&self, parts: &Parts) -> Box<dyn ErasedErrorArgs + Send> {
        Box::new(DefaultErrorArgs {
            uri: parts.uri.clone(),
        })
    }

    #[inline]
    fn needs_parts_extraction(&self) -> bool {
        false
    }
}

/// Owns a closure that handles an error
#[derive(Debug)]
pub struct ErrorFunc<F, R, Args>
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequestParts + Send,
{
    func: F,
    _marker: std::marker::PhantomData<fn(Args) -> R>,
}

impl<F, R, Args> ErrorFunc<F, R, Args>
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequestParts + Send,
{
    pub(crate) fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

/// Stores a pre-extracted handler invocation: the function, its arguments,
/// and the request URI (for `err.instance`). Allocated once per request
/// instead of cloning the full `Parts`.
struct BoundErrorArgs<F, Args> {
    func: F,
    args: Args,
    uri: Uri,
}

impl<F, Args> ErasedErrorArgs for BoundErrorArgs<F, Args>
where
    F: MapErrHandler<Args> + Send + 'static,
    F::Output: IntoResponse + 'static,
    Args: Send + 'static,
{
    fn call(self: Box<Self>, mut err: Error) -> BoxFuture<'static, HttpResult> {
        Box::pin(async move {
            if err.instance.is_none() {
                err.instance = Some(self.uri.to_string());
            }
            match self.func.call(err, self.args).await.into_response() {
                Ok(resp) => Ok(resp),
                Err(err) => default_error_handler(err).await,
            }
        })
    }
}

/// Fallback used when `Args` extraction fails; delegates to the default handler.
struct DefaultErrorArgs {
    uri: Uri,
}

impl ErasedErrorArgs for DefaultErrorArgs {
    fn call(self: Box<Self>, mut err: Error) -> BoxFuture<'static, HttpResult> {
        Box::pin(async move {
            if err.instance.is_none() {
                err.instance = Some(self.uri.to_string());
            }
            default_error_handler(err).await
        })
    }
}

impl<F, R, Args> ErrorHandler for ErrorFunc<F, R, Args>
where
    F: MapErrHandler<Args, Output = R> + Clone + 'static,
    R: IntoResponse + 'static,
    Args: FromRequestParts + Send + 'static,
{
    #[inline]
    fn extract(&self, parts: &Parts) -> Box<dyn ErasedErrorArgs + Send> {
        let uri = parts.uri.clone();
        match Args::from_parts(parts) {
            Ok(args) => Box::new(BoundErrorArgs {
                func: self.func.clone(),
                args,
                uri,
            }),
            Err(_) => Box::new(DefaultErrorArgs { uri }),
        }
    }
}

impl<F, R, Args> From<ErrorFunc<F, R, Args>> for PipelineErrorHandler
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequestParts + Send + 'static,
{
    #[inline]
    fn from(func: ErrorFunc<F, R, Args>) -> Self {
        Arc::new(func)
    }
}

/// Holds a strong reference to the global error handler.
pub(crate) type PipelineErrorHandler = Arc<dyn ErrorHandler + Send + Sync>;

/// Default error handler that creates a [`HttpResult`] from error
#[inline]
pub(crate) async fn default_error_handler(err: Error) -> HttpResult {
    status!(err.status.as_u16(), "{err}")
}

/// Extracts error handler arguments from request parts before they are consumed.
///
/// Returns [`ErrorArgsSlot::Uri`] when the pipeline uses [`DefaultErrorHandler`]
/// (no custom `map_err` configured), avoiding a `Box` allocation on the happy path.
/// Returns [`ErrorArgsSlot::Custom`] for user-configured handlers, pre-extracting
/// their `Args` from parts while they are still available.
#[inline]
pub(crate) fn extract_error_args(handler: &PipelineErrorHandler, parts: &Parts) -> ErrorArgsSlot {
    if handler.needs_parts_extraction() {
        ErrorArgsSlot::Custom(handler.extract(parts))
    } else {
        ErrorArgsSlot::Uri(parts.uri.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DefaultErrorHandler, Error, ErrorArgsSlot, ErrorFunc, PipelineErrorHandler,
        default_error_handler, extract_error_args,
    };
    use crate::{error::ErrorHandler, status};
    use http_body_util::BodyExt;
    use hyper::Request;
    use std::sync::Arc;

    #[tokio::test]
    async fn default_error_handler_returns_server_error_status_code() {
        let error = Error::server_error("Some error");
        let response = default_error_handler(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 500);
        assert_eq!(String::from_utf8_lossy(body), "Some error");
    }

    #[tokio::test]
    async fn default_error_handler_returns_client_error_status_code() {
        let error = Error::client_error("Some error");
        let response = default_error_handler(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 400);
        assert_eq!(String::from_utf8_lossy(body), "Some error");
    }

    #[tokio::test]
    async fn it_create_new_error_handler() {
        let fallback = |_: Error| async { status!(403) };
        let handler = ErrorFunc::new(fallback);

        let error = Error::server_error("Some error");

        let req = Request::get("/foo/bar?baz").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let extracted = handler.extract(&parts);
        let response = extracted.call(error).await;

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 403);
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_calls_error_handler_via_extract_error_args() {
        let fallback = |_: Error| async { status!(403) };
        let handler = PipelineErrorHandler::from(ErrorFunc::new(fallback));

        let error = Error::server_error("Some error");

        let req = Request::get("/foo/bar?baz").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let slot = extract_error_args(&handler, &parts);
        assert!(matches!(slot, ErrorArgsSlot::Custom(_)));

        let response = slot.call(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 403);
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_returns_uri_slot_for_default_handler() {
        let handler: PipelineErrorHandler = Arc::new(DefaultErrorHandler);

        let req = Request::get("/foo/bar").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let slot = extract_error_args(&handler, &parts);

        assert!(matches!(slot, ErrorArgsSlot::Uri(_)));
    }

    #[tokio::test]
    async fn it_calls_default_handler_via_uri_slot() {
        let handler: PipelineErrorHandler = Arc::new(DefaultErrorHandler);

        let error = Error::server_error("Some error");

        let req = Request::get("/foo/bar").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let slot = extract_error_args(&handler, &parts);
        let response = slot.call(error).await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 500);
    }
}
