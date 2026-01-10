//! Tools for tracing, logging and observability

use futures_util::TryFutureExt;
use tracing::{Instrument, trace_span};
use crate::{
    App, HttpResult, middleware::{HttpContext, NextFn}, 
    error::handler::call_weak_err_handler
};

const DEFAULT_SPAN_HEADER_NAME: &str = "request-id";

/// Represents a tracing configuration
#[derive(Debug, Clone, Copy)]
pub struct TracingConfig {
    /// Specifies whether to include a span id HTTP header
    /// 
    /// Default: `false`
    include_header: bool,
    
    /// Specifies a span id HTTP header name
    /// 
    /// Default: `request-id`
    span_header_name: &'static str,
}

impl Default for TracingConfig {
    #[inline]
    fn default() -> Self {
        Self { 
            include_header: false,
            span_header_name: DEFAULT_SPAN_HEADER_NAME,
        }
    }
}

impl TracingConfig {
    /// Creates a default tracing configuration
    ///
    /// Defaults:
    /// - include_header: `false`
    /// - span_header_name: `request-id`
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Configures tracing to include the span id as an HTTP header
    /// 
    /// Default: `false`
    pub fn with_header(mut self) -> Self {
        self.include_header = true;
        self
    }

    /// Configures tracing to use a specific HTTP header name if the `include_header` is set to `true`
    ///
    /// Default: `request-id`
    pub fn with_header_name(mut self, name: &'static str) -> Self {
        self.span_header_name = name;
        self
    }
}

impl App {
    /// Configures web server with the default Tracing configurations
    ///
    /// Defaults:
    /// - include_header: `false`
    /// - span_header_name: `request-id`
    pub fn with_default_tracing(mut self) -> Self {
        self.tracing_config = Some(TracingConfig::default());
        self
    }

    /// Configures web server with specific Tracing configurations.
    ///
    /// Defaults:
    /// - include_header: `false`
    /// - span_header_name: `request-id`
    /// 
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_tracing(|config| config.with_header());
    /// ```
    /// 
    /// If tracing was already preconfigured, it does not overwrite it
    /// ```no_run
    /// use volga::App;
    /// use volga::tracing::TracingConfig;
    ///
    /// let app = App::new()
    ///     .set_tracing(TracingConfig::new().with_header()) // sets include_header to true 
    ///     .with_tracing(|config| config
    ///         .with_header_name("x-span-id"));               // sets a specific header name, include_header remains true
    /// ```
    pub fn with_tracing<T>(mut self, config: T) -> Self 
    where
        T : FnOnce(TracingConfig) -> TracingConfig
    {
        self.tracing_config = Some(config(self.tracing_config.unwrap_or_default()));
        self
    }
    
    /// Configures web server with specific Tracing configurations
    ///
    /// Defaults:
    /// - include_header: `false`
    /// - span_header_name: `request-id`
    pub fn set_tracing(mut self, config: TracingConfig) -> Self {
        self.tracing_config = Some(config);
        self
    }
    
    /// Adds middleware for wrapping each request into unique [`tracing::Span`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, tracing::TracingConfig};
    ///
    /// let mut app = App::new();
    /// // is equivalent to:
    /// let mut app = App::new().set_tracing(TracingConfig::default());
    ///
    /// // if the tracing middleware is enabled
    /// app.use_tracing(); 
    /// ```
    pub fn use_tracing(&mut self) -> &mut Self {
        let cfg = self.tracing_config
            .take()
            .unwrap_or_default();
        self.wrap(move |ctx, next| wrap_tracing(cfg, ctx, next))
    }
}

async fn wrap_tracing(
    cfg: TracingConfig, 
    ctx: HttpContext, 
    next: NextFn
) -> HttpResult {
    let method = ctx.request().method();
    let uri = ctx.request().uri();

    let span = trace_span!("request", %method, %uri);
    let span_id = span.id();
    
    let parts = ctx.request_parts_snapshot();
    let error_handler = ctx.error_handler();
    let http_result = next(ctx)
        .or_else(|err| async { call_weak_err_handler(error_handler, &parts, err).await })
        .instrument(span)
        .await;

    if cfg.include_header && span_id.is_some() {
        http_result.map(|mut response| {
            response.headers_mut().append(
                cfg.span_header_name,
                span_id.map_or(0, |id| id.into_u64()).into());
            response
        })
    } else {
        http_result
    }
}

#[cfg(test)]
mod tests {
    use crate::App;
    use super::{TracingConfig, DEFAULT_SPAN_HEADER_NAME};
    
    #[test]
    fn it_creates_new() {
        let tracing_config = TracingConfig::new();
        
        assert!(!tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME);
    }

    #[test]
    fn it_creates_default() {
        let tracing_config = TracingConfig::default();

        assert!(!tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME);
    }

    #[test]
    fn it_creates_with_include_header() {
        let tracing_config = TracingConfig::new().with_header();

        assert!(tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME);
    }

    #[test]
    fn it_creates_with_header_name() {
        let tracing_config = TracingConfig::new()
            .with_header()
            .with_header_name("correlation-id");

        assert!(tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, "correlation-id");
    }

    #[test]
    fn it_creates_app_with_default_tracing() {
        let app = App::new().with_default_tracing();
        let tracing_config = app.tracing_config.unwrap();
        
        assert!(!tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME)
    }

    #[test]
    fn it_creates_app_with_span_header() {
        let app = App::new()
            .set_tracing(TracingConfig::new().with_header());
        
        let tracing_config = app.tracing_config.unwrap();

        assert!(tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME)
    }

    #[test]
    fn it_creates_app_with_span_header_name() {
        let app = App::new()
            .with_tracing(|tracing| tracing
                .with_header()
                .with_header_name("correlation-id"));

        let tracing_config = app.tracing_config.unwrap();

        assert!(tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, "correlation-id")
    }
}