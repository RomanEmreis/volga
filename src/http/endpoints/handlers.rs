use std::{sync::Arc, future::Future};
use futures_util::future::BoxFuture;
use crate::{HttpResult, HttpRequest};
use crate::http::{
    endpoints::args::FromRequest,
    IntoResponse
};

/// Represents a specific registered request handler
pub(crate) type RouteHandler = Arc<
    dyn Handler 
    + Send 
    + Sync
>;

pub(crate) trait Handler {
    fn call(&self, req: HttpRequest) -> BoxFuture<HttpResult>;
}

/// Represents a function request handler that could take different arguments
/// that implements [`FromRequest`] trait.
pub(crate) struct Func<F, R, Args>
where
    F: GenericHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequest
{
    func: F,
    _marker: std::marker::PhantomData<Args>,
}

impl<F, R ,Args> Func<F, R, Args>
where
    F: GenericHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequest
{
    /// Creates a new [`Func`] wrapped into [`Arc`]
    #[inline]
    pub(crate) fn new(func: F) -> Arc<Self> {
        Arc::new(Self::new_local(func))
    }

    /// Creates a new [`Func`]
    #[inline]
    pub(crate) fn new_local(func: F) -> Self {
        Self { func, _marker: std::marker::PhantomData }
    }
}

impl<F, R, Args> Handler for Func<F, R, Args>
where
    F: GenericHandler<Args, Output = R>,
    R: IntoResponse,
    Args: FromRequest + Send + Sync
{
    #[inline]
    fn call(&self, req: HttpRequest) -> BoxFuture<HttpResult> {
        Box::pin(async move {
            let args = Args::from_request(req).await?;
            self.func
                .call(args)
                .await
                .into_response()
        })
    }
}

/// Describes a generic request handler that could take 0 or N parameters of types
/// that are implement [`FromPayload`] trait
pub trait GenericHandler<Args>: Clone + Send + Sync + 'static {
    type Output;
    type Future: Future<Output = Self::Output> + Send;

    fn call(&self, args: Args) -> Self::Future;
}

macro_rules! define_generic_handler ({ $($param:ident)* } => {
    impl<Func, Fut: Send, $($param,)*> GenericHandler<($($param,)*)> for Func
    where
        Func: Fn($($param),*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;
        type Future = Fut;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ($($param,)*): ($($param,)*)) -> Self::Future {
            (self)($($param,)*)
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
