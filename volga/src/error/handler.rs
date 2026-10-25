//! Error Handler

use super::Error;
use crate::{
    HttpResponse, HttpResult,
    headers::HeaderMap,
    http::{FromRequestParts, IntoResponse, MapErr, marker},
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
    /// When `false`, the framework skips the `Box<dyn ErasedErrorArgs>`
    /// allocation entirely and stores nothing. Overridden to `false`
    /// by the default error handler to eliminate a per-request heap allocation on
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
/// default (no custom `map_err`) handler is in use. Nothing is stored in that
/// case; the full pre-extraction path is taken only when a user-configured
/// handler is present.
pub(crate) enum ErrorArgsSlot {
    /// Default handler: nothing is stored - no box allocation.
    Default,
    /// Custom handler: args were pre-extracted from parts before consumption.
    Custom(Box<dyn ErasedErrorArgs + Send>),
}

impl ErrorArgsSlot {
    /// Invokes the appropriate error handler.
    #[inline]
    pub(crate) async fn call(self, err: Error) -> HttpResult {
        match self {
            // The default handler answers with the error's message alone, so it is not given
            // an instance it would not read: that is an allocation or two on every error,
            // and a refusal under load - a `429` - is as hot a path as an answer
            Self::Default => default_error_handler(err).await,
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
pub struct ErrorFunc<F, R, Args, M = marker::Async>
where
    F: MapErr<Args, M, Output = R>,
    R: IntoResponse,
    Args: FromRequestParts + Send,
{
    func: F,
    _marker: std::marker::PhantomData<fn(Args, M) -> R>,
}

impl<F, R, Args, M> ErrorFunc<F, R, Args, M>
where
    F: MapErr<Args, M, Output = R>,
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
/// and the request URI (for the error's instance). Allocated once per request
/// instead of cloning the full `Parts`.
struct BoundErrorArgs<F, Args, M> {
    func: F,
    args: Args,
    uri: Uri,
    _marker: std::marker::PhantomData<fn(M)>,
}

impl<F, Args, M> ErasedErrorArgs for BoundErrorArgs<F, Args, M>
where
    F: MapErr<Args, M> + Send + 'static,
    F::Output: IntoResponse + 'static,
    Args: Send + 'static,
    M: 'static,
{
    fn call(self: Box<Self>, mut err: Error) -> BoxFuture<'static, HttpResult> {
        Box::pin(async move {
            err.set_instance_if_none(|| self.uri.to_string());
            let response = answer_with(err, |err| async move {
                self.func.map_err(err, self.args).await.into_response()
            });

            match response.await {
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
            err.set_instance_if_none(|| self.uri.to_string());
            default_error_handler(err).await
        })
    }
}

impl<F, R, Args, M> ErrorHandler for ErrorFunc<F, R, Args, M>
where
    F: MapErr<Args, M, Output = R> + Clone + 'static,
    R: IntoResponse + 'static,
    Args: FromRequestParts + Send + 'static,
    M: 'static,
{
    #[inline]
    fn extract(&self, parts: &Parts) -> Box<dyn ErasedErrorArgs + Send> {
        let uri = parts.uri.clone();
        match Args::from_parts(parts) {
            Ok(args) => Box::new(BoundErrorArgs {
                func: self.func.clone(),
                args,
                uri,
                _marker: std::marker::PhantomData,
            }),
            Err(_) => Box::new(DefaultErrorArgs { uri }),
        }
    }
}

impl<F, R, Args, M> From<ErrorFunc<F, R, Args, M>> for PipelineErrorHandler
where
    F: MapErr<Args, M, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequestParts + Send + 'static,
    M: 'static,
{
    #[inline]
    fn from(func: ErrorFunc<F, R, Args, M>) -> Self {
        Arc::new(func)
    }
}

/// Holds a strong reference to the global error handler.
pub(crate) type PipelineErrorHandler = Arc<dyn ErrorHandler + Send + Sync>;

/// Default error handler that creates a [`HttpResult`] from error
///
/// An error carrying a response of its own - see [`Error::with_response`] - answers with it,
/// and the headers the error carries - see [`Error::with_header`] - are added to the response
/// either way.
#[inline]
pub(crate) async fn default_error_handler(mut err: Error) -> HttpResult {
    let headers = err.take_headers();
    let response = match err.take_response() {
        Some(response) => Ok(response),
        None => status!(err.status.as_u16(), "{err}"),
    };

    match headers {
        Some(headers) => response.map(|resp| with_error_headers(resp, headers)),
        None => response,
    }
}

/// Answers `err` with `handler`, adding the headers the error carries - see
/// [`Error::with_header`] - to the response the handler answers with
///
/// The handler is given the error with its headers, so it can read them, and they are
/// copied for the response beforehand, since the handler takes the error. A handler
/// answering with an error passes that error on with whatever headers it carries, and they
/// are added where it is answered in the end, so they are added once.
#[inline]
pub(crate) async fn answer_with<H, Fut>(err: Error, handler: H) -> HttpResult
where
    H: FnOnce(Error) -> Fut,
    Fut: Future<Output = HttpResult>,
{
    match err.headers().cloned() {
        None => handler(err).await,
        Some(headers) => handler(err)
            .await
            .map(|resp| with_error_headers(resp, headers)),
    }
}

/// Adds the headers of an error to the response it is answered with, leaving every name the
/// response carries already as the response has it
#[inline]
pub(crate) fn with_error_headers(mut resp: HttpResponse, headers: HeaderMap) -> HttpResponse {
    let target = resp.headers_mut();
    let mut keep = false;

    // A name is yielded once, ahead of every value it carries, and `None` after it means
    // another value for that name
    let mut name = None;
    for (next, value) in headers {
        if let Some(next) = next {
            keep = !target.contains_key(&next);
            name = Some(next);
        }

        if keep && let Some(name) = name.as_ref() {
            target.append(name, value);
        }
    }
    resp
}

/// Extracts error handler arguments from request parts before they are consumed.
///
/// Returns [`ErrorArgsSlot::Default`] when the pipeline uses [`DefaultErrorHandler`]
/// (no custom `map_err` configured), avoiding a `Box` allocation on the happy path.
/// Returns [`ErrorArgsSlot::Custom`] for user-configured handlers, pre-extracting
/// their `Args` from parts while they are still available.
#[inline]
pub(crate) fn extract_error_args(handler: &PipelineErrorHandler, parts: &Parts) -> ErrorArgsSlot {
    if handler.needs_parts_extraction() {
        ErrorArgsSlot::Custom(handler.extract(parts))
    } else {
        ErrorArgsSlot::Default
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DefaultErrorHandler, Error, ErrorArgsSlot, ErrorFunc, PipelineErrorHandler, answer_with,
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
    async fn default_error_handler_answers_with_the_attached_response() {
        let error = Error::client_error("Some error").with_response(status!(200, "custom"));
        let response = default_error_handler(error).await;
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 400);
        assert_eq!(String::from_utf8_lossy(body), "custom");
    }

    #[tokio::test]
    async fn default_error_handler_adds_the_error_headers() {
        let error = Error::client_error("Some error")
            .with_header(("x-reason", "one"))
            .with_header(("x-reason", "two"));
        let response = default_error_handler(error).await.unwrap();

        let values: Vec<_> = response.headers().get_all("x-reason").iter().collect();
        assert_eq!(values, ["one", "two"]);
    }

    #[tokio::test]
    async fn it_adds_the_error_headers_to_the_response_the_handler_answers_with() {
        let error = Error::client_error("Some error")
            .with_header(("x-reason", "error"))
            .with_header(("x-kept", "one"))
            .with_header(("x-kept", "two"));

        let response = answer_with(error, |err| async move {
            // The handler is given the headers to read
            assert!(err.headers().is_some_and(|h| h.contains_key("x-reason")));
            status!(418; [("x-reason", "handler")])
        })
        .await
        .unwrap();

        assert_eq!(response.status(), 418);
        let reason: Vec<_> = response.headers().get_all("x-reason").iter().collect();
        assert_eq!(reason, ["handler"]);
        let kept: Vec<_> = response.headers().get_all("x-kept").iter().collect();
        assert_eq!(kept, ["one", "two"]);
    }

    #[tokio::test]
    async fn it_passes_on_an_error_the_handler_answers_with() {
        let error = Error::client_error("Some error").with_header(("x-reason", "error"));

        let err = answer_with(error, |err| async move { Err(err) })
            .await
            .unwrap_err();

        assert_eq!(err.headers().unwrap()["x-reason"], "error");
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
    async fn it_returns_the_default_slot_for_the_default_handler() {
        let handler: PipelineErrorHandler = Arc::new(DefaultErrorHandler);

        let req = Request::get("/foo/bar").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let slot = extract_error_args(&handler, &parts);

        assert!(matches!(slot, ErrorArgsSlot::Default));
    }

    #[tokio::test]
    async fn it_calls_default_handler_via_the_default_slot() {
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
