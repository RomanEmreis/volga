//! Extractors for middleware functions

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::error::Error;
use crate::{HttpRequest, HttpResponse, HttpResult};
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
    next: NextFn,
    ctx: Option<HttpContext>
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
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ctx = self.ctx
            .take()
            .ok_or_else(|| Error::server_error("Next polled after completion"))?;
        let fut = (self.next)(ctx);
        Box::pin(fut).as_mut().poll(cx)
    }
}

impl Next {
    /// Creates a new [`Next`]
    pub fn new(ctx: HttpContext, next: NextFn) -> Self {
        Self { ctx: Some(ctx), next }
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

/// Describes a generic [`tap_req`] middleware handler that could take 0 or N parameters and [`HttpRequest`]
pub trait TapReqHandler<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;
    /// Tap handler future
    type Future: Future<Output = Self::Output> + Send;

    /// Calls the [`tap_req`] handler
    fn call(&self, req: HttpRequest, args: Args) -> Self::Future;
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
    impl<Func, Fut: Send, $($param,)*> TapReqHandler<($($param,)*)> for Func
    where
        Func: Fn(HttpRequest,$($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;
        type Future = Fut;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, req: HttpRequest, ($($param,)*): ($($param,)*)) -> Self::Future {
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