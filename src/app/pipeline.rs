use std::sync::Arc;
use hyper::{Request, body::Incoming};

use crate::{
    error::{
        ErrorFunc,
        FallbackFunc,
        fallback::{PipelineFallbackHandler, default_fallback_handler},
        handler::{PipelineErrorHandler, WeakErrorHandler, default_error_handler}
    },
    http::endpoints::Endpoints,
    HttpResult
};

#[cfg(feature = "middleware")]
use crate::{middleware::{Middlewares, HttpContext, NextFn}};

pub(crate) struct PipelineBuilder {
    #[cfg(feature = "middleware")]
    middlewares: Middlewares,
    endpoints: Endpoints,
    error_handler: PipelineErrorHandler,
    fallback_handler: PipelineFallbackHandler
}

pub(crate) struct Pipeline {
    #[cfg(feature = "middleware")]
    start: Option<NextFn>,
    endpoints: Endpoints,
    error_handler: PipelineErrorHandler,
    fallback_handler: PipelineFallbackHandler
}

impl PipelineBuilder {
    #[cfg(feature = "middleware")]
    pub(super) fn new() -> Self {
        Self {
            middlewares: Middlewares::new(),
            endpoints: Endpoints::new(),
            error_handler: ErrorFunc::new(default_error_handler).into(),
            fallback_handler: FallbackFunc::new(default_fallback_handler).into()
        }
    }

    #[cfg(not(feature = "middleware"))]
    pub(super) fn new() -> Self {
        Self { 
            endpoints: Endpoints::new(),
            error_handler: ErrorFunc::new(default_error_handler).into(),
            fallback_handler: FallbackFunc::new(default_fallback_handler).into()
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
            start
        }
    }

    #[cfg(not(feature = "middleware"))]
    pub(super) fn build(self) -> Pipeline {
        Pipeline { 
            endpoints: self.endpoints,
            error_handler: self.error_handler,
            fallback_handler: self.fallback_handler
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
    pub(super) fn error_handler(&self) -> WeakErrorHandler {
        Arc::downgrade(&self.error_handler)
    }
    
    #[inline]
    pub(super) async fn fallback(&self, req: Request<Incoming>) -> HttpResult {
        self.fallback_handler.call(req).await
    }
    
    #[cfg(feature = "middleware")]
    pub(crate) fn has_middleware_pipeline(&self) -> bool {
        self.start.is_some()
    }

    #[cfg(feature = "middleware")]
    pub(crate) async fn execute(&self, ctx: HttpContext) -> HttpResult {
        let next = &self.start;
        if let Some(next) = next {
            let next: NextFn = next.clone();
            next(ctx).await
        } else {
            ctx.execute().await
        }
    }
}
