use crate:: app::endpoints::Endpoints;
#[cfg(feature = "middleware")]
use crate::{
    app::middlewares::Middlewares,
    HttpResult,
    HttpContext,
    Next
};

#[cfg(feature = "middleware")]
pub(crate) struct PipelineBuilder {
    middlewares: Middlewares,
    endpoints: Endpoints,
}

#[cfg(not(feature = "middleware"))]
pub(crate) struct PipelineBuilder {
    endpoints: Endpoints,
}

#[cfg(feature = "middleware")]
pub(crate) struct Pipeline {
    endpoints: Endpoints,
    start: Option<Next>,
}

#[cfg(not(feature = "middleware"))]
pub(crate) struct Pipeline {
    endpoints: Endpoints
}

#[cfg(feature = "middleware")]
impl PipelineBuilder {
    pub(crate) fn new() -> Self {
        Self {
            middlewares: Middlewares::new(),
            endpoints: Endpoints::new()
        }
    }
    
    pub(crate) fn build(self) -> Pipeline {
        let start = self.middlewares.compose();
        Pipeline {
            endpoints: self.endpoints,
            start
        }
    }

    pub(crate) fn has_middleware_pipeline(&self) -> bool {
        self.middlewares.is_empty()
    }

    pub(crate) fn middlewares_mut(&mut self) -> &mut Middlewares {
        &mut self.middlewares
    }

    pub(crate) fn endpoints_mut(&mut self) -> &mut Endpoints {
        &mut self.endpoints
    }
}

#[cfg(not(feature = "middleware"))]
impl PipelineBuilder {
    pub(crate) fn new() -> Self {
        Self {
            endpoints: Endpoints::new()
        }
    }

    pub(crate) fn build(self) -> Pipeline {
        Pipeline {
            endpoints: self.endpoints
        }
    }

    pub(crate) fn endpoints_mut(&mut self) -> &mut Endpoints {
        &mut self.endpoints
    }
}

#[cfg(feature = "middleware")]
impl Pipeline {
    pub(crate) fn has_middleware_pipeline(&self) -> bool {
        self.start.is_some()
    }

    #[inline]
    pub(crate) fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    pub(crate) async fn execute(&self, ctx: HttpContext) -> HttpResult {
        let next = &self.start;
        if let Some(next) = next {
            let next: Next = next.clone();
            next(ctx).await
        } else {
            ctx.execute().await
        }
    }
}

#[cfg(not(feature = "middleware"))]
impl Pipeline {
    #[inline]
    pub(crate) fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }
}