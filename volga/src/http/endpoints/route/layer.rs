//! Represents tools for "local" middleware

use crate::http::endpoints::handlers::RouteHandler;
use crate::{HttpResult, status};

#[cfg(feature = "middleware")]
use {
    crate::middleware::{HttpContext, MiddlewareFn, Middlewares, NextFn},
    futures_util::future::BoxFuture,
};

#[cfg(not(feature = "middleware"))]
use crate::http::request::HttpRequest;

/// A layer of middleware or a route handler
#[derive(Clone)]
pub(crate) enum Layer {
    Handler(RouteHandler),
    #[cfg(feature = "middleware")]
    Middleware(MiddlewareFn),
}

impl From<RouteHandler> for Layer {
    #[inline]
    fn from(handler: RouteHandler) -> Self {
        Self::Handler(handler)
    }
}

#[cfg(feature = "middleware")]
impl From<MiddlewareFn> for Layer {
    #[inline]
    fn from(mw: MiddlewareFn) -> Self {
        Self::Middleware(mw)
    }
}

impl From<Layer> for RouteHandler {
    #[inline]
    fn from(layer: Layer) -> Self {
        match layer {
            Layer::Handler(handler) => handler,
            #[cfg(feature = "middleware")]
            Layer::Middleware(_) => unreachable!(),
        }
    }
}

/// A route pipeline while the route is being configured: its middleware, outermost first,
/// and what answers once all of it has called `next`
#[cfg(feature = "middleware")]
#[derive(Clone)]
pub(crate) struct RouteLayers {
    middleware: Middlewares,
    terminal: Option<NextFn>,
}

/// Route's middleware pipeline
#[derive(Clone)]
pub(crate) enum RoutePipeline {
    /// Boxed, since it only lives until the pipeline is composed, while the composed one is
    /// moved with every request a route answers
    #[cfg(feature = "middleware")]
    Builder(Box<RouteLayers>),
    #[cfg(feature = "middleware")]
    Middleware(Option<NextFn>),
    #[cfg(not(feature = "middleware"))]
    Handler(Option<RouteHandler>),
}

impl From<Layer> for RoutePipeline {
    fn from(layer: Layer) -> Self {
        let mut pipeline = Self::new();
        pipeline.insert(layer);
        pipeline
    }
}

impl RoutePipeline {
    /// Creates s new middleware pipeline
    pub(super) fn new() -> Self {
        #[cfg(feature = "middleware")]
        let pipeline = Self::Builder(Box::new(RouteLayers {
            middleware: Middlewares::new(),
            terminal: None,
        }));
        #[cfg(not(feature = "middleware"))]
        let pipeline = Self::Handler(None);
        pipeline
    }

    /// Creates a pipeline that ends in `terminal` rather than in a route handler
    ///
    /// A static file mount answers through one of these: it carries a group's middleware the
    /// way a route does, and what answers at the end of it is the file.
    #[cfg(feature = "static-files")]
    pub(crate) fn ending_in(terminal: NextFn) -> Self {
        Self::Builder(Box::new(RouteLayers {
            middleware: Middlewares::new(),
            terminal: Some(terminal),
        }))
    }

    /// Inserts middleware at the front of the chain, ahead of the layers the pipeline
    /// already holds
    ///
    /// This is how a route group applies its middleware once its closure returns: the layers
    /// already there belong to the route itself or to a nested group, and both run inside the
    /// group's. A static file mount is layered the same way - it answers under the group's
    /// prefix rather than through a route, and carries the same pipeline.
    #[cfg(feature = "middleware")]
    pub(crate) fn prepend(&mut self, layers: &[MiddlewareFn]) {
        match self {
            Self::Builder(builder) => builder.middleware.prepend(layers),
            Self::Middleware(_) => (),
        }
    }

    /// Inserts a layer into the pipeline
    pub(super) fn insert(&mut self, layer: Layer) {
        match self {
            #[cfg(feature = "middleware")]
            Self::Builder(builder) => match layer {
                Layer::Handler(handler) => builder.terminal = Some(handler.into_next()),
                Layer::Middleware(mw) => builder.middleware.add(mw),
            },
            #[cfg(feature = "middleware")]
            Self::Middleware(_) => (),
            #[cfg(not(feature = "middleware"))]
            Self::Handler(route_handler) => *route_handler = Some(layer.into()),
        }
    }

    /// Hands the request to the pipeline chain
    ///
    /// The future returned is the chain's own, so reaching a route through here allocates
    /// nothing of its own.
    #[cfg(feature = "middleware")]
    pub(crate) fn call(&self, ctx: HttpContext) -> BoxFuture<'static, HttpResult> {
        match self {
            Self::Middleware(Some(next)) => next(ctx),
            _ => Box::pin(async { status!(405) }),
        }
    }

    /// Calls the request handler
    #[cfg(not(feature = "middleware"))]
    pub(crate) async fn call(self, req: HttpRequest) -> HttpResult {
        match self {
            Self::Handler(Some(handler)) => handler.call(req).await,
            _ => status!(405),
        }
    }

    /// Builds a middleware pipeline
    #[cfg(feature = "middleware")]
    pub(crate) fn compose(&mut self) {
        let next = match self {
            Self::Middleware(_) => return,
            // Layers with nothing to end in - a pipeline no handler was mapped into - compose
            // to nothing, and are answered `405` like a route with no pipeline at all
            Self::Builder(builder) => builder
                .terminal
                .take()
                .map(|terminal| builder.middleware.compose(terminal)),
        };
        *self = Self::Middleware(next)
    }
}

#[cfg(all(test, not(feature = "middleware")))]
mod tests {
    use super::{Layer, RoutePipeline};
    use crate::http::endpoints::handlers::{Handler, RouteHandler};
    use crate::{HttpRequest, HttpResult, status};
    use futures_util::future::BoxFuture;
    use std::sync::Arc;

    struct NoopHandler;

    impl Handler for NoopHandler {
        fn call(&self, _req: HttpRequest) -> BoxFuture<'_, HttpResult> {
            Box::pin(async { status!(204) })
        }
    }

    #[test]
    fn pipeline_from_layer_contains_handler() {
        let handler: RouteHandler = Arc::new(NoopHandler);
        let pipeline = RoutePipeline::from(Layer::from(handler.clone()));

        match pipeline {
            RoutePipeline::Handler(Some(inner)) => {
                assert!(Arc::ptr_eq(&inner, &handler));
            }
            _ => panic!("expected handler pipeline"),
        }
    }

    #[test]
    fn insert_sets_handler_in_pipeline() {
        let handler: RouteHandler = Arc::new(NoopHandler);
        let mut pipeline = RoutePipeline::new();
        pipeline.insert(Layer::from(handler.clone()));

        match pipeline {
            RoutePipeline::Handler(Some(inner)) => {
                assert!(Arc::ptr_eq(&inner, &handler));
            }
            _ => panic!("expected handler pipeline"),
        }
    }
}

#[cfg(all(test, feature = "middleware"))]
mod middleware_tests {
    use super::{Layer, RoutePipeline};
    use crate::http::cors::CorsOverride;
    use crate::http::endpoints::handlers::{Func, RouteHandler};
    use crate::middleware::{HttpContext, MiddlewareFn, NextFn};
    use crate::{HttpBody, HttpRequest, ok};
    use hyper::Request;
    use std::sync::{Arc, Mutex};

    type Log = Arc<Mutex<Vec<&'static str>>>;

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

    fn recording(log: &Log, name: &'static str) -> MiddlewareFn {
        let log = log.clone();
        Arc::new(move |ctx: HttpContext, next: NextFn| {
            log.lock().unwrap().push(name);
            next(ctx)
        })
    }

    #[tokio::test]
    async fn it_runs_group_middleware_then_route_middleware_then_the_handler() {
        let log = Log::default();
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut pipeline = RoutePipeline::from(Layer::from(handler));
        pipeline.insert(Layer::from(recording(&log, "route 1")));
        pipeline.insert(Layer::from(recording(&log, "route 2")));
        pipeline.prepend(&[recording(&log, "group 1"), recording(&log, "group 2")]);
        pipeline.compose();

        let response = pipeline.call(ctx()).await.unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(
            *log.lock().unwrap(),
            ["group 1", "group 2", "route 1", "route 2"]
        );
    }

    #[tokio::test]
    async fn it_answers_405_when_no_handler_was_mapped() {
        let log = Log::default();

        let mut pipeline = RoutePipeline::new();
        pipeline.insert(Layer::from(recording(&log, "route")));
        pipeline.compose();

        let response = pipeline.call(ctx()).await.unwrap();

        assert_eq!(response.status(), 405);
        assert!(log.lock().unwrap().is_empty());
    }
}
