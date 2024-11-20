﻿use std::{pin::Pin, sync::Arc, future::Future};
use crate::{app::BoxedHttpResultFuture, HttpResult, HttpRequest};

pub(crate) type RouteHandler = Arc<dyn Handler + Send + Sync>;

// Encapsulates the complexity of async and sync handlers
pub(crate) trait Handler {
    fn call(&self, req: HttpRequest) -> Pin<BoxedHttpResultFuture>;
}

#[cfg(feature = "async")]
pub(crate) struct AsyncHandler<F>(pub F);

#[cfg(feature = "async")]
impl<F, Fut> Handler for AsyncHandler<F>
where
    F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HttpResult> + Send + 'static,
{
    fn call(&self, req: HttpRequest) -> Pin<BoxedHttpResultFuture> {
        Box::pin(self.0(req))
    }
}

#[cfg(feature = "sync")]
pub(crate) struct SyncHandler<F>(pub F);

#[cfg(feature = "sync")]
impl<F> Handler for SyncHandler<F>
where
    F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static,
{
    fn call(&self, req: HttpRequest) -> Pin<BoxedHttpResultFuture> {
        let response = self.0(req);
        Box::pin(async move { response })
    }
}