//! Utilities cor middleware functions

use std::{future::Future, sync::Arc};
use crate::{
    http::{
        endpoints::handlers::RouteHandler, 
        FromRequest, 
        GenericHandler, 
        IntoResponse,
        FilterResult
    },
    error::Error,
    HttpRequest, 
    HttpResponse,
    HttpResult,
};
use super::{MiddlewareFn, Next, HttpContext};

/// Wraps a [`RouteHandler`] into [`MiddlewareFn`]
pub(crate) fn from_handler(handler: RouteHandler) -> MiddlewareFn {
    Arc::new(move |ctx: HttpContext, _| {
        let handler = handler.clone();
        Box::pin(async move { handler.call(ctx.request).await })
    })
}

/// Wraps a closure into [`MiddlewareFn`]
#[inline]
pub(super) fn make_fn<F, Fut>(middleware: F) -> MiddlewareFn
where
    F: Fn(HttpContext, Next) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HttpResult> + Send + 'static,
{
    Arc::new(move |ctx: HttpContext, next: Next| {
        Box::pin(middleware(ctx, next))
    })
}

/// Wraps a closure for the route filter into [`MiddlewareFn`]
#[inline]
pub(super) fn make_filter_fn<F, R, Args>(filter: F) -> MiddlewareFn
where
    F: GenericHandler<Args, Output = R>,
    R: Into<FilterResult> + 'static,
    Args: FromRequest + Send + Sync + 'static
{
    let middleware_fn = move |ctx: HttpContext, next: Next| {
        let filter = filter.clone();
        async move {
            let (req, pipeline) = ctx.into_parts();
            let (parts, body) = req.into_parts();

            let args = Args::from_request(HttpRequest::slim(&parts)).await.unwrap();
            let result = filter
                .call(args)
                .await
                .into();

            let req = HttpRequest::from_parts(parts, body);
            let ctx = HttpContext::new(req, pipeline);
            match result.into_inner() {
                Ok(_) => next(ctx).await,
                Err(err) => err.into_response()
            }
        }
    };
    make_fn(middleware_fn)
}

/// Wraps a closure for the response mapping into [`MiddlewareFn`]
#[inline]
pub(super) fn make_map_ok_fn<F, R, Fut>(map: F) -> MiddlewareFn
where
    F: Fn(HttpResponse) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = R> + Send,
    R: IntoResponse + 'static,
{
    let middleware_fn = move |ctx: HttpContext, next: Next| {
        let map = map.clone();
        async move {
            match next(ctx).await {
                Ok(resp) => map(resp).await.into_response(),
                Err(err) => err.into_response()
            }
        }
    };
    make_fn(middleware_fn)
}

/// Wraps a closure for the error mapping into [`MiddlewareFn`]
#[inline]
pub(super) fn make_map_err_fn<F, R, Fut>(map: F) -> MiddlewareFn
where
    F: Fn(Error) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = R> + Send,
    R: IntoResponse + 'static,
{
    let middleware_fn = move |ctx: HttpContext, next: Next| {
        let map = map.clone();
        async move {
            match next(ctx).await {
                Ok(resp) => Ok(resp),
                Err(err) => map(err).await.into_response()
            }
        }
    };
    make_fn(middleware_fn)
}

/// Wraps a closure for the request mapping into [`MiddlewareFn`]
#[inline]
pub(super) fn make_map_request_fn<F, Fut>(map: F) -> MiddlewareFn
where
    F: Fn(HttpRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = HttpRequest> + Send,
{
    let middleware_fn = move |ctx: HttpContext, next: Next| {
        let map = map.clone();
        async move {
            let (req, pipeline) = ctx.into_parts();
            let req = map(req).await;
            let ctx = HttpContext::new(req, pipeline);
            next(ctx).await
        }
    };
    make_fn(middleware_fn)
}