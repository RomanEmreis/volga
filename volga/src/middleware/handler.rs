//! Extractors for middleware functions

use futures_util::future::BoxFuture;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{HttpContext, NextFn};
use crate::error::Error;
use crate::{HttpRequestMut, HttpResponse, HttpResult};

/// Internal state machine for [`Next`]
///
/// `Pending` is intentionally large: `HttpContext` lives here until the first
/// poll, avoiding the heap allocation that would be required to box it.
/// Both variants reside inside the already heap-allocated [`Next`] future,
/// so this does not create stack pressure.
#[allow(clippy::large_enum_variant)]
enum NextState {
    /// Not yet polled; the inner future is created on demand
    Pending(HttpContext, NextFn),
    /// Polled at least once; holds the running future
    Running(BoxFuture<'static, HttpResult>),
}

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
    state: Option<NextState>,
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
        match this.state.take() {
            None => Poll::Ready(Err(Error::server_error("Next polled after completion"))),
            Some(NextState::Pending(ctx, next)) => {
                let mut fut = next(ctx);
                let poll = fut.as_mut().poll(cx);
                if poll.is_pending() {
                    this.state = Some(NextState::Running(fut));
                }
                poll
            }
            Some(NextState::Running(mut fut)) => {
                let poll = fut.as_mut().poll(cx);
                if poll.is_pending() {
                    this.state = Some(NextState::Running(fut));
                }
                poll
            }
        }
    }
}

impl Next {
    /// Creates a new [`Next`].
    ///
    /// The inner future is created lazily: `next(ctx)` is not called until
    /// this future is first polled. This avoids a heap allocation when the
    /// middleware exits early without awaiting `next`.
    pub fn new(ctx: HttpContext, next: NextFn) -> Self {
        Self {
            state: Some(NextState::Pending(ctx, next)),
        }
    }
}

/// Describes a generic middleware handler that could take [`HttpContext`] parameters and [`NextFn`] middleware
pub trait WrapHandler: Send + Sync + 'static {
    /// Calls the middleware handler
    fn call(
        &self,
        ctx: HttpContext,
        next: NextFn,
    ) -> impl Future<Output = HttpResult> + Send + 'static;
}

/// Describes a generic middleware handler that could take 0 or N parameters and [`Next`] middleware
pub trait MiddlewareHandler<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls the middleware handler
    fn call(&self, args: Args, next: Next) -> impl Future<Output = Self::Output> + Send;
}

/// Describes a generic [`tap_req`] middleware handler that could take 0 or N parameters and [`HttpRequestMut`]
pub trait TapReqHandler<Args = ()>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls the [`tap_req`] handler
    fn call(&self, req: HttpRequestMut, args: Args) -> impl Future<Output = Self::Output> + Send;
}

/// Describes a generic [`map_ok`] middleware handler that could take 0 or N parameters and [`HttpResponse`]
pub trait MapOkHandler<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls the [`map_ok`] handler
    fn call(&self, resp: HttpResponse, args: Args) -> impl Future<Output = Self::Output> + Send;
}

impl<Func, Fut: Send> WrapHandler for Func
where
    Func: Fn(HttpContext, NextFn) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = HttpResult> + Send + 'static,
{
    #[inline]
    fn call(
        &self,
        ctx: HttpContext,
        next: NextFn,
    ) -> impl Future<Output = HttpResult> + Send + 'static {
        self(ctx, next)
    }
}

#[cfg(not(feature = "di"))]
impl<Func, Fut: Send> TapReqHandler for Func
where
    Func: Fn(HttpRequestMut) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future,
{
    type Output = Fut::Output;

    #[inline]
    fn call(&self, req: HttpRequestMut, _args: ()) -> impl Future<Output = Self::Output> + Send {
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

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ($($param,)*): ($($param,)*), next: Next) -> impl Future<Output = Self::Output> {
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

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, req: HttpRequestMut, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            (self)(req, $($param,)*)
        }
    }
    impl<Func, Fut: Send, $($param,)*> MapOkHandler<($($param,)*)> for Func
    where
        Func: Fn(HttpResponse,$($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, resp: HttpResponse, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
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
    use super::{MapOkHandler, MiddlewareHandler, Next, NextState};
    use crate::error::Error;
    use crate::{HttpBody, HttpResponse, status};
    use futures_util::task::noop_waker_ref;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    #[test]
    fn next_returns_error_when_polled_after_completion() {
        let mut next = Next {
            state: Some(NextState::Running(Box::pin(async { status!(204) }))),
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
            state: Some(NextState::Running(Box::pin(async { status!(204) }))),
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
