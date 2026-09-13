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

/// Type-level markers telling the two shapes of a handler apart
///
/// A handler is recognized by its signature alone, and the two shapes - one returning a
/// future, one returning its response directly - cannot be separated by a `where` clause:
/// an impl for each would overlap, and coherence rejects that. Carrying the shape as a type
/// parameter keeps the impls distinct, and the marker is inferred where the handler is
/// registered, so it never appears in handler code.
///
/// # Which shape to write
///
/// - The body **awaits** something - a database driver, an HTTP call, a file read through
///   `tokio::fs` - write an `async fn` or a closure returning a future.
///   ([`Async`](crate::marker::Async))
/// - The body is **computation on data already in hand** - formatting, arithmetic, a lookup
///   in a map, a check of a header - write a plain `fn` or a closure returning the response.
///   It runs inline on the worker that polls the request, with no extra state machine, which
///   is the cheapest thing the server can do. ([`Immediate`](crate::marker::Immediate))
/// - The body **blocks** - `std::fs`, a synchronous database or HTTP client, a long
///   computation - write a plain `fn` and register it through [`blocking`](crate::blocking).
///   Left inline, it would hold a runtime worker for its whole duration, and that worker
///   polls nothing else meanwhile.
///
/// Extractors run before the handler in every shape, so a synchronous handler still takes
/// `Json<T>`, `Form<T>`, `Query<T>` or `Dc<T>` with its body already read. A streaming
/// extractor - [`ByteStream`](crate::ByteStream), [`File`](crate::File) - has nothing to
/// offer one, since reading it takes an `await`.
///
/// # Examples
/// ```no_run
/// use volga::{App, ok};
///
///# #[tokio::main]
///# async fn main() -> std::io::Result<()> {
/// let mut app = App::new();
///
/// // `marker::Async`: the handler returns a future, which the server awaits
/// app.map_get("/hello/{name}", |name: String| async move {
///     ok!("Hello, {name}!")
/// });
///
/// // `marker::Immediate`: the handler returns the response itself
/// app.map_get("/sum/{x}/{y}", |x: i32, y: i32| x + y);
///# app.run().await
///# }
/// ```
pub mod marker {
    /// Marks a handler that returns a [`Future`](std::future::Future) for the server to await
    ///
    /// This is the default marker of every handler trait, so a bound written without one -
    /// `F: GenericHandler<Args>` - means exactly what it meant before synchronous handlers
    /// existed.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_get("/hello", || async { ok!("Hello World!") });
    ///# app.run().await
    ///# }
    /// ```
    #[derive(Debug)]
    pub struct Async;

    /// Marks a handler that returns its response directly, with nothing to await
    ///
    /// Such a handler runs to completion on the runtime worker that polls the request. That
    /// is right for computation and lookups; a handler that genuinely blocks should either
    /// be asynchronous or be wrapped in [`blocking`](crate::blocking), which moves it off
    /// that worker.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_get("/hello", || ok!("Hello World!"));
    ///# app.run().await
    ///# }
    /// ```
    #[derive(Debug)]
    pub struct Immediate;
}

/// Runs a synchronous request handler on Tokio's blocking pool instead of the runtime worker
/// that polls the request
///
/// A [`marker::Immediate`] handler runs inline, which is right for computation and lookups
/// and wrong for anything that actually blocks - file or socket I/O through `std`, a
/// synchronous database driver, a long computation. Blocking a runtime worker stalls every
/// other request it was going to poll. Wrapped here, the extractors still run on the worker,
/// and the body is moved to [`tokio::task::spawn_blocking`] and awaited, so the worker stays
/// free.
///
/// The trade is a hand-off to another thread, which costs far more than a short body does:
/// reach for this only when the body really blocks. See [`marker`] for the shapes side by
/// side.
///
/// It takes a *synchronous* handler: an asynchronous one has nothing to offload, since it
/// already yields, and is rejected at compile time.
///
/// # Cancellation
///
/// The offloaded call is **not cancelled** with the request: once started it runs to
/// completion, and its result is discarded if the client has gone. A long body that should
/// stop early can take a [`CancellationToken`](crate::CancellationToken) and check
/// `is_cancelled()` as it goes.
///
/// If the runtime shuts down before the call starts, the request is answered with
/// `500 Internal Server Error`.
///
/// # Panics
///
/// A panic inside the handler is resumed on the task that awaited it, exactly where it would
/// have landed had the handler run inline.
///
/// # Example
/// ```no_run
/// use volga::{App, blocking, ok};
///
///# #[tokio::main]
///# async fn main() -> std::io::Result<()> {
/// let mut app = App::new();
///
/// app.map_get("/reports/{id}", blocking(|id: u32| {
///     let report = std::fs::read_to_string(format!("reports/{id}.txt"))?;
///     ok!(report)
/// }));
///# app.run().await
///# }
/// ```
#[inline]
pub fn blocking<F>(handler: F) -> BlockingFn<F> {
    BlockingFn(Arc::new(handler))
}

/// A synchronous request handler moved onto Tokio's blocking pool. Created by [`blocking`].
///
/// The handler is shared rather than cloned per request, so what it captures does not have
/// to be [`Clone`].
pub struct BlockingFn<F>(Arc<F>);

impl<F> Clone for BlockingFn<F> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<F> std::fmt::Debug for BlockingFn<F> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BlockingFn(..)")
    }
}

/// Awaits a call running on the blocking pool, putting a panic back where it came from
#[inline]
async fn join_blocking<R>(handle: tokio::task::JoinHandle<R>) -> Result<R, Error> {
    match handle.await {
        Ok(response) => Ok(response),
        Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
        // A blocking task is cancelled only when the runtime shuts down before it starts
        Err(err) => Err(Error::server_error(err)),
    }
}

/// Represents a function request handler that could take different arguments
/// that implements [`FromRequest`] trait.
pub(crate) struct Func<F, R, Args, M>
where
    F: GenericHandler<Args, M, Output = R>,
    R: IntoResponse,
    Args: FromRequest,
{
    func: F,
    _marker: std::marker::PhantomData<fn(Args, M)>,
}

impl<F, R, Args, M> Func<F, R, Args, M>
where
    F: GenericHandler<Args, M, Output = R>,
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

impl<F, R, Args, M> Handler for Func<F, R, Args, M>
where
    F: GenericHandler<Args, M, Output = R>,
    R: IntoResponse + 'static,
    Args: FromRequest + Send + 'static,
    M: 'static,
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
///
/// `M` is the handler's shape - [`marker::Async`] or [`marker::Immediate`] - and is inferred
/// where the handler is registered. It defaults to [`marker::Async`], so a bound that does not
/// spell it out accepts asynchronous handlers only.
///
/// # Keeping the two shapes apart
///
/// The [`marker::Immediate`] impl is what excludes a future: it requires the return type to
/// implement [`IntoResponse`], which no future does. That bound has to stay on the impl.
/// rustc chooses between the two impls by their own where-clauses, so a registration method
/// adding `R: IntoResponse` after the fact does not take part in the choice - and an impl
/// loosened to accept any return type would make every asynchronous handler ambiguous.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a request handler",
    label = "not a handler",
    note = "a handler is an `async fn` or a closure returning a future, or a plain `fn` or closure returning a response directly, taking up to 10 extractors as arguments",
    note = "it must also be `Clone + Send + Sync + 'static`, which a closure capturing a non-`Send` value is not"
)]
pub trait GenericHandler<Args, M = marker::Async>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls a generic handler
    fn call(&self, args: Args) -> impl Future<Output = Self::Output> + Send;
}

/// Describes a generic `map_err` middleware handler that could take 0 or N parameters and [`Error`]
///
/// `M` is the handler's shape, inferred as for [`GenericHandler`]: an error handler may return
/// a future or its response directly.
pub trait MapErr<Args, M = marker::Async>: Clone + Send + Sync + 'static {
    /// Return type
    type Output;

    /// Calls an error handler
    fn map_err(&self, err: Error, args: Args) -> impl Future<Output = Self::Output> + Send;
}

macro_rules! define_generic_handler ({ $($param:ident)* } => {
    impl<Func, Fut: Send, $($param,)*> GenericHandler<($($param,)*), marker::Async> for Func
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
    impl<Func, R, $($param,)*> GenericHandler<($($param,)*), marker::Immediate> for Func
    where
        Func: Fn($($param),*) -> R + Send + Sync + Clone + 'static,
        R: IntoResponse + Send,
    {
        type Output = R;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> + Send {
            std::future::ready((self)($($param,)*))
        }
    }
    #[diagnostic::do_not_recommend]
    impl<Func, R, $($param,)*> GenericHandler<($($param,)*), marker::Async> for BlockingFn<Func>
    where
        Func: Fn($($param),*) -> R + Send + Sync + 'static,
        R: IntoResponse + Send + 'static,
        $($param: Send + 'static,)*
    {
        // `Result<R, _>` describes itself to OpenAPI as `R` does
        type Output = Result<R, Error>;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> + Send {
            let func = Arc::clone(&self.0);
            async move {
                join_blocking(tokio::task::spawn_blocking(move || func($($param,)*))).await
            }
        }
    }
    impl<Func, Fut: Send, $($param,)*> MapErr<($($param,)*), marker::Async> for Func
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
    impl<Func, R, $($param,)*> MapErr<($($param,)*), marker::Immediate> for Func
    where
        Func: Fn(Error, $($param,)*) -> R + Send + Sync + Clone + 'static,
        R: IntoResponse + Send,
    {
        type Output = R;

        #[inline]
        #[allow(non_snake_case)]
        fn map_err(&self, err: Error, ($($param,)*): ($($param,)*)) -> impl Future<Output = Self::Output> {
            std::future::ready((self)(err, $($param,)*))
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
    use super::{GenericHandler, MapErr, blocking, join_blocking, marker};
    use crate::error::Error;
    use crate::{HttpResult, status};
    use std::sync::Mutex;

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

    #[tokio::test]
    async fn immediate_handler_returns_its_value() {
        let handler = |a: i32, b: i32| a + b;
        let result = GenericHandler::<_, marker::Immediate>::call(&handler, (2, 3)).await;

        assert_eq!(result, 5);
    }

    #[tokio::test]
    async fn immediate_map_err_handler_returns_its_value() {
        let handler = |err: Error, code: u16| status!(err.status.as_u16(), "{code}");
        let err = Error::client_error("bad");

        let response = MapErr::<_, marker::Immediate>::map_err(&handler, err, (42,))
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn blocking_handler_runs_off_the_polling_thread() {
        let polling = std::thread::current().id();
        let handler = blocking(move |id: u32| {
            assert_ne!(std::thread::current().id(), polling);
            id + 1
        });

        let result = GenericHandler::call(&handler, (1,)).await.unwrap();
        assert_eq!(result, 2);
    }

    #[tokio::test]
    async fn blocking_handler_shares_state_that_is_not_clone() {
        let hits = Mutex::new(0u32);
        let handler = blocking(move || {
            let mut hits = hits.lock().unwrap();
            *hits += 1;
            *hits
        });

        assert_eq!(GenericHandler::call(&handler.clone(), ()).await.unwrap(), 1);
        assert_eq!(GenericHandler::call(&handler, ()).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn blocking_handler_resumes_a_panic_on_the_awaiting_task() {
        let handler = blocking(|| -> HttpResult { panic!("boom") });

        let joined = tokio::spawn(async move { GenericHandler::call(&handler, ()).await }).await;

        let panic = joined.unwrap_err().into_panic();
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"boom"));
    }

    #[tokio::test]
    async fn blocking_join_answers_a_cancelled_call_with_a_server_error() {
        let handle = tokio::spawn(std::future::pending::<()>());
        handle.abort();

        let err = join_blocking(handle).await.unwrap_err();
        assert!(err.is_server_error());
    }
}

#[cfg(all(test, feature = "middleware"))]
mod next_tests {
    use super::{Func, RouteHandler, blocking};
    use crate::http::cors::CorsOverride;
    use crate::middleware::HttpContext;
    use crate::{HttpBody, HttpRequest, ok, status};
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
    async fn it_reaches_an_immediate_handler() {
        let handler: RouteHandler = Func::new(|| status!(204));
        let next = handler.into_next();

        let response = next(ctx()).await.unwrap();

        assert_eq!(response.status(), 204);
    }

    #[tokio::test]
    async fn it_reaches_a_blocking_handler() {
        let handler: RouteHandler = Func::new(blocking(|| status!(202)));
        let next = handler.into_next();

        let response = next(ctx()).await.unwrap();

        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_does_not_clone_the_state_an_immediate_handler_captures() {
        let state = CountsClones::default();
        let clones = state.0.clone();
        let handler: RouteHandler = Func::new(move || {
            let _state = &state;
            ok!()
        });
        let next = handler.into_next();

        for _ in 0..3 {
            let response = next(ctx()).await.unwrap();
            assert_eq!(response.status(), 200);
        }

        assert_eq!(clones.load(Ordering::SeqCst), 0);
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
