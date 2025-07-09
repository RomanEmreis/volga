//! Utilities for middleware functions

use std::{future::Future, sync::Arc};
use crate::{
    http::{
        endpoints::handlers::RouteHandler, 
        FromRequest,
        FromRequestRef,
        GenericHandler,
        MapErrHandler,
        IntoResponse,
        FilterResult
    },
    HttpRequest, 
    HttpResult,
};
use super::{
    handler::{
        MiddlewareHandler,
        MapOkHandler,
        TapReqHandler,
        Next
    },
    MiddlewareFn, 
    HttpContext,
    NextFn, 
};


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
    F: Fn(HttpContext, NextFn) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HttpResult> + Send + 'static,
{
    Arc::new(move |ctx: HttpContext, next: NextFn| {
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
    let middleware_fn = move |ctx: HttpContext, next: NextFn| {
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
pub(super) fn make_map_ok_fn<F, R, Args>(map: F) -> MiddlewareFn
where
    F: MapOkHandler<Args, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequestRef + Send + Sync + 'static,
{
    let middleware_fn = move |ctx: HttpContext, next: NextFn| {
        let map = map.clone();
        async move {
            match Args::from_request(&ctx.request) {
                Err(err) => err.into_response(),
                Ok(args) => {
                    match next(ctx).await {
                        Ok(resp) => map.call(resp, args).await.into_response(),
                        Err(err) => err.into_response()
                    }       
                }
            }
        }
    };
    make_fn(middleware_fn)
}

/// Wraps a closure for the error mapping into [`MiddlewareFn`]
#[inline]
pub(super) fn make_map_err_fn<F, R, Args>(map: F) -> MiddlewareFn
where
    F: MapErrHandler<Args, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequestRef + Send + Sync + 'static,
{
    let middleware_fn = move |ctx: HttpContext, next: NextFn| {
        let map = map.clone();
        async move {
            match Args::from_request(&ctx.request) { 
                Err(err) => err.into_response(),
                Ok(args) => match next(ctx).await {
                    Err(err) => map.call(err, args).await.into_response(),
                    Ok(resp) => Ok(resp),
                }
            }
        }
    };
    make_fn(middleware_fn)
}

/// Wraps a closure for the request mapping into [`MiddlewareFn`]
#[inline]
pub(super) fn make_tap_req_fn<F, Args>(map: F) -> MiddlewareFn
where
    F: TapReqHandler<Args, Output = HttpRequest>,
    Args: FromRequestRef + Send + Sync + 'static,
{
    let middleware_fn = move |ctx: HttpContext, next: NextFn| {
        let map = map.clone();
        async move {
            let (req, pipeline) = ctx.into_parts();
            match Args::from_request(&req) {
                Err(err) => err.into_response(),
                Ok(args) => {
                    let req = map.call(req, args).await;
                    let ctx = HttpContext::new(req, pipeline);
                    next(ctx).await
                },
            }
        }
    };
    make_fn(middleware_fn)
}

/// Wraps a closure for the `with()` middleware into [`MiddlewareFn`]
#[inline]
pub(super) fn make_with_fn<F, R, Args>(middleware: F) -> MiddlewareFn
where
    F: MiddlewareHandler<Args, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequestRef + Send + Sync + 'static,
{
    let middleware_fn = move |ctx: HttpContext, next: NextFn| {
        let middleware = middleware.clone();
        async move {
            match Args::from_request(&ctx.request) { 
                Err(err) => err.into_response(),
                Ok(args) => {
                    let next = Next::new(ctx, next);
                    middleware.call(args, next).await.into_response()
                }
            }
        }
    };
    make_fn(middleware_fn)
}

#[cfg(test)]
mod tests {
    use hyper::Request;
    use super::*;
    use crate::{bad_request, ok, HttpResponse, HttpBody};
    use crate::http::endpoints::handlers::Func;
    use crate::http::StatusCode;
    use crate::error::Error;

    fn create_request() -> HttpRequest {
        let req = Request::get("http://localhost")
            .body(HttpBody::empty())
            .unwrap();
        let (parts, body) = req.into_parts();
        HttpRequest::from_parts(parts, body)
    }
    
    #[tokio::test]
    async fn it_tests_from_handler() {
        let handler = || async { ok!() };
        let route_handler = Func::new(handler);
        let middleware = from_handler(route_handler);
        
        let req = create_request();
        let ctx = HttpContext::slim(req);
        let next: NextFn = Arc::new(|_| Box::pin(async { ok!() }));

        let result = middleware(ctx, next).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_tests_make_fn() {
        let middleware_logic = |ctx: HttpContext, next: NextFn| async move {
            // Simple pass-through middleware
            next(ctx).await
        };

        let middleware = make_fn(middleware_logic);

        let req = create_request();
        let ctx = HttpContext::slim(req);
        let next: NextFn = Arc::new(|_| Box::pin(async { ok!() }));

        let result = middleware(ctx, next).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_tests_make_filter_fn() {
        let filter = || async { true };
        let middleware = make_filter_fn(filter);

        let req = create_request();
        let ctx = HttpContext::slim(req);
        let next: NextFn = Arc::new(|_| Box::pin(async { ok!() }));

        let result = middleware(ctx, next).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_tests_make_map_ok_fn() {
        // Create a response mapper that adds a header
        let map = |mut resp: HttpResponse| async move {
            resp.headers_mut().insert("X-Test", "value".parse().unwrap());
            resp
        };

        let middleware = make_map_ok_fn(map);

        let req = create_request();
        let ctx = HttpContext::slim(req);
        let next: NextFn = Arc::new(|_| Box::pin(async { ok!() }));

        let result = middleware(ctx, next).await;
        assert!(result.is_ok());
        if let Ok(response) = result {
            assert_eq!(response.headers().get("X-Test").unwrap(), "value");
        }
    }

    #[tokio::test]
    async fn it_tests_make_map_err_fn() {
        // Create an error mapper that converts errors to 400 Bad Request
        let map = |_err: Error| async {
            bad_request!()
        };

        let middleware = make_map_err_fn(map);

        let req = create_request();
        let ctx = HttpContext::slim(req);
        // Create a next function that returns an error
        let next: NextFn = Arc::new(|_| Box::pin(async {
            Err(Error::client_error("test error"))
        }));

        let result = middleware(ctx, next).await;
        assert!(result.is_ok());
        if let Ok(response) = result {
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn it_test_make_map_request_fn() {
        // Create a request mapper that adds a header
        let map = |mut req: HttpRequest| async move {
            req.headers_mut().insert("X-Test", "value".parse().unwrap());
            req
        };

        let middleware = make_tap_req_fn(map);

        let req = create_request();
        let ctx = HttpContext::slim(req);
        let next: NextFn = Arc::new(|ctx: HttpContext| Box::pin(async move {
            assert_eq!(ctx.request.headers().get("X-Test").unwrap(), "value");
            ok!()
        }));

        let result = middleware(ctx, next).await;
        assert!(result.is_ok());
    }
}