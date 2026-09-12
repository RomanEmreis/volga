use crate::HttpResult;
use crate::error::Error;
use crate::http::{IntoResponse, endpoints::args::FromRequest};
use futures_util::future::BoxFuture;
use std::{future::Future, sync::Arc};

#[cfg(not(feature = "middleware"))]
use crate::HttpRequest;

#[cfg(feature = "middleware")]
use crate::middleware::{HttpContext, NextFn};

/// Represents a specific registered request handler
pub(crate) type RouteHandler = Arc<dyn Handler + Send + Sync>;

pub(crate) trait Handler {
    #[cfg(not(feature = "middleware"))]
    fn call(&self, req: HttpRequest) -> BoxFuture<'_, HttpResult>;

    /// Turns the handler into the [`NextFn`] a route's middleware chain ends in
    ///
    /// The handler's own future is the whole of what reaching it costs: nothing is wrapped
    /// around it, and there is no `next` after it to hand over.
    #[cfg(feature = "middleware")]
    fn into_next(self: Arc<Self>) -> NextFn;
}

/// Represents a function request handler that could take different arguments
/// that implements [`FromRequest`] trait.
pub(crate) struct Func<F, R, Args>
where
    F: GenericHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequest,
{
    func: F,
    _marker: std::marker::PhantomData<fn(Args)>,
}

impl<F, R, Args> Func<F, R, Args>
where
    F: GenericHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequest,
{
    /// Creates a new [`Func`] wrapped into [`Arc`]
    #[inline]
    pub(crate) fn new(func: F) -> Arc<Self> {
        Arc::new(Self::new_local(func))
    }

    /// Creates a new [`Func`]
    #[inline]
    pub(crate) fn new_local(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<F, R, Args> Handler for Func<F, R, Args>
where
    F: GenericHandler<Args, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequest + Send + 'static,
{
    #[inline]
    #[cfg(not(feature = "middleware"))]
    fn call(&self, req: HttpRequest) -> BoxFuture<'_, HttpResult> {
        Box::pin(async move {
            let args = Args::from_request(req).await?;
            self.func.call(args).await.into_response()
        })
    }

    #[cfg(feature = "middleware")]
    fn into_next(self: Arc<Self>) -> NextFn {
        Arc::new(move |ctx: HttpContext| -> BoxFuture<'static, HttpResult> {
            let (req, _, _) = ctx.into_parts();
            let req = req.freeze();

            // A handler with no state of its own - a function, or a closure capturing
            // nothing - is copied into the future, which is free and touches no count. One
            // that captures state is reached through its `Arc` instead: a write to a count
            // every request to this route shares, where cloning what the closure captured
            // could cost anything at all
            if size_of::<F>() == 0 {
                let func = self.func.clone();
                Box::pin(async move {
                    let args = Args::from_request(req).await?;
                    func.call(args).await.into_response()
                })
            } else {
                let this = Arc::clone(&self);
                Box::pin(async move {
                    let args = Args::from_request(req).await?;
                    this.func.call(args).await.into_response()
                })
            }
        })
    }
}

/// Describes a generic request handler that could take 0 or N parameters of types
/// that are implement `FromPayload` trait
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a request handler",
    label = "not a handler",
    note = "a handler is an `async fn`, or a closure returning a future, taking up to 10 extractors as arguments",
    note = "it must also be `Clone + Send + Sync + 'static`, which a closure capturing a non-`Send` value is not"
)]
pub trait GenericHandler<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls a generic handler
    fn call(&self, args: Args) -> impl Future<Output = Self::Output> + Send;
}

/// Describes a generic `map_err` middleware handler that could take 0 or N parameters and [`Error`]
pub trait MapErr<Args>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls an error handler
    fn map_err(&self, err: Error, args: Args) -> impl Future<Output = Self::Output> + Send;
}

macro_rules! define_generic_handler ({ $($param:ident)* } => {
    impl<Func, Fut: Send, $($param,)*> GenericHandler<($($param,)*)> for Func
    where
        Func: Fn($($param),*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> + Send {
            (self)($($param,)*)
        }
    }
    impl<Func, Fut: Send, $($param,)*> MapErr<($($param,)*)> for Func
    where
        Func: Fn(Error, $($param,)*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;

        #[inline]
        #[allow(non_snake_case)]
        fn map_err(&self, err: Error, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            (self)(err, $($param,)*)
        }
    }
});

define_generic_handler! {}
define_generic_handler! { T1 }
define_generic_handler! { T1 T2 }
define_generic_handler! { T1 T2 T3 }
define_generic_handler! { T1 T2 T3 T4 }
define_generic_handler! { T1 T2 T3 T4 T5 }
define_generic_handler! { T1 T2 T3 T4 T5 T6 }
define_generic_handler! { T1 T2 T3 T4 T5 T6 T7 }
define_generic_handler! { T1 T2 T3 T4 T5 T6 T7 T8 }
define_generic_handler! { T1 T2 T3 T4 T5 T6 T7 T8 T9 }
define_generic_handler! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 }

#[cfg(test)]
mod tests {
    use super::{GenericHandler, MapErr};
    use crate::error::Error;

    #[tokio::test]
    async fn generic_handler_invokes_function_with_arguments() {
        let handler = |a: i32, b: i32| async move { a + b };
        let result = GenericHandler::call(&handler, (2, 3)).await;

        assert_eq!(result, 5);
    }

    #[tokio::test]
    async fn map_err_handler_invokes_function_with_error_and_args() {
        let handler = |err: Error, code: u16| async move { (err.status.as_u16(), code) };
        let err = Error::client_error("bad");

        let result = MapErr::map_err(&handler, err, (42,)).await;
        assert_eq!(result, (400, 42));
    }
}

#[cfg(all(test, feature = "middleware"))]
mod next_tests {
    use super::{Func, RouteHandler};
    use crate::http::cors::CorsOverride;
    use crate::middleware::HttpContext;
    use crate::{HttpBody, HttpRequest, ok};
    use hyper::Request;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn ctx() -> HttpContext {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        HttpContext::new(
            HttpRequest::from_parts(parts, body),
            None,
            CorsOverride::Inherit,
        )
    }

    /// State a handler captures, counting how many times it is cloned
    #[derive(Default)]
    struct CountsClones(Arc<AtomicUsize>);

    impl Clone for CountsClones {
        fn clone(&self) -> Self {
            self.0.fetch_add(1, Ordering::SeqCst);
            Self(self.0.clone())
        }
    }

    #[tokio::test]
    async fn it_reaches_a_handler_with_no_state() {
        let handler: RouteHandler = Func::new(|| async { ok!() });
        let next = handler.into_next();

        let response = next(ctx()).await.unwrap();

        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_does_not_clone_the_state_a_handler_captures() {
        let state = CountsClones::default();
        let clones = state.0.clone();
        let handler: RouteHandler = Func::new(move || {
            let _state = &state;
            async { ok!() }
        });
        let next = handler.into_next();

        for _ in 0..3 {
            let response = next(ctx()).await.unwrap();
            assert_eq!(response.status(), 200);
        }

        assert_eq!(clones.load(Ordering::SeqCst), 0);
    }
}
