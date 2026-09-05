use std::sync::Arc;

use crate::{
    error::{
        FallbackFunc,
        fallback::{PipelineFallbackHandler, default_fallback_handler},
        handler::{DefaultErrorHandler, PipelineErrorHandler},
    },
    http::endpoints::{Endpoints, route::RoutePipeline},
};

#[cfg(feature = "middleware")]
use crate::{
    HttpResult,
    middleware::{HttpContext, Middlewares, NextFn},
};

/// The terminal stage of the request pipeline: whatever answers the request
/// once the global middleware chain has run.
///
/// Routing happens before that chain, so which of the three answers is decided
/// up front. Carrying the decision through the chain instead of acting on it
/// immediately is what keeps global middleware - and the per-request scope it
/// reads - on requests that match no route.
pub(crate) enum Terminal {
    /// The matched route's own middleware pipeline and handler.
    Route(RoutePipeline),

    /// No route matched the path: the application fallback answers.
    Fallback(PipelineFallbackHandler),

    /// The path matched but the method did not: `405` with an `Allow` header
    /// listing the methods the path does have.
    MethodNotAllowed(Arc<str>),
}

pub(crate) struct PipelineBuilder {
    #[cfg(feature = "middleware")]
    middlewares: Middlewares,
    endpoints: Endpoints,
    error_handler: PipelineErrorHandler,
    fallback_handler: PipelineFallbackHandler,
}

impl std::fmt::Debug for PipelineBuilder {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PipelineBuilder(..)")
    }
}

pub(crate) struct Pipeline {
    #[cfg(feature = "middleware")]
    start: Option<NextFn>,
    endpoints: Endpoints,
    error_handler: PipelineErrorHandler,
    fallback_handler: PipelineFallbackHandler,
}

impl PipelineBuilder {
    #[cfg(feature = "middleware")]
    pub(super) fn new() -> Self {
        Self {
            middlewares: Middlewares::new(),
            endpoints: Endpoints::new(),
            error_handler: Arc::new(DefaultErrorHandler),
            fallback_handler: FallbackFunc::new(default_fallback_handler).into(),
        }
    }

    #[cfg(not(feature = "middleware"))]
    pub(super) fn new() -> Self {
        Self {
            endpoints: Endpoints::new(),
            error_handler: Arc::new(DefaultErrorHandler),
            fallback_handler: FallbackFunc::new(default_fallback_handler).into(),
        }
    }

    #[cfg(feature = "middleware")]
    pub(super) fn build(mut self) -> Pipeline {
        let start = self.middlewares.compose();
        self.endpoints.compose();
        Pipeline {
            endpoints: self.endpoints,
            error_handler: self.error_handler,
            fallback_handler: self.fallback_handler,
            start,
        }
    }

    #[cfg(not(feature = "middleware"))]
    pub(super) fn build(self) -> Pipeline {
        Pipeline {
            endpoints: self.endpoints,
            error_handler: self.error_handler,
            fallback_handler: self.fallback_handler,
        }
    }

    #[cfg(feature = "middleware")]
    pub(crate) fn has_middleware_pipeline(&self) -> bool {
        !self.middlewares.is_empty()
    }

    #[cfg(feature = "middleware")]
    pub(crate) fn middlewares_mut(&mut self) -> &mut Middlewares {
        &mut self.middlewares
    }

    pub(crate) fn endpoints_mut(&mut self) -> &mut Endpoints {
        &mut self.endpoints
    }

    pub(crate) fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    pub(crate) fn set_error_handler(&mut self, handler: PipelineErrorHandler) {
        self.error_handler = handler;
    }

    pub(crate) fn set_fallback_handler(&mut self, handler: PipelineFallbackHandler) {
        self.fallback_handler = handler;
    }
}

impl Pipeline {
    #[inline]
    pub(crate) fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    #[inline]
    pub(super) fn error_handler(&self) -> &PipelineErrorHandler {
        &self.error_handler
    }

    #[inline]
    pub(super) fn fallback_handler(&self) -> &PipelineFallbackHandler {
        &self.fallback_handler
    }

    #[cfg(feature = "middleware")]
    pub(crate) async fn execute(&self, ctx: HttpContext) -> HttpResult {
        if let Some(next) = &self.start {
            next(ctx).await
        } else {
            ctx.execute().await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PipelineBuilder;
    use crate::error::{
        ErrorFunc, FallbackFunc, fallback::PipelineFallbackHandler, handler::PipelineErrorHandler,
    };
    use crate::status;

    #[test]
    fn it_sets_error_and_fallback_handlers() {
        let mut builder = PipelineBuilder::new();

        let error_handler: PipelineErrorHandler =
            ErrorFunc::new(|_err| async { status!(418) }).into();
        builder.set_error_handler(error_handler.clone());
        assert!(std::sync::Arc::ptr_eq(
            &builder.error_handler,
            &error_handler
        ));

        let fallback_handler: PipelineFallbackHandler =
            FallbackFunc::new(|| async { status!(404) }).into();
        builder.set_fallback_handler(fallback_handler.clone());
        assert!(std::sync::Arc::ptr_eq(
            &builder.fallback_handler,
            &fallback_handler
        ));
    }

    #[cfg(feature = "middleware")]
    #[test]
    fn it_builds_without_middleware_pipeline() {
        let builder = PipelineBuilder::new();
        assert!(!builder.has_middleware_pipeline());

        let pipeline = builder.build();
        assert!(pipeline.start.is_none());
    }
}
