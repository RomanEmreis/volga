//! Extractors for middleware functions

use futures_util::future::BoxFuture;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{HttpContext, NextFn};
use crate::error::Error;
use crate::http::{IntoResponse, marker, request::IntoTapResult};
use crate::{HttpRequestMut, HttpResponse, HttpResult, http::FilterResult};

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
pub trait Middleware: Send + Sync + 'static {
    /// Calls the middleware handler
    fn call(
        &self,
        ctx: HttpContext,
        next: NextFn,
    ) -> impl Future<Output = HttpResult> + Send + 'static;
}

/// Describes a generic middleware handler that could take 0 or N parameters and [`Next`] middleware
pub trait With<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls the middleware handler
    fn with(&self, args: Args, next: Next) -> impl Future<Output = Self::Output> + Send;
}

/// Describes a filter middleware handler that could take 0 or N parameters and return [`FilterResult`]
///
/// `M` is the filter's shape - [`marker::Async`] or [`marker::Immediate`] - inferred where the
/// filter is registered: a filter may return a future or its verdict directly.
///
/// Neither impl applies to a filter whose verdict does not convert into [`FilterResult`], so
/// rustc cannot tell which shape was meant and reports this trait rather than the conversion.
/// The message below names what a verdict can be.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a filter",
    label = "not a filter",
    note = "a filter is an `async fn` or a closure returning a future, or a plain `fn` or closure returning its verdict directly, taking up to 10 borrowing extractors as arguments",
    note = "the verdict is `bool`, `()`, `FilterResult` or `Result<(), E>`, where `E` is `volga::error::Error` or implements `volga::error::IntoError`, as the `Err` of a handler's `Result` does",
    note = "it must also be `Clone + Send + Sync + 'static`, which a closure capturing a non-`Send` value is not"
)]
pub trait Filter<Args, M = marker::Async>: Clone + Send + Sync + 'static {
    /// Return type
    type Output: Into<FilterResult>;

    /// Calls the filter handler
    fn filter(&self, args: Args) -> impl Future<Output = Self::Output> + Send;
}

/// Describes a generic `tap_req` middleware handler that could take 0 or N parameters and [`HttpRequestMut`]
///
/// `M` is the handler's shape - [`marker::Async`] or [`marker::Immediate`] - inferred where it
/// is registered: it may return a future or the request directly.
pub trait TapReq<Args = (), M = marker::Async>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls the `tap_req` handler
    fn tap_req(&self, req: HttpRequestMut, args: Args)
    -> impl Future<Output = Self::Output> + Send;
}

/// Describes a generic `map_ok` middleware handler that could take 0 or N parameters and [`HttpResponse`]
///
/// `M` is the handler's shape - [`marker::Async`] or [`marker::Immediate`] - inferred where it
/// is registered: it may return a future or the response directly.
pub trait MapOk<Args, M = marker::Async>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls the `map_ok` handler
    fn map_ok(&self, resp: HttpResponse, args: Args) -> impl Future<Output = Self::Output> + Send;
}

impl<Func, Fut: Send> Middleware for Func
where
    Func: Fn(HttpContext, NextFn) -> Fut + Send + Sync + 'static,
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
impl<Func, Fut: Send> TapReq<(), marker::Async> for Func
where
    Func: Fn(HttpRequestMut) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future,
{
    type Output = Fut::Output;

    #[inline]
    fn tap_req(&self, req: HttpRequestMut, _args: ()) -> impl Future<Output = Self::Output> + Send {
        self(req)
    }
}

#[cfg(not(feature = "di"))]
impl<Func, R> TapReq<(), marker::Immediate> for Func
where
    Func: Fn(HttpRequestMut) -> R + Send + Sync + Clone + 'static,
    R: IntoTapResult + Send,
{
    type Output = R;

    #[inline]
    fn tap_req(&self, req: HttpRequestMut, _args: ()) -> impl Future<Output = Self::Output> + Send {
        std::future::ready(self(req))
    }
}

macro_rules! define_generic_mw_handler ({ $($param:ident)* } => {
    impl<Func, Fut: Send, $($param,)*> With<($($param,)*)> for Func
    where
        Func: Fn($($param,)* Next) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;

        #[inline]
        #[allow(non_snake_case)]
        fn with(&self, ($($param,)*): ($($param,)*), next: Next) -> impl Future<Output = Self::Output> {
            (self)($($param,)* next)
        }
    }
    impl<Func, Fut: Send, $($param,)*> Filter<($($param,)*), marker::Async> for Func
    where
        Func: Fn($($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
        Fut::Output: Into<FilterResult>,
    {
        type Output = Fut::Output;

        #[inline]
        #[allow(non_snake_case)]
        fn filter(&self, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            (self)($($param,)*)
        }
    }
    // `Into<FilterResult>` is what keeps a future out of this impl; see `GenericHandler`
    impl<Func, R, $($param,)*> Filter<($($param,)*), marker::Immediate> for Func
    where
        Func: Fn($($param,)*) -> R + Send + Sync + Clone + 'static,
        R: Into<FilterResult> + Send,
    {
        type Output = R;

        #[inline]
        #[allow(non_snake_case)]
        fn filter(&self, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            std::future::ready((self)($($param,)*))
        }
    }
    #[cfg(feature = "di")]
    impl<Func, Fut: Send, $($param,)*> TapReq<($($param,)*), marker::Async> for Func
    where
        Func: Fn(HttpRequestMut,$($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;

        #[inline]
        #[allow(non_snake_case)]
        fn tap_req(&self, req: HttpRequestMut, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            (self)(req, $($param,)*)
        }
    }
    // `IntoTapResult` is what keeps a future out of this impl; see `GenericHandler`
    #[cfg(feature = "di")]
    impl<Func, R, $($param,)*> TapReq<($($param,)*), marker::Immediate> for Func
    where
        Func: Fn(HttpRequestMut,$($param,)*) -> R + Send + Sync + Clone + 'static,
        R: IntoTapResult + Send,
    {
        type Output = R;

        #[inline]
        #[allow(non_snake_case)]
        fn tap_req(&self, req: HttpRequestMut, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            std::future::ready((self)(req, $($param,)*))
        }
    }
    impl<Func, Fut: Send, $($param,)*> MapOk<($($param,)*), marker::Async> for Func
    where
        Func: Fn(HttpResponse,$($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;

        #[inline]
        #[allow(non_snake_case)]
        fn map_ok(&self, resp: HttpResponse, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            (self)(resp, $($param,)*)
        }
    }
    // `IntoResponse` is what keeps a future out of this impl; see `GenericHandler`
    impl<Func, R, $($param,)*> MapOk<($($param,)*), marker::Immediate> for Func
    where
        Func: Fn(HttpResponse,$($param,)*) -> R + Send + Sync + Clone + 'static,
        R: IntoResponse + Send,
    {
        type Output = R;

        #[inline]
        #[allow(non_snake_case)]
        fn map_ok(&self, resp: HttpResponse, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            std::future::ready((self)(resp, $($param,)*))
        }
    }
});

define_generic_mw_handler! {}
define_generic_mw_handler! { T1 }
define_generic_mw_handler! { T1 T2 }
define_generic_mw_handler! { T1 T2 T3 }
define_generic_mw_handler! { T1 T2 T3 T4 }
define_generic_mw_handler! { T1 T2 T3 T4 T5 }
define_generic_mw_handler! { T1 T2 T3 T4 T5 T6 }
define_generic_mw_handler! { T1 T2 T3 T4 T5 T6 T7 }
define_generic_mw_handler! { T1 T2 T3 T4 T5 T6 T7 T8 }
define_generic_mw_handler! { T1 T2 T3 T4 T5 T6 T7 T8 T9 }
define_generic_mw_handler! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 }

#[cfg(test)]
mod tests {
    use super::{Filter, MapOk, Next, NextState, With};
    use crate::error::Error;
    use crate::http::marker;
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

        let response = With::with(&handler, (7,), next).await.unwrap();
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

        let result = MapOk::map_ok(&handler, response, ("ok",)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn immediate_map_ok_handler_returns_its_value() {
        let handler = |resp: HttpResponse, extra: u16| {
            assert_eq!(extra, 7);
            resp
        };

        let response = HttpResponse::builder()
            .status(201)
            .body(HttpBody::from("ok"))
            .unwrap();

        let response = MapOk::<_, marker::Immediate>::map_ok(&handler, response, (7,)).await;
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn immediate_filter_returns_its_verdict() {
        let handler = |value: i32| value > 0;

        let accepted = Filter::<_, marker::Immediate>::filter(&handler, (1,)).await;
        let rejected = Filter::<_, marker::Immediate>::filter(&handler, (-1,)).await;

        assert!(accepted);
        assert!(!rejected);
    }
}
