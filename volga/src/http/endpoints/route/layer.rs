//! Represents tools for "local" middleware

use crate::http::endpoints::handlers::RouteHandler;
use crate::{status, HttpResult};

#[cfg(feature = "middleware")]
use crate::middleware::{
    from_handler,
    HttpContext,
    Middlewares,
    MiddlewareFn,
    NextFn,
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

#[cfg(feature = "middleware")]
impl From<Layer> for MiddlewareFn {
    #[inline]
    fn from(layer: Layer) -> Self {
        match layer {
            Layer::Middleware(mw) => mw,
            Layer::Handler(handler) => from_handler(handler)
        }
    }
}

/// Route's middleware pipeline
#[derive(Clone)]
pub(crate) enum RoutePipeline {
    #[cfg(feature = "middleware")]
    Builder(Middlewares),
    #[cfg(feature = "middleware")]
    Middleware(Option<NextFn>),
    #[cfg(not(feature = "middleware"))]
    Handler(Option<RouteHandler>)
}

impl From<Layer> for RoutePipeline {
    fn from(handler: Layer) -> Self {
        #[cfg(feature = "middleware")]
        let pipeline = Self::Builder(Middlewares::from(MiddlewareFn::from(handler)));
        #[cfg(not(feature = "middleware"))]
        let pipeline = Self::Handler(Some(RouteHandler::from(handler)));
        pipeline
    }
}

impl RoutePipeline {
    /// Creates s new middleware pipeline
    pub(super) fn new() -> Self {
        #[cfg(feature = "middleware")]
        let pipeline = Self::Builder(Middlewares::new());
        #[cfg(not(feature = "middleware"))]
        let pipeline = Self::Handler(None);
        pipeline
    }

    /// Inserts a layer into the pipeline
    pub(super) fn insert(&mut self, layer: Layer) {
        match self {
            #[cfg(feature = "middleware")]
            Self::Builder(mx) => mx.add(layer.into()),
            #[cfg(feature = "middleware")]
            Self::Middleware(_) => (),
            #[cfg(not(feature = "middleware"))]
            Self::Handler(ref mut route_handler) => *route_handler = Some(layer.into()),
        }
    }

    /// Calls the pipeline chain
    #[cfg(feature = "middleware")]
    pub(crate) async fn call(self, ctx: HttpContext) -> HttpResult {
        match self {
            Self::Middleware(Some(next)) => {
                let next = next.clone();
                next(ctx).await
            },
            _ => status!(405)
        }
    }

    /// Calls the request handler
    #[cfg(not(feature = "middleware"))]
    pub(crate) async fn call(self, req: HttpRequest) -> HttpResult {
        match self {
            Self::Handler(Some(handler)) => handler.call(req).await,
            _ => status!(405)
        }
    }

    /// Builds a middleware pipeline
    #[cfg(feature = "middleware")]
    pub(super) fn compose(&mut self) {
        let next = match self {
            Self::Middleware(_) => return,
            Self::Builder(mx) => {
                // Unlike global, in route middlewares the route handler 
                // initially locates at the beginning of the pipeline, 
                // so we need to take it to the end
                mx.pipeline.rotate_left(1);
                mx.compose()
            },
        };
        *self = Self::Middleware(next)
    }
}