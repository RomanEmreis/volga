//! Extractors for middleware functions

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::error::Error;
use crate::{HttpRequestMut, HttpResponse, HttpResult};
use super::{HttpContext, NextFn};

/// Represents the [`Future`] that wraps the next middleware in the chain,
/// that will be called by `await` with the current [`HttpContext`]
/// 
/// # Example
/// ```no_run
/// # use volga::middleware::Next;
/// # use volga::App;
/// # let mut app = App::new();
/// app.with(|next: Next| async move {
///     next.await
/// });
/// ```
pub struct Next {
    inner: Option<Pin<Box<dyn Future<Output = HttpResult> + Send>>>
}

impl std::fmt::Debug for Next {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Next(..)")
    }
}

impl Future for Next {
    type Output = HttpResult;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let fut = this
            .inner
            .as_mut()
            .ok_or_else(|| Error::server_error("Next polled after completion"))?;

        let poll = fut.as_mut().poll(cx);
        if matches!(poll, Poll::Ready(_)) {
            this.inner = None;
        }
        poll
    }
}

impl Next {
    /// Creates a new [`Next`]
    pub fn new(ctx: HttpContext, next: NextFn) -> Self {
        Self { inner: Some(Box::pin(next(ctx))) }
        //Self { ctx: Some(ctx), next }
    }
}

/// Describes a generic middleware handler that could take 0 or N parameters and [`Next`] middleware
pub trait MiddlewareHandler<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;
    /// Middleware handler future
    type Future: Future<Output = Self::Output> + Send;

    /// Calls the middleware handler
    fn call(&self, args: Args, next: Next) -> Self::Future;
}

/// Describes a generic [`tap_req`] middleware handler that could take 0 or N parameters and [`HttpRequestMut`]
pub trait TapReqHandler<Args = ()>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;
    /// Tap handler future
    type Future: Future<Output = Self::Output> + Send;

    /// Calls the [`tap_req`] handler
    fn call(&self, req: HttpRequestMut, args: Args) -> Self::Future;
}

/// Describes a generic [`map_ok`] middleware handler that could take 0 or N parameters and [`HttpResponse`]
pub trait MapOkHandler<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;
    /// MapOk handler future
    type Future: Future<Output = Self::Output> + Send;

    /// Calls the [`map_ok`] handler
    fn call(&self, resp: HttpResponse, args: Args) -> Self::Future;
}

#[cfg(not(feature = "di"))]
impl<Func, Fut: Send> TapReqHandler for Func
where
    Func: Fn(HttpRequestMut) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future,
{
    type Output = Fut::Output;
    type Future = Fut;
    
    #[inline]
    fn call(&self, req: HttpRequestMut, _args: ()) -> Self::Future {
        self(req)
    }
}

macro_rules! define_generic_mw_handler ({ $($param:ident)* } => {
    impl<Func, Fut: Send, $($param,)*> MiddlewareHandler<($($param,)*)> for Func
    where
        Func: Fn($($param,)* Next) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;
        type Future = Fut;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ($($param,)*): ($($param,)*), next: Next) -> Self::Future {
            (self)($($param,)* next)
        }
    }
    #[cfg(feature = "di")]
    impl<Func, Fut: Send, $($param,)*> TapReqHandler<($($param,)*)> for Func
    where
        Func: Fn(HttpRequestMut,$($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;
        type Future = Fut;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, req: HttpRequestMut, ($($param,)*): ($($param,)*)) -> Self::Future {
            (self)(req, $($param,)*)
        }
    }
    impl<Func, Fut: Send, $($param,)*> MapOkHandler<($($param,)*)> for Func
    where
        Func: Fn(HttpResponse,$($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;
        type Future = Fut;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, resp: HttpResponse, ($($param,)*): ($($param,)*)) -> Self::Future {
            (self)(resp, $($param,)*)
        }
    }
});

define_generic_mw_handler! {}
define_generic_mw_handler! { T1 }
define_generic_mw_handler! { T1 T2 }
define_generic_mw_handler! { T1 T2 T3 }
define_generic_mw_handler! { T1 T2 T3 T4 }
define_generic_mw_handler! { T1 T2 T3 T4 T5 }

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use futures_util::task::noop_waker_ref;
    use crate::{HttpBody, HttpResponse, status};
    use crate::error::Error;
    use super::{MapOkHandler, MiddlewareHandler, Next};

    #[test]
    fn next_returns_error_when_polled_after_completion() {
        let mut next = Next {
            inner: Some(Box::pin(async { status!(204) })),
        };

        let waker = noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        let mut pinned = Pin::new(&mut next);

        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(Ok(_)) => {}
            other => panic!("unexpected poll result: {other:?}"),
        }

        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(Err(err)) => {
                assert!(err.to_string().contains("Next polled after completion"));
            }
            other => panic!("expected error after completion, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn middleware_handler_invokes_function_with_next() {
        let next = Next {
            inner: Some(Box::pin(async { status!(204) })),
        };

        let handler = |value: u8, next: Next| async move {
            assert_eq!(value, 7);
            next.await
        };

        let response = MiddlewareHandler::call(&handler, (7,), next).await.unwrap();
        assert_eq!(response.status(), 204);
    }

    #[tokio::test]
    async fn map_ok_handler_invokes_function() {
        let handler = |resp: HttpResponse, extra: &'static str| async move {
            assert_eq!(resp.status(), 200);
            assert_eq!(extra, "ok");
            Ok::<HttpResponse, Error>(resp)
        };

        let response = HttpResponse::builder()
            .status(200)
            .body(HttpBody::from("ok"))
            .unwrap();

        let result = MapOkHandler::call(&handler, response, ("ok",)).await;
        assert!(result.is_ok());
    }
}